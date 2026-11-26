use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS randomness unavailable");
    buf
}

pub fn random_hex<const N: usize>() -> String {
    hex::encode(random_bytes::<N>())
}

pub fn random_token<const N: usize>() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<N>())
}

pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

pub fn sha512_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha512::digest(data.as_ref()))
}

pub fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC accepts any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac_sha256_hex(key: &[u8], parts: &[&[u8]]) -> String {
    hex::encode(hmac_sha256(key, parts))
}

/// Constant-time equality that does not leak the length of the secret.
pub fn secure_eq(a: &str, b: &str) -> bool {
    let da = Sha256::digest(a.as_bytes());
    let db = Sha256::digest(b.as_bytes());
    da.ct_eq(&db).into()
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "product".to_string()
    } else {
        out
    }
}

/// `••••abcd` for a secret, empty string when unset.
pub fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "••••".to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Opaque keyset cursor over `(timestamp, id)` ordered descending.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub ts: DateTime<Utc>,
    pub id: Uuid,
}

impl Cursor {
    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.ts.timestamp_micros(), self.id))
    }

    pub fn decode(s: &str) -> Option<Cursor> {
        let raw = URL_SAFE_NO_PAD.decode(s).ok()?;
        let raw = String::from_utf8(raw).ok()?;
        let (micros, id) = raw.split_once(':')?;
        Some(Cursor {
            ts: DateTime::from_timestamp_micros(micros.parse().ok()?)?,
            id: id.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip() {
        let c = Cursor {
            ts: Utc::now(),
            id: new_id(),
        };
        let d = Cursor::decode(&c.encode()).unwrap();
        assert_eq!(c.id, d.id);
        assert_eq!(c.ts.timestamp_micros(), d.ts.timestamp_micros());
        assert!(Cursor::decode("garbage").is_none());
    }

    #[test]
    fn slug_and_mask() {
        assert_eq!(slugify("  My App / v2! "), "my-app-v2");
        assert_eq!(slugify("***"), "product");
        assert_eq!(mask_secret("SB-Mid-server-abcdef1234"), "••••1234");
        assert_eq!(mask_secret("short"), "••••");
        assert_eq!(mask_secret(""), "");
    }

    #[test]
    fn hmac_known_vector() {
        // RFC 4231 test case 2
        let mac = hmac_sha256_hex(b"Jefe", &[b"what do ya want ", b"for nothing?"]);
        assert_eq!(
            mac,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn secure_eq_works() {
        assert!(secure_eq("abc", "abc"));
        assert!(!secure_eq("abc", "abd"));
        assert!(!secure_eq("abc", "abcd"));
    }
}
