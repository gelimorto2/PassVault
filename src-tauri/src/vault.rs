use crate::crypto::{self, EncryptedBlob};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Default)]
pub struct VaultData {
    pub accounts: Vec<Account>,
}

#[derive(Serialize, Deserialize)]
struct VaultFile {
    version: u8,
    blob: EncryptedBlob,
}

#[derive(Serialize, Deserialize)]
struct DeviceFile {
    version: u8,
    blob: EncryptedBlob,
}

#[derive(Serialize, Deserialize)]
struct DeviceData {
    totp_secret_b32: String,
}

pub fn write_vault(path: &Path, password: &str, data: &VaultData) -> std::io::Result<()> {
    let json = serde_json::to_vec(data).expect("vault data always serializes");
    let blob = crypto::encrypt(password, &json);
    let file = VaultFile { version: 1, blob };
    let contents = serde_json::to_vec_pretty(&file).expect("vault file always serializes");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

/// Reads and decrypts the vault, also returning the derived key and salt so
/// the caller can hold them in the unlocked session and reuse them for fast
/// re-saves without re-running Argon2id on every account edit.
pub fn read_vault_with_key(
    path: &Path,
    password: &str,
) -> Result<(VaultData, [u8; 32], Vec<u8>), String> {
    let contents = fs::read(path).map_err(|e| format!("could not read vault file: {e}"))?;
    let file: VaultFile =
        serde_json::from_slice(&contents).map_err(|_| "vault file is corrupted".to_string())?;
    let salt = crypto::salt_from_blob(&file.blob);
    let key = crypto::derive_key(password, &salt);
    let plaintext = crypto::decrypt_with_key(&key, &file.blob)
        .map_err(|_| "wrong master password or corrupted vault".to_string())?;
    let data = serde_json::from_slice(&plaintext)
        .map_err(|_| "vault contents are corrupted".to_string())?;
    Ok((data, key, salt))
}

pub fn write_vault_with_key(
    path: &Path,
    key: &[u8; 32],
    salt: &[u8],
    data: &VaultData,
) -> std::io::Result<()> {
    let json = serde_json::to_vec(data).expect("vault data always serializes");
    let blob = crypto::encrypt_with_key(key, salt, &json);
    let file = VaultFile { version: 1, blob };
    let contents = serde_json::to_vec_pretty(&file).expect("vault file always serializes");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

pub fn write_device_secret(path: &Path, password: &str, secret_b32: &str) -> std::io::Result<()> {
    let data = DeviceData {
        totp_secret_b32: secret_b32.to_string(),
    };
    let json = serde_json::to_vec(&data).expect("device data always serializes");
    let blob = crypto::encrypt(password, &json);
    let file = DeviceFile { version: 1, blob };
    let contents = serde_json::to_vec_pretty(&file).expect("device file always serializes");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

pub fn read_device_secret(path: &Path, password: &str) -> Result<String, String> {
    let contents = fs::read(path).map_err(|e| format!("could not read device file: {e}"))?;
    let file: DeviceFile =
        serde_json::from_slice(&contents).map_err(|_| "device file is corrupted".to_string())?;
    let plaintext = crypto::decrypt(password, &file.blob)
        .map_err(|_| "wrong master password".to_string())?;
    let data: DeviceData =
        serde_json::from_slice(&plaintext).map_err(|_| "device file is corrupted".to_string())?;
    Ok(data.totp_secret_b32)
}
