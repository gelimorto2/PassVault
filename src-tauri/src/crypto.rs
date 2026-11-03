use aes_gcm::aead::{Aead, KeyInit, OsRng as AesOsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Argon2, Algorithm, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

// Argon2id, ~64MB memory / 3 passes / 4 lanes: deliberately slow for offline
// brute-force resistance since this key protects a file that may live in a
// synced folder outside our control.
fn argon2_params() -> Params {
    Params::new(65536, 3, 4, Some(KEY_LEN)).expect("valid argon2 params")
}

pub fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &str, salt: &[u8]) -> [u8; KEY_LEN] {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params());
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("argon2 key derivation");
    key
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EncryptedBlob {
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Debug)]
pub enum CryptoError {
    DecryptFailed,
}

pub fn encrypt(password: &str, plaintext: &[u8]) -> EncryptedBlob {
    let salt = random_salt();
    let key = derive_key(password, &salt);
    encrypt_with_key(&key, &salt, plaintext)
}

pub fn encrypt_with_key(key: &[u8; KEY_LEN], salt: &[u8], plaintext: &[u8]) -> EncryptedBlob {
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key length");
    let mut nonce_bytes = [0u8; NONCE_LEN];
    AesOsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .expect("aes-gcm encryption");

    EncryptedBlob {
        salt: b64(salt),
        nonce: b64(&nonce_bytes),
        ciphertext: b64(&ciphertext),
    }
}

pub fn decrypt(password: &str, blob: &EncryptedBlob) -> Result<Vec<u8>, CryptoError> {
    let salt = unb64(&blob.salt).map_err(|_| CryptoError::DecryptFailed)?;
    let key = derive_key(password, &salt);
    decrypt_with_key(&key, blob)
}

pub fn decrypt_with_key(key: &[u8; KEY_LEN], blob: &EncryptedBlob) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::DecryptFailed)?;
    let nonce_bytes = unb64(&blob.nonce).map_err(|_| CryptoError::DecryptFailed)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = unb64(&blob.ciphertext).map_err(|_| CryptoError::DecryptFailed)?;
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| CryptoError::DecryptFailed)
}

pub fn salt_from_blob(blob: &EncryptedBlob) -> Vec<u8> {
    unb64(&blob.salt).expect("blob salt is valid base64")
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| ())
}
