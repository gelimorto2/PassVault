mod biometric;
mod crypto;
mod state;
mod storage_access;
mod totp;
mod vault;

use serde::Serialize;
use state::{AppState, PendingSetup, PointerConfig, Session};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};
use vault::{Account, VaultData};

type AppStateM = Mutex<AppState>;

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_millis() as i64
}

fn load_pointer(config_path: &PathBuf) -> PointerConfig {
    fs::read(config_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_pointer(config_path: &PathBuf, pointer: &PointerConfig) {
    if let Ok(json) = serde_json::to_vec_pretty(pointer) {
        let _ = fs::write(config_path, json);
    }
}

#[derive(Serialize)]
struct AppStatus {
    vault_exists: bool,
    device_linked: bool,
    unlocked: bool,
    vault_path: String,
}

#[tauri::command]
fn app_status(state: State<AppStateM>) -> AppStatus {
    let s = state.lock().unwrap();
    AppStatus {
        vault_exists: s.vault_path.exists(),
        device_linked: s.device_path.exists(),
        unlocked: s.session.is_some(),
        vault_path: s.vault_path.to_string_lossy().to_string(),
    }
}

#[cfg(desktop)]
#[tauri::command]
async fn choose_vault_folder(app: tauri::AppHandle, state: State<'_, AppStateM>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    if state.lock().unwrap().vault_path.exists() {
        return Err("Vault already exists; choose the folder before running setup.".into());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let picked = rx.recv().map_err(|_| "dialog closed unexpectedly".to_string())?;

    let Some(folder) = picked else {
        return Ok(None);
    };
    let folder_path: PathBuf = folder
        .into_path()
        .map_err(|e| format!("invalid folder path: {e}"))?;
    let new_vault_path = folder_path.join("vault.enc");

    let mut s = state.lock().unwrap();
    s.vault_path = new_vault_path.clone();
    save_pointer(
        &s.config_path,
        &PointerConfig {
            vault_path: Some(new_vault_path.to_string_lossy().to_string()),
        },
    );
    Ok(Some(new_vault_path.to_string_lossy().to_string()))
}

// Android's storage sandboxing means there's no plain folder-picker API like
// on desktop; picking a synced folder isn't available on mobile.
#[cfg(mobile)]
#[tauri::command]
async fn choose_vault_folder(_app: tauri::AppHandle, _state: State<'_, AppStateM>) -> Result<Option<String>, String> {
    Err("Choosing a vault folder isn't supported on mobile yet.".into())
}

/// Sends the encrypted vault file out through the OS share/save sheet, so it
/// can land in whatever sync tool (Syncthing, Dropbox, etc.) or cloud drive
/// the user picks. Works the same on desktop and mobile.
#[tauri::command]
async fn export_vault(app: tauri::AppHandle, state: State<'_, AppStateM>) -> Result<bool, String> {
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let vault_path = state.lock().unwrap().vault_path.clone();
    if !vault_path.exists() {
        return Err("No vault to export yet.".into());
    }
    let bytes = fs::read(&vault_path).map_err(|e| format!("could not read vault: {e}"))?;

    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_file_name("vault.enc")
        .save_file(move |file| {
            let _ = tx.send(file);
        });
    let picked = rx.recv().map_err(|_| "dialog closed unexpectedly".to_string())?;
    let Some(dest) = picked else {
        return Ok(false);
    };

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut file = app
        .fs()
        .open(dest, opts)
        .map_err(|e| format!("could not open destination: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("could not write file: {e}"))?;
    Ok(true)
}

/// Renders the vault file as a scannable QR code for quick device-to-device
/// sync without a cloud intermediary. Vault files are stored as compact JSON
/// (base64 blob inside), which is what makes this feasible at all — QR
/// codes top out around ~2-3KB, so this only works for vaults small enough
/// to fit; bigger ones need the file-based Export instead.
#[tauri::command]
fn export_vault_qr(state: State<AppStateM>) -> Result<String, String> {
    let vault_path = state.lock().unwrap().vault_path.clone();
    if !vault_path.exists() {
        return Err("No vault to export yet.".into());
    }
    let bytes = fs::read(&vault_path).map_err(|e| format!("could not read vault: {e}"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Vault file is corrupted.".to_string())?;
    let compact = serde_json::to_string(&value).map_err(|e| e.to_string())?;

    use qrcode::render::svg;
    use qrcode::QrCode;
    let code = QrCode::new(compact.as_bytes()).map_err(|_| {
        "This vault has too many accounts to fit in a QR code — use \"Export vault…\" instead."
            .to_string()
    })?;
    Ok(code
        .render()
        .min_dimensions(280, 280)
        .dark_color(svg::Color("#1a1a1a"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

/// Installs a vault scanned from [`export_vault_qr`] on another device.
#[tauri::command]
async fn import_vault_qr(
    app: tauri::AppHandle,
    content: String,
    state: State<'_, AppStateM>,
) -> Result<AppStatus, String> {
    let status = install_imported_vault(&state, content.into_bytes())?;
    biometric::disable_biometric_unlock(app);
    Ok(status)
}

/// Reads a vault file picked from anywhere the OS file picker can reach
/// (a synced folder, Downloads, a cloud drive) and replaces the local vault
/// with it. The device still needs to be linked (or the vault unlocked)
/// with the master password afterwards, same as any fresh device.
#[tauri::command]
async fn import_vault(
    app: tauri::AppHandle,
    state: State<'_, AppStateM>,
) -> Result<Option<AppStatus>, String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_fs::FsExt;

    // No add_filter() here: on Android the picker maps filters to MIME types,
    // and ".enc" isn't a recognized one, which hides matching files instead
    // of showing them. Any file can be picked; contents are validated below.
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_file(move |file| {
        let _ = tx.send(file);
    });
    let picked = rx.recv().map_err(|_| "dialog closed unexpectedly".to_string())?;
    let Some(src) = picked else {
        return Ok(None);
    };

    let bytes = app
        .fs()
        .read(src)
        .map_err(|e| format!("could not read file: {e}"))?;
    let status = install_imported_vault(&state, bytes)?;
    biometric::disable_biometric_unlock(app);
    Ok(Some(status))
}

/// Imports a vault file found by [`storage_access::scan_for_vault_files`] —
/// same validation and install path as [`import_vault`], just skipping the
/// file picker since we already have a full absolute path.
#[tauri::command]
async fn import_vault_from_path(
    app: tauri::AppHandle,
    path: String,
    state: State<'_, AppStateM>,
) -> Result<AppStatus, String> {
    let bytes = fs::read(&path).map_err(|e| format!("could not read file: {e}"))?;
    let status = install_imported_vault(&state, bytes)?;
    biometric::disable_biometric_unlock(app);
    Ok(status)
}

fn install_imported_vault(state: &State<'_, AppStateM>, bytes: Vec<u8>) -> Result<AppStatus, String> {
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "That doesn't look like a PassVault vault file.".to_string())?;
    if value.get("version").is_none() || value.get("blob").is_none() {
        return Err("That doesn't look like a PassVault vault file.".into());
    }

    let mut s = state.lock().unwrap();
    s.session = None;
    if let Some(parent) = s.vault_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("could not create vault folder: {e}"))?;
    }
    fs::write(&s.vault_path, &bytes).map_err(|e| format!("could not write vault: {e}"))?;

    Ok(AppStatus {
        vault_exists: s.vault_path.exists(),
        device_linked: s.device_path.exists(),
        unlocked: false,
        vault_path: s.vault_path.to_string_lossy().to_string(),
    })
}

#[derive(Serialize)]
struct SetupResp {
    secret_b32: String,
    otpauth_uri: String,
    qr_svg: String,
}

#[tauri::command]
fn start_setup(state: State<AppStateM>, password: String) -> Result<SetupResp, String> {
    let mut s = state.lock().unwrap();
    if s.vault_path.exists() {
        return Err("A vault already exists at this location.".into());
    }
    if password.len() < 8 {
        return Err("Master password must be at least 8 characters.".into());
    }

    let secret = totp::generate_secret();
    let secret_b32 = totp::to_base32(&secret);
    let otpauth_uri = totp::otpauth_uri(&secret_b32, "PassVault", "vault");
    let qr_svg = totp::qr_svg(&otpauth_uri);

    s.pending_setup = Some(PendingSetup {
        secret_b32: secret_b32.clone(),
        password,
    });

    Ok(SetupResp {
        secret_b32,
        otpauth_uri,
        qr_svg,
    })
}

/// Re-derives the QR/secret display for a device that's already linked, so a
/// lost secret key can be recovered as long as at least one linked device
/// and the master password are still available.
#[tauri::command]
fn reveal_totp_secret(state: State<AppStateM>, password: String) -> Result<SetupResp, String> {
    let s = state.lock().unwrap();
    if !s.device_path.exists() {
        return Err("This device isn't linked yet.".into());
    }
    let secret_b32 = vault::read_device_secret(&s.device_path, &password)
        .map_err(|_| "Wrong master password.".to_string())?;
    let otpauth_uri = totp::otpauth_uri(&secret_b32, "PassVault", "vault");
    let qr_svg = totp::qr_svg(&otpauth_uri);
    Ok(SetupResp {
        secret_b32,
        otpauth_uri,
        qr_svg,
    })
}

#[tauri::command]
fn confirm_setup(state: State<AppStateM>, code: String) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    let pending = s
        .pending_setup
        .take()
        .ok_or("No setup in progress. Start setup first.")?;

    let secret_bytes = totp::from_base32(&pending.secret_b32).ok_or("Invalid secret")?;
    if !totp::verify(&secret_bytes, &code) {
        s.pending_setup = Some(pending);
        return Err("That code didn't match. Check your authenticator app and try again.".into());
    }

    let empty = VaultData::default();
    vault::write_vault(&s.vault_path, &pending.password, &empty)
        .map_err(|e| format!("could not write vault: {e}"))?;
    vault::write_device_secret(&s.device_path, &pending.password, &pending.secret_b32)
        .map_err(|e| format!("could not write device file: {e}"))?;

    save_pointer(
        &s.config_path,
        &PointerConfig {
            vault_path: Some(s.vault_path.to_string_lossy().to_string()),
        },
    );

    let (data, key, salt) = vault::read_vault_with_key(&s.vault_path, &pending.password)?;
    s.session = Some(Session {
        key,
        salt,
        accounts: data.accounts.clone(),
    });

    Ok(data.accounts)
}

#[tauri::command]
fn link_device(
    state: State<AppStateM>,
    password: String,
    secret_b32: String,
    code: String,
) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    if !s.vault_path.exists() {
        return Err("No vault found at this location yet. Run setup instead.".into());
    }

    let secret_b32 = secret_b32.trim().to_string();
    let secret_bytes =
        totp::from_base32(&secret_b32).ok_or("That doesn't look like a valid secret key.")?;
    if !totp::verify(&secret_bytes, &code) {
        return Err("That code didn't match the secret key.".into());
    }

    let (data, key, salt) = vault::read_vault_with_key(&s.vault_path, &password)
        .map_err(|_| "Wrong master password.".to_string())?;

    vault::write_device_secret(&s.device_path, &password, &secret_b32)
        .map_err(|e| format!("could not write device file: {e}"))?;

    s.session = Some(Session {
        key,
        salt,
        accounts: data.accounts.clone(),
    });

    Ok(data.accounts)
}

#[tauri::command]
fn unlock(state: State<AppStateM>, password: String, code: String) -> Result<Vec<Account>, String> {
    do_unlock(&state, &password, &code)
}

fn do_unlock(state: &State<AppStateM>, password: &str, code: &str) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    if !s.vault_path.exists() || !s.device_path.exists() {
        return Err("This device isn't set up yet.".into());
    }

    let secret_b32 = vault::read_device_secret(&s.device_path, password)
        .map_err(|_| "Wrong master password.".to_string())?;
    let secret_bytes = totp::from_base32(&secret_b32).ok_or("Corrupted device secret")?;
    if !totp::verify(&secret_bytes, code) {
        return Err("Invalid authenticator code.".into());
    }

    let (data, key, salt) = vault::read_vault_with_key(&s.vault_path, password)
        .map_err(|_| "Wrong master password or corrupted vault.".to_string())?;

    s.session = Some(Session {
        key,
        salt,
        accounts: data.accounts.clone(),
    });

    Ok(data.accounts)
}

#[tauri::command]
fn lock(state: State<AppStateM>) {
    let mut s = state.lock().unwrap();
    s.session = None;
}

#[tauri::command]
fn list_accounts(state: State<AppStateM>) -> Result<Vec<Account>, String> {
    let s = state.lock().unwrap();
    let session = s.session.as_ref().ok_or("Vault is locked.")?;
    Ok(session.accounts.clone())
}

fn persist_session(s: &mut AppState) -> Result<(), String> {
    let session = s.session.as_ref().ok_or("Vault is locked.")?;
    let data = VaultData {
        accounts: session.accounts.clone(),
    };
    vault::write_vault_with_key(&s.vault_path, &session.key, &session.salt, &data)
        .map_err(|e| format!("could not save vault: {e}"))
}

#[tauri::command]
fn add_account(
    state: State<AppStateM>,
    name: String,
    username: String,
    password: String,
    url: String,
    notes: String,
) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    let ts = now_millis();
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        username,
        password,
        url,
        notes,
        created_at: ts,
        updated_at: ts,
    };
    {
        let session = s.session.as_mut().ok_or("Vault is locked.")?;
        session.accounts.push(account);
    }
    persist_session(&mut s)?;
    Ok(s.session.as_ref().unwrap().accounts.clone())
}

#[tauri::command]
fn update_account(
    state: State<AppStateM>,
    id: String,
    name: String,
    username: String,
    password: String,
    url: String,
    notes: String,
) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    {
        let session = s.session.as_mut().ok_or("Vault is locked.")?;
        let account = session
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found.")?;
        account.name = name;
        account.username = username;
        account.password = password;
        account.url = url;
        account.notes = notes;
        account.updated_at = now_millis();
    }
    persist_session(&mut s)?;
    Ok(s.session.as_ref().unwrap().accounts.clone())
}

#[tauri::command]
fn delete_account(state: State<AppStateM>, id: String) -> Result<Vec<Account>, String> {
    let mut s = state.lock().unwrap();
    {
        let session = s.session.as_mut().ok_or("Vault is locked.")?;
        session.accounts.retain(|a| a.id != id);
    }
    persist_session(&mut s)?;
    Ok(s.session.as_ref().unwrap().accounts.clone())
}

#[tauri::command]
fn reveal_vault_folder(app: tauri::AppHandle, state: State<AppStateM>) -> Result<(), String> {
    let s = state.lock().unwrap();
    let folder = s
        .vault_path
        .parent()
        .ok_or("No vault folder set.")?
        .to_path_buf();
    // Ensure the folder exists before asking the OS to open it (e.g. right
    // after choosing a folder during setup, before the vault is written).
    let _ = fs::create_dir_all(&folder);
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(folder.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("could not open folder: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(storage_access::init())
        .plugin(biometric::init());
    // The barcode-scanner crate only exists on mobile (`#![cfg(mobile)]`);
    // referencing it unconditionally would fail to compile on desktop.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    builder
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("app config dir resolvable");
            fs::create_dir_all(&config_dir).expect("can create app config dir");

            let config_path = config_dir.join("config.json");
            let device_path = config_dir.join("device.json");
            let pointer = load_pointer(&config_path);
            let vault_path = pointer
                .vault_path
                .map(PathBuf::from)
                .unwrap_or_else(|| config_dir.join("PassVault").join("vault.enc"));

            app.manage(Mutex::new(AppState {
                vault_path,
                device_path,
                config_path,
                pending_setup: None,
                session: None,
            }));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            choose_vault_folder,
            start_setup,
            confirm_setup,
            link_device,
            unlock,
            lock,
            list_accounts,
            add_account,
            update_account,
            delete_account,
            reveal_vault_folder,
            export_vault,
            export_vault_qr,
            import_vault_qr,
            import_vault,
            import_vault_from_path,
            reveal_totp_secret,
            storage_access::has_full_file_access,
            storage_access::request_full_file_access,
            storage_access::scan_for_vault_files,
            biometric::biometric_status,
            biometric::enable_biometric_unlock,
            biometric::disable_biometric_unlock,
            biometric::biometric_unlock,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Auto-lock as soon as the app is backgrounded (Android's
            // onPause), not just on an explicit Lock tap or process death.
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Suspended,
                ..
            } = &event
            {
                if let Some(state) = app_handle.try_state::<AppStateM>() {
                    state.lock().unwrap().session = None;
                }
            }
        });
}
