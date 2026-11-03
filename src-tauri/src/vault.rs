use crate::crypto::{self, EncryptedBlob};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Current on-disk format. v1 files (0.1.0) have the same envelope and an
/// inner payload without `tombstones`/`favorite`/`tags`; every new field is
/// `#[serde(default)]`, so v1 vaults open unchanged. 0.1.0 builds can also
/// still read v2 files (serde ignores unknown fields), they just drop the
/// new fields when they save.
pub const VAULT_FORMAT: u8 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Marks a deleted account so sync doesn't resurrect it from another device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tombstone {
    pub id: String,
    pub deleted_at: i64,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct VaultData {
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
}

#[derive(Serialize, Deserialize)]
pub struct VaultFile {
    pub version: u8,
    pub blob: EncryptedBlob,
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

/// Writes atomically (temp file + rename) so a crash or a sync tool reading
/// mid-write never sees a half-written vault.
fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("enc.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

pub fn write_vault(path: &Path, password: &str, data: &VaultData) -> std::io::Result<()> {
    let json = serde_json::to_vec(data).expect("vault data always serializes");
    let blob = crypto::encrypt(password, &json);
    let file = VaultFile { version: VAULT_FORMAT, blob };
    let contents = serde_json::to_vec_pretty(&file).expect("vault file always serializes");
    atomic_write(path, &contents)
}

pub fn parse_vault_file(bytes: &[u8]) -> Result<VaultFile, String> {
    serde_json::from_slice(bytes).map_err(|_| "vault file is corrupted".to_string())
}

pub fn decrypt_vault_file(file: &VaultFile, key: &[u8; 32]) -> Result<VaultData, String> {
    let plaintext = crypto::decrypt_with_key(key, &file.blob)
        .map_err(|_| "wrong master password or corrupted vault".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|_| "vault contents are corrupted".to_string())
}

/// Reads and decrypts the vault, also returning the derived key and salt so
/// the caller can hold them in the unlocked session and reuse them for fast
/// re-saves without re-running Argon2id on every account edit.
pub fn read_vault_with_key(
    path: &Path,
    password: &str,
) -> Result<(VaultData, [u8; 32], Vec<u8>), String> {
    let contents = fs::read(path).map_err(|e| format!("could not read vault file: {e}"))?;
    let file = parse_vault_file(&contents)?;
    let salt = crypto::salt_from_blob(&file.blob);
    let key = crypto::derive_key(password, &salt);
    let data = decrypt_vault_file(&file, &key)?;
    Ok((data, key, salt))
}

pub fn encode_vault_with_key(key: &[u8; 32], salt: &[u8], data: &VaultData) -> Vec<u8> {
    let json = serde_json::to_vec(data).expect("vault data always serializes");
    let blob = crypto::encrypt_with_key(key, salt, &json);
    let file = VaultFile { version: VAULT_FORMAT, blob };
    serde_json::to_vec_pretty(&file).expect("vault file always serializes")
}

pub fn write_vault_with_key(
    path: &Path,
    key: &[u8; 32],
    salt: &[u8],
    data: &VaultData,
) -> std::io::Result<()> {
    atomic_write(path, &encode_vault_with_key(key, salt, data))
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

/// Merges two copies of the vault (local and the server's). Per account the
/// newer `updated_at` wins; a tombstone beats any edit made before the
/// delete. Tombstones older than ~1 year are pruned.
pub fn merge(a: &VaultData, b: &VaultData) -> VaultData {
    use std::collections::HashMap;
    let mut accounts: HashMap<&str, &Account> = HashMap::new();
    for acc in a.accounts.iter().chain(b.accounts.iter()) {
        match accounts.get(acc.id.as_str()) {
            Some(existing) if existing.updated_at >= acc.updated_at => {}
            _ => {
                accounts.insert(acc.id.as_str(), acc);
            }
        }
    }
    let mut tombs: HashMap<&str, i64> = HashMap::new();
    for t in a.tombstones.iter().chain(b.tombstones.iter()) {
        let e = tombs.entry(t.id.as_str()).or_insert(t.deleted_at);
        if t.deleted_at > *e {
            *e = t.deleted_at;
        }
    }
    let cutoff = crate::now_millis() - 365 * 24 * 3600 * 1000;
    let mut out_accounts: Vec<Account> = accounts
        .into_values()
        .filter(|acc| match tombs.get(acc.id.as_str()) {
            Some(&deleted_at) => acc.updated_at > deleted_at,
            None => true,
        })
        .cloned()
        .collect();
    out_accounts.sort_by(|x, y| x.created_at.cmp(&y.created_at).then(x.id.cmp(&y.id)));
    let mut out_tombs: Vec<Tombstone> = tombs
        .into_iter()
        .filter(|(_, t)| *t > cutoff)
        .map(|(id, deleted_at)| Tombstone {
            id: id.to_string(),
            deleted_at,
        })
        .collect();
    out_tombs.sort_by(|x, y| x.id.cmp(&y.id));
    VaultData {
        accounts: out_accounts,
        tombstones: out_tombs,
    }
}

/// True when both copies hold the same accounts and tombstones (order-free).
pub fn same_content(a: &VaultData, b: &VaultData) -> bool {
    let norm = |v: &VaultData| {
        let mut accs = v.accounts.clone();
        accs.sort_by(|x, y| x.id.cmp(&y.id));
        let mut t = v.tombstones.clone();
        t.sort_by(|x, y| x.id.cmp(&y.id));
        (accs, t)
    };
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(id: &str, name: &str, updated: i64) -> Account {
        Account {
            id: id.into(),
            name: name.into(),
            username: String::new(),
            password: String::new(),
            url: String::new(),
            notes: String::new(),
            created_at: 1,
            updated_at: updated,
            favorite: false,
            tags: vec![],
        }
    }

    #[test]
    fn newer_edit_wins_and_union_kept() {
        let now = crate::now_millis();
        let a = VaultData { accounts: vec![acc("1", "old", now), acc("2", "only-a", now)], tombstones: vec![] };
        let b = VaultData { accounts: vec![acc("1", "new", now + 5), acc("3", "only-b", now)], tombstones: vec![] };
        let m = merge(&a, &b);
        assert_eq!(m.accounts.len(), 3);
        assert_eq!(m.accounts.iter().find(|x| x.id == "1").unwrap().name, "new");
        assert!(same_content(&m, &merge(&b, &a)));
    }

    #[test]
    fn tombstone_removes_older_edit_but_not_newer() {
        let now = crate::now_millis();
        let a = VaultData { accounts: vec![acc("1", "x", now), acc("2", "y", now + 100)], tombstones: vec![] };
        let b = VaultData {
            accounts: vec![],
            tombstones: vec![Tombstone { id: "1".into(), deleted_at: now + 10 }, Tombstone { id: "2".into(), deleted_at: now + 10 }],
        };
        let m = merge(&a, &b);
        assert_eq!(m.accounts.len(), 1);
        assert_eq!(m.accounts[0].id, "2");
        assert_eq!(m.tombstones.len(), 2);
    }

    #[test]
    fn v1_payload_still_parses() {
        let v1 = r#"{"accounts":[{"id":"a","name":"n","username":"u","password":"p","url":"","notes":"","created_at":1,"updated_at":2}]}"#;
        let d: VaultData = serde_json::from_str(v1).unwrap();
        assert_eq!(d.accounts.len(), 1);
        assert!(!d.accounts[0].favorite);
        assert!(d.tombstones.is_empty());
    }
}
