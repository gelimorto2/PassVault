use crate::AppStateM;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[cfg(target_os = "android")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use tauri::{plugin::PluginHandle, Manager};

/// What's actually stored behind the biometric gate. Bundling the TOTP
/// secret alongside the password means one fingerprint prompt can supply
/// both factors — the vault file itself is still only decryptable with the
/// password and a valid code, so a leaked vault.enc (e.g. from a synced
/// folder) is no easier to crack; this only changes what "possessing this
/// specific unlocked device" gets you, which is the same trust boundary
/// biometric unlock already implies for the password alone.
#[cfg(any(target_os = "android", target_os = "linux"))]
#[derive(serde::Serialize, serde::Deserialize)]
struct BiometricPayload {
    password: String,
    secret_b32: String,
}

#[cfg(target_os = "android")]
pub struct Biometric<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
#[derive(Serialize)]
struct SecretArg<'a> {
    secret: &'a str,
}

#[cfg(target_os = "android")]
impl<R: Runtime> Biometric<R> {
    pub fn is_available(&self) -> (bool, bool) {
        #[derive(Deserialize)]
        struct Resp {
            available: bool,
            #[serde(rename = "hasSavedSecret")]
            has_saved_secret: bool,
        }
        self.0
            .run_mobile_plugin::<Resp>("isAvailable", ())
            .map(|r| (r.available, r.has_saved_secret))
            .unwrap_or((false, false))
    }

    pub fn save_secret(&self, secret: &str) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("saveSecret", SecretArg { secret })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn load_secret(&self) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Resp {
            secret: String,
        }
        self.0
            .run_mobile_plugin::<Resp>("loadSecret", ())
            .map(|r| r.secret)
            .map_err(|e| e.to_string())
    }

    pub fn clear_secret(&self) {
        let _ = self.0.run_mobile_plugin::<serde_json::Value>("clearSecret", ());
    }
}

/// Bridges to the local `BiometricPlugin.kt` (see gen/android): stores the
/// master password behind an Android-Keystore key that requires biometric
/// auth to use, so unlocking can skip typing the password (the TOTP code is
/// still required — biometrics replace the password, not the second
/// factor). No-op on desktop.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("biometric")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("com.leo.passvault.biometric", "BiometricPlugin")?;
                _app.manage(Biometric(handle));
            }
            Ok(())
        })
        .build()
}

#[derive(serde::Serialize)]
pub struct BiometricStatus {
    available: bool,
    has_saved_secret: bool,
    /// "fingerprint" (Android) or "face" (Linux + Howdy) — drives UI wording.
    kind: &'static str,
    /// Why it's unavailable, when we can tell (e.g. Howdy has no face model).
    reason: Option<String>,
}

#[tauri::command]
pub async fn biometric_status(app: tauri::AppHandle) -> BiometricStatus {
    #[cfg(target_os = "android")]
    {
        if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
            let (available, has_saved_secret) = b.is_available();
            return BiometricStatus {
                available,
                has_saved_secret,
                kind: "fingerprint",
                reason: None,
            };
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = &app;
        return tauri::async_runtime::spawn_blocking(|| {
            let (available, reason) = match face::howdy() {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e)),
            };
            BiometricStatus {
                available,
                has_saved_secret: available && face::has_secret(),
                kind: "face",
                reason,
            }
        })
        .await
        .unwrap_or(BiometricStatus {
            available: false,
            has_saved_secret: false,
            kind: "face",
            reason: None,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = app;
        BiometricStatus {
            available: false,
            has_saved_secret: false,
            kind: "none",
            reason: None,
        }
    }
}

/// Called right after a normal password+code unlock succeeds, if the user
/// opts in, to wrap the password and TOTP secret behind biometrics so a
/// single fingerprint/face check can supply both factors next time.
#[tauri::command]
pub async fn enable_biometric_unlock(
    app: tauri::AppHandle,
    state: State<'_, AppStateM>,
    password: String,
) -> Result<(), String> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
        let secret_b32 = {
            let s = state.lock().unwrap();
            crate::vault::read_device_secret(&s.device_path, &password)
                .map_err(|_| "Wrong master password.".to_string())?
        };
        let payload = BiometricPayload {
            password,
            secret_b32,
        };
        let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        #[cfg(target_os = "android")]
        {
            if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
                return b.save_secret(&json);
            }
            return Err("Biometric unlock isn't available on this device.".into());
        }
        #[cfg(target_os = "linux")]
        {
            let _ = app;
            return tauri::async_runtime::spawn_blocking(move || {
                // Prove Howdy actually recognises the person enabling it.
                face::verify_face()?;
                face::save_secret(&json)
            })
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    #[allow(unreachable_code)]
    {
        let _ = (app, state, password);
        Err("Biometric unlock isn't available on this platform.".into())
    }
}

#[tauri::command]
pub async fn disable_biometric_unlock(app: tauri::AppHandle) {
    disable_biometric_unlock_inner(&app);
}

pub fn disable_biometric_unlock_inner(app: &tauri::AppHandle) {
    #[cfg(target_os = "android")]
    {
        if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
            b.clear_secret();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        face::clear_secret();
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let _ = app;
}

/// Prompts biometrics once and, on success, unlocks the vault directly —
/// no password or code typing. The TOTP code is computed locally from the
/// biometric-gated secret rather than skipped, so it still goes through the
/// exact same verification `unlock` does.
#[tauri::command]
pub async fn biometric_unlock(
    app: tauri::AppHandle,
    state: State<'_, AppStateM>,
) -> Result<Vec<crate::vault::Account>, String> {
    #[cfg(target_os = "android")]
    {
        let Some(b) = app.try_state::<Biometric<tauri::Wry>>() else {
            return Err("Biometric unlock isn't available on this device.".into());
        };
        let json = b.load_secret()?;
        let r = unlock_with_payload(&state, &json);
        if is_stale(&r) {
            b.clear_secret();
        }
        return r.map_err(stale_msg);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        let app2 = app.clone();
        let json = tauri::async_runtime::spawn_blocking(move || {
            face::verify_face()?;
            use tauri::Emitter;
            let _ = app2.emit("unlock-progress", "Recognised — unlocking…");
            face::load_secret()
        })
        .await
        .map_err(|e| e.to_string())??;
        let r = unlock_with_payload(&state, &json);
        if is_stale(&r) {
            face::clear_secret();
        }
        return r.map_err(stale_msg);
    }
    #[allow(unreachable_code)]
    {
        let _ = (app, state);
        Err("Biometric unlock isn't available on this platform.".into())
    }
}

/// Saved biometric data that no longer opens the vault (master password
/// changed, vault replaced/imported, …) is useless — drop it so the next
/// launch goes straight to the password form instead of failing forever.
#[cfg(any(target_os = "android", target_os = "linux"))]
fn is_stale<T>(r: &Result<T, String>) -> bool {
    matches!(r, Err(e) if e.contains("Wrong master password") || e.contains("corrupted"))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn stale_msg(e: String) -> String {
    if e.contains("Wrong master password") || e.contains("corrupted") {
        "Saved unlock data was out of date and has been removed. Unlock with your password, then turn it back on in Settings.".into()
    } else {
        e
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn unlock_with_payload(state: &State<'_, AppStateM>, json: &str) -> Result<Vec<crate::vault::Account>, String> {
    let payload: BiometricPayload = serde_json::from_str(json)
        .map_err(|_| "Saved unlock data is corrupted; re-enable it in Settings.".to_string())?;
    let secret_bytes = crate::totp::from_base32(&payload.secret_b32)
        .ok_or("Saved unlock data is corrupted; re-enable it in Settings.")?;
    let code = crate::totp::current_code(&secret_bytes);
    crate::do_unlock(state, &payload.password, &code)
}

/// Linux face unlock via Howdy (https://github.com/boltgolt/howdy).
///
/// Howdy only answers "is this the enrolled user, yes/no" — it can't hand
/// out a key. So, mirroring the Android design, the master password + TOTP
/// secret are kept in the desktop keyring (GNOME Keyring / KWallet via the
/// Secret Service API, encrypted at rest with your login password) and are
/// only read after Howdy reports a match. This is a convenience gate, not a
/// cryptographic one: any program running as you could read that keyring
/// entry, which is why it's opt-in.
#[cfg(target_os = "linux")]
pub mod face {
    use dbus_secret_service::{EncryptionType, SecretService};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::process::Command;

    const ATTRS: [(&str, &str); 2] = [("application", "com.leo.passvault"), ("purpose", "face-unlock")];

    fn user() -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_default()
    }

    pub struct Howdy {
        python: PathBuf,
        compare: PathBuf,
    }

    pub fn howdy() -> Result<Howdy, String> {
        let roots = [
            "/usr/local/lib/x86_64-linux-gnu/howdy",
            "/usr/lib/x86_64-linux-gnu/howdy",
            "/usr/local/lib/aarch64-linux-gnu/howdy",
            "/usr/lib/aarch64-linux-gnu/howdy",
            "/usr/lib/security/howdy",
            "/lib/security/howdy",
            "/usr/lib64/security/howdy",
        ];
        let compare = roots
            .iter()
            .map(|r| PathBuf::from(r).join("compare.py"))
            .find(|p| p.is_file())
            .ok_or("Howdy isn't installed.")?;
        let python = ["/usr/bin/python3", "/usr/bin/python"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or("Python 3 isn't installed.")?;
        let u = user();
        let has_model = [
            "/usr/local/etc/howdy/models",
            "/etc/howdy/models",
            "/usr/lib/security/howdy/models",
            "/lib/security/howdy/models",
        ]
        .iter()
        .any(|d| PathBuf::from(d).join(format!("{u}.dat")).is_file());
        if !has_model {
            return Err(format!("Howdy has no face model for {u} yet — run: sudo howdy add"));
        }
        Ok(Howdy { python, compare })
    }

    /// Runs Howdy's own matcher (same thing the login screen uses). Exit 0 =
    /// match; Howdy's documented codes map to friendlier messages.
    pub fn verify_face() -> Result<(), String> {
        let h = howdy()?;
        let out = Command::new(&h.python)
            .arg(&h.compare)
            .arg(user())
            // AppImage/flatpak runtimes leak these into children and break
            // the system Python that Howdy needs.
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .output()
            .map_err(|e| format!("Couldn't start Howdy: {e}"))?;
        let code = out.status.code();
        if code == Some(0) {
            return Ok(());
        }
        // Howdy's own last words (e.g. "Failed to read camera", a Python
        // traceback's final line, "No face model known…") so the user sees
        // the real reason, not just our summary.
        let raw = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let detail: String = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .last()
            .unwrap_or("")
            .chars()
            .take(240)
            .collect();
        let summary = match code {
            Some(10) => "Howdy has no face model for you — run: sudo howdy add".to_string(),
            Some(11) => "Face not recognised. Look at the camera and try again.".to_string(),
            Some(13) => "Too dark for the camera — add some light and try again.".to_string(),
            Some(1) => "Howdy couldn't use the camera.".to_string(),
            Some(c) => format!("Face check failed (Howdy exit code {c})."),
            None => "Face check was interrupted.".to_string(),
        };
        Err(if detail.is_empty() {
            summary
        } else {
            format!("{summary}\nHowdy: {detail}")
        })
    }

    fn connect() -> Result<SecretService, String> {
        SecretService::connect(EncryptionType::Dh)
            .map_err(|_| "Couldn't reach the desktop keyring (GNOME Keyring / KWallet).".to_string())
    }

    pub fn has_secret() -> bool {
        let Ok(ss) = connect() else { return false };
        let has = ss
            .search_items(HashMap::from(ATTRS))
            .map(|r| !r.unlocked.is_empty() || !r.locked.is_empty())
            .unwrap_or(false);
        has
    }

    pub fn save_secret(json: &str) -> Result<(), String> {
        let ss = connect()?;
        let coll = ss.get_default_collection().map_err(|e| e.to_string())?;
        let _ = coll.unlock();
        coll.create_item(
            "PassVault face unlock",
            HashMap::from(ATTRS),
            json.as_bytes(),
            true,
            "text/plain",
        )
        .map_err(|e| format!("Couldn't save to the keyring: {e}"))?;
        Ok(())
    }

    pub fn load_secret() -> Result<String, String> {
        let ss = connect()?;
        let found = ss.search_items(HashMap::from(ATTRS)).map_err(|e| e.to_string())?;
        let item = found
            .unlocked
            .into_iter()
            .chain(found.locked)
            .next()
            .ok_or("Face unlock isn't set up on this PC.")?;
        let _ = item.unlock();
        let bytes = item.get_secret().map_err(|e| format!("Keyring refused: {e}"))?;
        String::from_utf8(bytes).map_err(|_| "Saved unlock data is corrupted.".into())
    }

    pub fn clear_secret() {
        if let Ok(ss) = connect() {
            if let Ok(found) = ss.search_items(HashMap::from(ATTRS)) {
                for item in found.unlocked.into_iter().chain(found.locked) {
                    let _ = item.delete();
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod face_tests {
    /// PV_FACE_CLEAR=1 cargo test --lib face_keyring_state -- --ignored --nocapture
    #[test]
    #[ignore]
    fn face_keyring_state() {
        println!("face secret present: {}", super::face::has_secret());
        if std::env::var("PV_FACE_CLEAR").is_ok() {
            super::face::clear_secret();
            println!("after clear: {}", super::face::has_secret());
        }
    }
}
