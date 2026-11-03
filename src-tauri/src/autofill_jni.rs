//! JNI entry point for the Android autofill service (`autofill/` Kotlin).
//!
//! The autofill flow runs while the main app may be closed, so it can't go
//! through Tauri's IPC. After the user passes the fingerprint prompt, the
//! Kotlin side holds the same `{password, secret_b32}` payload biometric
//! unlock uses and calls this function to decrypt the vault with the exact
//! same Rust crypto. Plaintext only ever lives in that short-lived process
//! memory and the datasets handed to the Android autofill framework.

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Payload {
    password: String,
    secret_b32: String,
}

#[derive(serde::Serialize)]
struct Out {
    ok: bool,
    error: Option<String>,
    accounts: Vec<OutAccount>,
}

#[derive(serde::Serialize)]
struct OutAccount {
    name: String,
    username: String,
    password: String,
    url: String,
}

fn run(config_dir: &str, payload_json: &str) -> Result<Vec<OutAccount>, String> {
    let payload: Payload =
        serde_json::from_str(payload_json).map_err(|_| "Saved fingerprint data is corrupted.")?;
    let config_dir = PathBuf::from(config_dir);
    let pointer: crate::state::PointerConfig = std::fs::read(config_dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let vault_path = pointer
        .vault_path
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.join("PassVault").join("vault.enc"));

    // Same two checks as a normal unlock: the password opens this device's
    // secret file, and that secret matches the one stored with the payload.
    let device_secret = crate::vault::read_device_secret(&config_dir.join("device.json"), &payload.password)
        .map_err(|_| "Master password changed — open PassVault and re-enable fingerprint unlock.")?;
    if device_secret.trim() != payload.secret_b32.trim() {
        return Err("Device key mismatch — open PassVault and re-enable fingerprint unlock.".into());
    }
    let (data, _key, _salt) = crate::vault::read_vault_with_key(&vault_path, &payload.password)?;
    Ok(data
        .accounts
        .into_iter()
        .map(|a| OutAccount {
            name: a.name,
            username: a.username,
            password: a.password,
            url: a.url,
        })
        .collect())
}

#[no_mangle]
pub extern "system" fn Java_com_leo_passvault_autofill_NativeVault_decrypt<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    config_dir: JString<'local>,
    payload_json: JString<'local>,
) -> jstring {
    let config_dir: String = env.get_string(&config_dir).map(Into::into).unwrap_or_default();
    let payload_json: String = env.get_string(&payload_json).map(Into::into).unwrap_or_default();
    let out = match std::panic::catch_unwind(|| run(&config_dir, &payload_json)) {
        Ok(Ok(accounts)) => Out { ok: true, error: None, accounts },
        Ok(Err(e)) => Out { ok: false, error: Some(e), accounts: vec![] },
        Err(_) => Out { ok: false, error: Some("Internal error.".into()), accounts: vec![] },
    };
    let json = serde_json::to_string(&out).unwrap_or_else(|_| "{\"ok\":false}".into());
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
