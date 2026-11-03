use crate::AppStateM;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[cfg(target_os = "android")]
use crate::vault;
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
#[cfg(target_os = "android")]
#[derive(Serialize, Deserialize)]
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
}

#[tauri::command]
pub fn biometric_status(app: tauri::AppHandle) -> BiometricStatus {
    #[cfg(target_os = "android")]
    {
        if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
            let (available, has_saved_secret) = b.is_available();
            return BiometricStatus {
                available,
                has_saved_secret,
            };
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
    BiometricStatus {
        available: false,
        has_saved_secret: false,
    }
}

/// Called right after a normal password+code unlock succeeds, if the user
/// opts in, to wrap the password and TOTP secret behind biometrics so a
/// single fingerprint prompt can supply both factors next time.
#[tauri::command]
pub fn enable_biometric_unlock(
    app: tauri::AppHandle,
    state: State<'_, AppStateM>,
    password: String,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let secret_b32 = {
            let s = state.lock().unwrap();
            vault::read_device_secret(&s.device_path, &password)
                .map_err(|_| "Wrong master password.".to_string())?
        };
        let payload = BiometricPayload {
            password,
            secret_b32,
        };
        let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
            return b.save_secret(&json);
        }
        Err("Biometric unlock isn't available on this device.".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, state, password);
        Err("Biometric unlock is only available on mobile.".into())
    }
}

#[tauri::command]
pub fn disable_biometric_unlock(app: tauri::AppHandle) {
    #[cfg(target_os = "android")]
    {
        if let Some(b) = app.try_state::<Biometric<tauri::Wry>>() {
            b.clear_secret();
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

/// Prompts biometrics once and, on success, unlocks the vault directly —
/// no password or code typing. The TOTP code is computed locally from the
/// biometric-gated secret rather than skipped, so it still goes through the
/// exact same verification `unlock` does.
#[tauri::command]
pub fn biometric_unlock(
    app: tauri::AppHandle,
    state: State<'_, AppStateM>,
) -> Result<Vec<crate::vault::Account>, String> {
    #[cfg(target_os = "android")]
    {
        let Some(b) = app.try_state::<Biometric<tauri::Wry>>() else {
            return Err("Biometric unlock isn't available on this device.".into());
        };
        let json = b.load_secret()?;
        let payload: BiometricPayload = serde_json::from_str(&json)
            .map_err(|_| "Saved fingerprint data is corrupted; re-enable it.".to_string())?;
        let secret_bytes = crate::totp::from_base32(&payload.secret_b32)
            .ok_or("Saved fingerprint data is corrupted; re-enable it.")?;
        let code = crate::totp::current_code(&secret_bytes);
        crate::do_unlock(&state, &payload.password, &code)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, state);
        Err("Biometric unlock is only available on mobile.".into())
    }
}
