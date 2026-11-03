//! Linux-only: read saved logins straight out of local browser profiles.
//!
//! * Firefox family (Firefox incl. snap/flatpak, LibreWolf): `logins.json`
//!   + `key4.db` (NSS). Works when no Firefox *primary password* is set;
//!   otherwise we tell the user to export CSV instead.
//! * Chromium family (Chrome, Chromium, Brave, Edge, Vivaldi, Opera):
//!   `Login Data` SQLite, values encrypted with a key kept in the GNOME
//!   keyring ("<Browser> Safe Storage"), v10/v11 AES-128-CBC.
//!
//! Databases are opened read-only/immutable so a running browser is fine.

use crate::importer::{display_name_for, Candidate};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct BrowserProfile {
    pub id: String,
    pub browser: String,
    pub profile: String,
    pub path: String,
    pub kind: String, // "firefox" | "chromium"
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn detect() -> Vec<BrowserProfile> {
    let h = home();
    let mut out = Vec::new();

    let firefox_roots = [
        ("Firefox", h.join(".mozilla/firefox")),
        ("Firefox (snap)", h.join("snap/firefox/common/.mozilla/firefox")),
        ("Firefox (flatpak)", h.join(".var/app/org.mozilla.firefox/.mozilla/firefox")),
        ("LibreWolf", h.join(".librewolf")),
        ("LibreWolf (flatpak)", h.join(".var/app/io.gitlab.librewolf-community/.librewolf")),
        ("Waterfox", h.join(".waterfox")),
    ];
    for (browser, root) in firefox_roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.join("logins.json").is_file() && p.join("key4.db").is_file() {
                let dir = e.file_name().to_string_lossy().to_string();
                let profile = dir.splitn(2, '.').nth(1).unwrap_or(&dir).to_string();
                out.push(BrowserProfile {
                    id: p.to_string_lossy().to_string(),
                    browser: browser.into(),
                    profile,
                    path: p.to_string_lossy().to_string(),
                    kind: "firefox".into(),
                });
            }
        }
    }

    for (browser, root, _) in CHROMIUMS {
        let root = h.join(root);
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.join("Login Data").is_file() {
                let name = e.file_name().to_string_lossy().to_string();
                out.push(BrowserProfile {
                    id: p.to_string_lossy().to_string(),
                    browser: browser.to_string(),
                    profile: if name == "Default" { "Default".into() } else { name },
                    path: p.to_string_lossy().to_string(),
                    kind: "chromium".into(),
                });
            }
        }
    }
    out
}

/// (display name, config dir under $HOME, keyring `application` attribute)
const CHROMIUMS: &[(&str, &str, &str)] = &[
    ("Google Chrome", ".config/google-chrome", "chrome"),
    ("Chromium", ".config/chromium", "chromium"),
    ("Chromium (snap)", "snap/chromium/common/chromium", "chromium"),
    ("Brave", ".config/BraveSoftware/Brave-Browser", "brave"),
    ("Brave (snap)", "snap/brave/current/.config/BraveSoftware/Brave-Browser", "brave"),
    ("Microsoft Edge", ".config/microsoft-edge", "microsoft-edge"),
    ("Vivaldi", ".config/vivaldi", "vivaldi"),
    ("Opera", ".config/opera", "opera"),
];

pub fn read_profile(path: &str) -> Result<(String, Vec<Candidate>), String> {
    let p = Path::new(path);
    // Only accept something detect() would have offered.
    let profile = detect()
        .into_iter()
        .find(|b| b.path == path)
        .ok_or("That browser profile wasn't found.")?;
    let label = format!("{} — {}", profile.browser, profile.profile);
    let cands = if profile.kind == "firefox" {
        firefox::read(p)?
    } else {
        let app = CHROMIUMS
            .iter()
            .find(|(b, _, _)| *b == profile.browser)
            .map(|(_, _, a)| *a)
            .unwrap_or("chrome");
        chromium::read(p, app)?
    };
    Ok((label, cands))
}

fn open_ro(path: &Path) -> Result<rusqlite::Connection, String> {
    use rusqlite::OpenFlags;
    let uri = format!(
        "file:{}?immutable=1",
        path.to_string_lossy().replace('?', "%3f").replace('#', "%23")
    );
    rusqlite::Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("could not open browser database: {e}"))
}

fn name_for(url: &str) -> String {
    let n = display_name_for(url);
    if n.is_empty() {
        url.to_string()
    } else {
        n
    }
}

// ---------------------------------------------------------------- minimal DER

#[derive(Debug)]
enum Der {
    Seq(Vec<Der>),
    Oct(Vec<u8>),
    Int(Vec<u8>),
    Oid(Vec<u8>),
    Other,
}

fn der_parse(mut b: &[u8]) -> Result<Vec<Der>, ()> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let tag = b[0];
        let (len, hdr) = der_len(&b[1..])?;
        let start = 1 + hdr;
        let end = start.checked_add(len).ok_or(())?;
        if end > b.len() {
            return Err(());
        }
        let body = &b[start..end];
        out.push(match tag {
            0x30 => Der::Seq(der_parse(body)?),
            0x04 => Der::Oct(body.to_vec()),
            0x02 => Der::Int(body.to_vec()),
            0x06 => Der::Oid(body.to_vec()),
            _ => Der::Other,
        });
        b = &b[end..];
    }
    Ok(out)
}

fn der_len(b: &[u8]) -> Result<(usize, usize), ()> {
    let first = *b.first().ok_or(())?;
    if first < 0x80 {
        return Ok((first as usize, 1));
    }
    let n = (first & 0x7f) as usize;
    if n == 0 || n > 4 || b.len() < 1 + n {
        return Err(());
    }
    let mut len = 0usize;
    for &x in &b[1..1 + n] {
        len = (len << 8) | x as usize;
    }
    Ok((len, 1 + n))
}

fn seq(d: &Der) -> Result<&[Der], String> {
    match d {
        Der::Seq(v) => Ok(v),
        _ => Err("unexpected key format".into()),
    }
}
fn oct(d: &Der) -> Result<&[u8], String> {
    match d {
        Der::Oct(v) => Ok(v),
        _ => Err("unexpected key format".into()),
    }
}
fn int(d: &Der) -> Result<u64, String> {
    match d {
        Der::Int(v) => Ok(v.iter().fold(0u64, |a, &x| (a << 8) | x as u64)),
        _ => Err("unexpected key format".into()),
    }
}

fn unpad(mut v: Vec<u8>) -> Result<Vec<u8>, String> {
    let p = *v.last().ok_or("empty plaintext")? as usize;
    if p == 0 || p > 16 || p > v.len() {
        return Err("bad padding".into());
    }
    v.truncate(v.len() - p);
    Ok(v)
}

fn cbc_decrypt_aes256(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit};
    let dec = cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv).map_err(|_| "bad key/iv")?;
    let mut buf = data.to_vec();
    dec.decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|_| "decrypt failed")?;
    Ok(buf)
}

fn cbc_decrypt_aes128(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit};
    let dec = cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv).map_err(|_| "bad key/iv")?;
    let mut buf = data.to_vec();
    dec.decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|_| "decrypt failed")?;
    Ok(buf)
}

fn cbc_decrypt_3des(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    use des::cipher::{BlockDecryptMut, KeyIvInit};
    let dec = cbc::Decryptor::<des::TdesEde3>::new_from_slices(key, iv).map_err(|_| "bad key/iv")?;
    let mut buf = data.to_vec();
    dec.decrypt_padded_mut::<des::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|_| "decrypt failed")?;
    Ok(buf)
}

// -------------------------------------------------------------------- Firefox

mod firefox {
    use super::*;
    use sha1::{Digest, Sha1};

    const OID_PBES2: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x05, 0x0d];
    const OID_3DES_CBC: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x03, 0x07];

    /// Decrypts an NSS PBES2 (PBKDF2-SHA256 + AES-256-CBC) blob with the
    /// empty primary password.
    fn pbe_decrypt(global_salt: &[u8], der: &[u8]) -> Result<Vec<u8>, String> {
        let top = der_parse(der).map_err(|_| "unreadable key4.db")?;
        let top = seq(top.first().ok_or("unreadable key4.db")?)?;
        let algo = seq(&top[0])?;
        let ciphertext = oct(&top[1])?;
        match &algo[0] {
            Der::Oid(o) if o.as_slice() == OID_PBES2 => {}
            _ => {
                return Err(
                    "This Firefox profile uses an old key format. Export a CSV from Firefox instead (Passwords → ⋯ → Export)."
                        .into(),
                )
            }
        }
        let params = seq(&algo[1])?;
        let kdf = seq(&params[0])?;
        let kdf_params = seq(&kdf[1])?;
        let salt = oct(&kdf_params[0])?;
        let iters = int(&kdf_params[1])? as u32;
        let key_len = int(&kdf_params[2]).unwrap_or(32) as usize;
        let cipher = seq(&params[1])?;
        let iv_tail = oct(&cipher[1])?;

        let k = Sha1::digest(global_salt); // sha1(global_salt + "") — empty primary password
        let mut key = vec![0u8; key_len];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&k, salt, iters, &mut key);
        let mut iv = vec![0x04, 0x0e];
        iv.extend_from_slice(iv_tail);
        cbc_decrypt_aes256(&key, &iv, ciphertext)
    }

    fn master_key(db: &Path) -> Result<Vec<u8>, String> {
        let conn = open_ro(db)?;
        let (global_salt, check): (Vec<u8>, Vec<u8>) = conn
            .query_row("SELECT item1, item2 FROM metadata WHERE id = 'password'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|_| "Firefox key database is unreadable.")?;
        let clear = pbe_decrypt(&global_salt, &check)?;
        if !clear.starts_with(b"password-check") {
            return Err(
                "This Firefox profile is protected by a primary password. In Firefox open Passwords → ⋯ → Export passwords, then import that CSV here."
                    .into(),
            );
        }
        let a11: Vec<u8> = conn
            .query_row("SELECT a11 FROM nssPrivate WHERE a11 IS NOT NULL LIMIT 1", [], |r| r.get(0))
            .map_err(|_| "No saved-login key in this Firefox profile.")?;
        let key = pbe_decrypt(&global_salt, &a11)?;
        Ok(key)
    }

    fn decrypt_field(key: &[u8], b64: &str) -> Result<String, String> {
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| "bad field")?;
        let top = der_parse(&raw).map_err(|_| "bad field")?;
        let top = seq(top.first().ok_or("bad field")?)?;
        let algo = seq(&top[1])?;
        let iv = oct(&algo[1])?;
        let ct = oct(&top[2])?;
        let plain = match &algo[0] {
            Der::Oid(o) if o.as_slice() == OID_3DES_CBC => cbc_decrypt_3des(&key[..24], iv, ct)?,
            _ => cbc_decrypt_aes256(&key[..32.min(key.len())], iv, ct)?,
        };
        String::from_utf8(unpad(plain)?).map_err(|_| "bad field".into())
    }

    pub fn read(profile: &Path) -> Result<Vec<Candidate>, String> {
        let key = master_key(&profile.join("key4.db"))?;
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(profile.join("logins.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|_| "logins.json is unreadable")?;
        let mut out = Vec::new();
        for l in json["logins"].as_array().cloned().unwrap_or_default() {
            let host = l["hostname"].as_str().unwrap_or("").to_string();
            let (Some(eu), Some(ep)) = (l["encryptedUsername"].as_str(), l["encryptedPassword"].as_str()) else {
                continue;
            };
            let (Ok(username), Ok(password)) = (decrypt_field(&key, eu), decrypt_field(&key, ep)) else {
                continue;
            };
            out.push(Candidate {
                name: name_for(&host),
                username,
                password,
                url: host,
                notes: String::new(),
            });
        }
        Ok(out)
    }
}

// ------------------------------------------------------------------- Chromium

mod chromium {
    use super::*;

    fn keyring_password(app: &str) -> Option<Vec<u8>> {
        use dbus_secret_service::{EncryptionType, SecretService};
        use std::collections::HashMap;
        let ss = SecretService::connect(EncryptionType::Dh).ok()?;
        let attrs = HashMap::from([("application", app)]);
        let found = ss.search_items(attrs).ok()?;
        let item = found.unlocked.into_iter().chain(found.locked).next()?;
        let _ = item.unlock();
        item.get_secret().ok()
    }

    fn key_from(pass: &[u8]) -> [u8; 16] {
        let mut k = [0u8; 16];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(pass, b"saltysalt", 1, &mut k);
        k
    }

    pub fn read(profile: &Path, app: &str) -> Result<Vec<Candidate>, String> {
        let conn = open_ro(&profile.join("Login Data"))?;
        let db_version: i64 = conn
            .query_row("SELECT value FROM meta WHERE key = 'version'", [], |r| {
                r.get::<_, String>(0).map(|s| s.parse().unwrap_or(0)).or_else(|_| r.get(0))
            })
            .unwrap_or(0);
        let v10 = key_from(b"peanuts");
        let v11 = keyring_password(app).map(|p| key_from(&p));
        let iv = [b' '; 16];

        let mut stmt = conn
            .prepare("SELECT origin_url, username_value, password_value FROM logins WHERE blacklisted_by_user = 0")
            .map_err(|e| format!("unexpected browser database: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?))
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        let mut undecryptable = 0;
        for row in rows.flatten() {
            let (url, username, enc) = row;
            let plain = if enc.starts_with(b"v10") || enc.starts_with(b"v11") {
                let key = if enc.starts_with(b"v11") { v11.as_ref() } else { Some(&v10) };
                key.and_then(|k| cbc_decrypt_aes128(k, &iv, &enc[3..]).ok())
                    .and_then(|p| unpad(p).ok())
            } else {
                Some(enc.clone())
            };
            let Some(mut plain) = plain else {
                undecryptable += 1;
                continue;
            };
            // Chrome ≥ M130 (db version 24+) prefixes a 32-byte domain hash.
            if db_version >= 24 && plain.len() >= 32 {
                plain.drain(..32);
            }
            let Ok(password) = String::from_utf8(plain) else {
                undecryptable += 1;
                continue;
            };
            out.push(Candidate {
                name: name_for(&url),
                username,
                password,
                url,
                notes: String::new(),
            });
        }
        if out.is_empty() && undecryptable > 0 {
            return Err(
                "Couldn't unlock this browser's saved passwords (keyring locked?). Export them as CSV from the browser's password settings instead."
                    .into(),
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    /// Manual smoke test against the real local profiles. Prints counts only.
    #[test]
    #[ignore]
    fn local_profiles_decrypt() {
        for p in super::detect() {
            match super::read_profile(&p.path) {
                Ok((label, c)) => {
                    let with_pw = c.iter().filter(|x| !x.password.is_empty()).count();
                    let printable = c.iter().filter(|x| x.password.chars().all(|ch| !ch.is_control())).count();
                    println!("{label}: {} logins, {with_pw} with password, {printable} clean-text", c.len());
                }
                Err(e) => println!("{} {}: ERROR {e}", p.browser, p.profile),
            }
        }
    }
}
