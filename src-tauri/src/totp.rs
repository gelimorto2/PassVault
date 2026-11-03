use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

const STEP_SECONDS: u64 = 30;
const DIGITS: u32 = 6;
// Accept one step of clock drift either side, matching typical
// Google Authenticator / RFC 6238 recommended tolerance.
const SKEW_STEPS: i64 = 1;

pub fn generate_secret() -> Vec<u8> {
    let mut secret = [0u8; 20]; // 160-bit, standard TOTP secret size
    rand::thread_rng().fill_bytes(&mut secret);
    secret.to_vec()
}

pub fn to_base32(secret: &[u8]) -> String {
    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, secret)
}

pub fn from_base32(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &cleaned.to_uppercase())
}

pub fn otpauth_uri(secret_b32: &str, issuer: &str, account: &str) -> String {
    format!(
        "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits=6&period=30",
        issuer = urlencode(issuer),
        account = urlencode(account),
        secret = secret_b32,
    )
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

fn hotp(secret: &[u8], counter: u64) -> u32 {
    let mut mac = HmacSha1::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(&counter.to_be_bytes());
    let hash = mac.finalize().into_bytes();

    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let binary = ((hash[offset] as u32 & 0x7f) << 24)
        | ((hash[offset + 1] as u32) << 16)
        | ((hash[offset + 2] as u32) << 8)
        | (hash[offset + 3] as u32);

    binary % 10u32.pow(DIGITS)
}

fn current_step() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_secs();
    now / STEP_SECONDS
}

/// Computes the current code directly from the secret. Used only for the
/// biometric-unlock path, where the secret is already sitting behind a
/// device-level biometric gate — this just saves re-typing what that gate
/// already proves possession of.
pub fn current_code(secret: &[u8]) -> String {
    format!(
        "{:0width$}",
        hotp(secret, current_step()),
        width = DIGITS as usize
    )
}

pub fn verify(secret: &[u8], code: &str) -> bool {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let step = current_step() as i64;
    for skew in -SKEW_STEPS..=SKEW_STEPS {
        let candidate_step = step + skew;
        if candidate_step < 0 {
            continue;
        }
        let expected = format!(
            "{:0width$}",
            hotp(secret, candidate_step as u64),
            width = DIGITS as usize
        );
        if constant_time_eq(expected.as_bytes(), code.as_bytes()) {
            return true;
        }
    }
    false
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn qr_svg(otpauth_uri: &str) -> String {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = QrCode::new(otpauth_uri.as_bytes()).expect("otpauth uri fits in a QR code");
    code.render()
        .min_dimensions(260, 260)
        .dark_color(svg::Color("#1a1a1a"))
        .light_color(svg::Color("#ffffff"))
        .build()
}
