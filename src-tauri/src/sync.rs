//! Home-network sync against the small `passvault-sync` server (leoserv0).
//!
//! The server is zero-knowledge: it only ever stores the same encrypted
//! vault file that sits on disk, plus a revision counter. All merging
//! happens here, after decryption, so the server can't read or forge
//! entries. The vault id and the bearer token are both derived from the
//! TOTP secret that every linked device already holds, so pairing a new
//! device for sync needs nothing beyond linking it.

use crate::state::AppState;
use crate::vault::{self, VaultFile};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Mutex;
use std::time::Duration;

pub fn derive_root(totp_secret_b32: &str) -> Option<[u8; 32]> {
    let secret = crate::totp::from_base32(totp_secret_b32)?;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&secret).ok()?;
    mac.update(b"passvault-sync-root-v1");
    Some(mac.finalize().into_bytes().into())
}

fn label(root: &[u8; 32], what: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(root).expect("hmac key");
    mac.update(what);
    hex::encode(mac.finalize().into_bytes())
}

#[derive(Serialize)]
pub struct SyncResult {
    pub accounts: Vec<vault::Account>,
    pub changed_locally: bool,
    pub pushed: bool,
    pub last_sync_ms: i64,
}

#[derive(Deserialize)]
struct GetResp {
    rev: u64,
    vault: serde_json::Value,
}

#[derive(Deserialize)]
struct PutResp {
    rev: u64,
}

enum Remote {
    Missing,
    Found(u64, VaultFile),
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
}

fn friendly(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => {
            "Sync server rejected this vault's key.".into()
        }
        ureq::Error::Status(code, _) => format!("Sync server error ({code})."),
        ureq::Error::Transport(_) => {
            "Can't reach the sync server — sync only works on your home Wi-Fi.".into()
        }
    }
}

pub fn normalize_url(url: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/');
    if u.is_empty() {
        return Ok(String::new());
    }
    let with_scheme = if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("http://{u}")
    };
    if with_scheme.starts_with("https://") {
        return Err("This build syncs over plain HTTP on your home network; use http://".into());
    }
    url::Url::parse(&with_scheme).map_err(|_| "That isn't a valid server address.".to_string())?;
    Ok(with_scheme)
}

pub fn ping(base: &str) -> Result<(), String> {
    let resp = agent()
        .get(&format!("{base}/v1/ping"))
        .call()
        .map_err(friendly)?;
    let body = resp.into_string().map_err(|e| e.to_string())?;
    if body.contains("passvault-sync") {
        Ok(())
    } else {
        Err("Something answered at that address, but it isn't a PassVault sync server.".into())
    }
}

fn get_remote(base: &str, id: &str, token: &str) -> Result<Remote, String> {
    match agent()
        .get(&format!("{base}/v1/vault/{id}"))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) => {
            let r: GetResp = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?)
                .map_err(|_| "Sync server sent a malformed reply.".to_string())?;
            let file: VaultFile = serde_json::from_value(r.vault)
                .map_err(|_| "Server copy of the vault is corrupted.".to_string())?;
            Ok(Remote::Found(r.rev, file))
        }
        Err(ureq::Error::Status(404, _)) => Ok(Remote::Missing),
        Err(e) => Err(friendly(e)),
    }
}

/// Ok(Some(rev)) on success, Ok(None) on revision conflict (retry).
fn put_remote(base: &str, id: &str, token: &str, if_rev: u64, file_bytes: &[u8]) -> Result<Option<u64>, String> {
    let vault_json: serde_json::Value =
        serde_json::from_slice(file_bytes).map_err(|e| e.to_string())?;
    let body = serde_json::json!({ "vault": vault_json }).to_string();
    match agent()
        .put(&format!("{base}/v1/vault/{id}"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("If-Match", &if_rev.to_string())
        .set("Content-Type", "application/json")
        .send_string(&body)
    {
        Ok(resp) => {
            let r: PutResp = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?)
                .map_err(|_| "Sync server sent a malformed reply.".to_string())?;
            Ok(Some(r.rev))
        }
        Err(ureq::Error::Status(409, _)) => Ok(None),
        Err(e) => Err(friendly(e)),
    }
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// One full pull-merge-push round. Holds the state lock only for the brief
/// snapshot/commit steps, never across network calls.
pub fn sync_once(state: &Mutex<AppState>) -> Result<SyncResult, String> {
    for _attempt in 0..4 {
        let (base, key, salt, local, root) = {
            let s = state.lock().unwrap();
            let base = s
                .config
                .sync_url
                .clone()
                .filter(|u| !u.is_empty())
                .ok_or("Sync isn't set up on this device yet.")?;
            let session = s.session.as_ref().ok_or("Vault is locked.")?;
            let root = session
                .sync_root
                .ok_or("Unlock with your password once to enable sync on this device.")?;
            (base, session.key, session.salt.clone(), session.data.clone(), root)
        };
        let id = &label(&root, b"vault-id")[..32];
        let token = label(&root, b"token");

        let (rev, merged, remote_same) = match get_remote(&base, id, &token)? {
            Remote::Missing => (0, local.clone(), false),
            Remote::Found(rev, file) => {
                if file.blob.salt != b64(&salt) {
                    return Err(
                        "The server holds a different vault (created with another master password). Sync stopped to protect your data."
                            .into(),
                    );
                }
                let remote = vault::decrypt_vault_file(&file, &key)
                    .map_err(|_| "Couldn't decrypt the server copy with this vault's key.".to_string())?;
                let merged = vault::merge(&local, &remote);
                let same = vault::same_content(&merged, &remote);
                (rev, merged, same)
            }
        };

        let mut pushed = false;
        if !remote_same {
            let bytes = vault::encode_vault_with_key(&key, &salt, &merged);
            match put_remote(&base, id, &token, rev, &bytes)? {
                Some(_) => pushed = true,
                None => continue, // someone else wrote in between: re-pull
            }
        }

        let mut s = state.lock().unwrap();
        let now = crate::now_millis();
        let vault_path = s.vault_path.clone();
        let session = s.session.as_mut().ok_or("Vault was locked during sync.")?;
        if session.key != key {
            return Err("Vault changed during sync.".into());
        }
        // Fold in anything edited locally while we were on the network.
        let final_data = vault::merge(&session.data, &merged);
        let changed_locally = !vault::same_content(&final_data, &session.data);
        session.data = final_data;
        if changed_locally {
            vault::write_vault_with_key(&vault_path, &key, &salt, &session.data)
                .map_err(|e| format!("could not save vault: {e}"))?;
        }
        let accounts = session.data.accounts.clone();
        s.config.last_sync_ms = Some(now);
        crate::save_pointer(&s.config_path, &s.config);
        return Ok(SyncResult {
            accounts,
            changed_locally,
            pushed,
            last_sync_ms: now,
        });
    }
    Err("Sync kept conflicting with another device; try again in a moment.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{PointerConfig, Session};
    use crate::vault::{Account, Tombstone, VaultData};
    use std::path::PathBuf;

    fn device(dir: &PathBuf, name: &str, url: &str, vault_src: &PathBuf, secret: &str) -> Mutex<AppState> {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        let vp = d.join("vault.enc");
        std::fs::copy(vault_src, &vp).unwrap();
        let (data, key, salt) = vault::read_vault_with_key(&vp, "correct horse").unwrap();
        Mutex::new(AppState {
            vault_path: vp,
            device_path: d.join("device.json"),
            config_path: d.join("config.json"),
            config: PointerConfig { vault_path: None, sync_url: Some(url.into()), last_sync_ms: None },
            pending_setup: None,
            session: Some(Session { key, salt, data, sync_root: derive_root(secret) }),
            pending_import: None,
            keep_unlocked_until: 0,
        })
    }

    fn add(dev: &Mutex<AppState>, id: &str, name: &str) {
        let mut s = dev.lock().unwrap();
        let now = crate::now_millis();
        s.session.as_mut().unwrap().data.accounts.push(Account {
            id: id.into(), name: name.into(), username: "u".into(), password: "p".into(),
            url: String::new(), notes: String::new(), created_at: now, updated_at: now,
            favorite: false, tags: vec![],
        });
    }

    fn names(dev: &Mutex<AppState>) -> Vec<String> {
        let s = dev.lock().unwrap();
        let mut v: Vec<String> = s.session.as_ref().unwrap().data.accounts.iter().map(|a| a.name.clone()).collect();
        v.sort();
        v
    }

    /// Needs a running server: PASSVAULT_TEST_SYNC_URL=http://127.0.0.1:18787
    #[test]
    #[ignore]
    fn two_devices_converge() {
        let url = std::env::var("PASSVAULT_TEST_SYNC_URL").expect("set PASSVAULT_TEST_SYNC_URL");
        let dir = std::env::temp_dir().join(format!("pv-sync-test-{}", crate::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("seed.enc");
        vault::write_vault(&src, "correct horse", &VaultData::default()).unwrap();
        let secret = crate::totp::to_base32(&crate::totp::generate_secret());

        let a = device(&dir, "a", &url, &src, &secret);
        let b = device(&dir, "b", &url, &src, &secret);
        ping(&url).unwrap();

        add(&a, "1", "github");
        let r = sync_once(&a).unwrap();
        assert!(r.pushed);
        let r = sync_once(&b).unwrap();
        assert!(r.changed_locally);
        assert_eq!(names(&b), vec!["github"]);

        // Concurrent: B deletes github + adds gitlab, A adds mail.
        {
            let mut s = b.lock().unwrap();
            let sess = s.session.as_mut().unwrap();
            sess.data.accounts.retain(|x| x.id != "1");
            sess.data.tombstones.push(Tombstone { id: "1".into(), deleted_at: crate::now_millis() + 1 });
        }
        add(&b, "2", "gitlab");
        add(&a, "3", "mail");
        sync_once(&b).unwrap();
        sync_once(&a).unwrap();
        sync_once(&b).unwrap();
        assert_eq!(names(&a), vec!["gitlab", "mail"]);
        assert_eq!(names(&b), vec!["gitlab", "mail"]);

        // Same content -> no push.
        assert!(!sync_once(&a).unwrap().pushed);

        // On-disk vault of A reflects merged state.
        let (disk, _, _) = vault::read_vault_with_key(&a.lock().unwrap().vault_path, "correct horse").unwrap();
        assert_eq!(disk.accounts.len(), 2);

        // A different vault (other TOTP secret) never sees ours; wrong token is refused.
        let c = device(&dir, "c", &url, &src, &crate::totp::to_base32(&crate::totp::generate_secret()));
        sync_once(&c).unwrap();
        assert!(names(&c).is_empty());
        let root = derive_root(&secret).unwrap();
        let id = &label(&root, b"vault-id")[..32];
        assert!(get_remote(&url, id, "wrong-token").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
