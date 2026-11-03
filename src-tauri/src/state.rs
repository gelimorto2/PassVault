use crate::vault::VaultData;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zeroize::Zeroize;

pub struct PendingSetup {
    pub secret_b32: String,
    pub password: String,
}

impl Drop for PendingSetup {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

pub struct Session {
    pub key: [u8; 32],
    pub salt: Vec<u8>,
    pub data: VaultData,
    /// HMAC root derived from the TOTP secret; every linked device has the
    /// same one, so it identifies + authenticates this vault to the sync
    /// server without any extra pairing step.
    pub sync_root: Option<[u8; 32]>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.key.zeroize();
        if let Some(r) = self.sync_root.as_mut() {
            r.zeroize();
        }
    }
}

pub struct AppState {
    pub vault_path: PathBuf,
    pub device_path: PathBuf,
    pub config_path: PathBuf,
    pub config: PointerConfig,
    pub pending_setup: Option<PendingSetup>,
    pub session: Option<Session>,
    pub pending_import: Option<(String, Vec<crate::importer::Candidate>)>,
    /// Android: skip the auto-lock-on-background until this time (ms), so
    /// opening a system file picker / settings screen we launched ourselves
    /// doesn't lock the vault underneath the user.
    pub keep_unlocked_until: i64,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct PointerConfig {
    pub vault_path: Option<String>,
    /// e.g. "http://192.168.0.246:8787" — home-network sync server.
    #[serde(default)]
    pub sync_url: Option<String>,
    #[serde(default)]
    pub last_sync_ms: Option<i64>,
}
