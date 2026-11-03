use crate::vault::Account;
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
    pub accounts: Vec<Account>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

pub struct AppState {
    pub vault_path: PathBuf,
    pub device_path: PathBuf,
    pub config_path: PathBuf,
    pub pending_setup: Option<PendingSetup>,
    pub session: Option<Session>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct PointerConfig {
    pub vault_path: Option<String>,
}
