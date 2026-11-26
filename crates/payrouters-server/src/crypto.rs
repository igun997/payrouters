//! AES-256-GCM envelope for secrets stored in Postgres (provider credentials, webhook secrets).

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::util::random_bytes;

const NONCE_LEN: usize = 12;

#[derive(Clone)]
pub struct Cipher {
    inner: Aes256Gcm,
}

impl Cipher {
    pub fn new(key: &[u8; 32]) -> Cipher {
        Cipher {
            inner: Aes256Gcm::new_from_slice(key).expect("32-byte key"),
        }
    }

    /// `base64(nonce || ciphertext)`; a fresh random nonce per call.
    pub fn encrypt(&self, plaintext: &[u8]) -> anyhow::Result<String> {
        let nonce_bytes = random_bytes::<NONCE_LEN>();
        let nonce = Nonce::try_from(&nonce_bytes[..]).map_err(|_| anyhow!("bad nonce length"))?;
        let ct = self
            .inner
            .encrypt(&nonce, plaintext)
            .map_err(|_| anyhow!("encryption failed"))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(STANDARD.encode(out))
    }

    pub fn decrypt(&self, encoded: &str) -> anyhow::Result<Vec<u8>> {
        let raw = STANDARD
            .decode(encoded)
            .context("ciphertext is not base64")?;
        if raw.len() < NONCE_LEN + 16 {
            return Err(anyhow!("ciphertext too short"));
        }
        let (nonce_bytes, ct) = raw.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce_bytes).map_err(|_| anyhow!("bad nonce length"))?;
        self.inner
            .decrypt(&nonce, ct)
            .map_err(|_| anyhow!("decryption failed (wrong MASTER_KEY or corrupted data)"))
    }

    pub fn encrypt_str(&self, s: &str) -> anyhow::Result<String> {
        self.encrypt(s.as_bytes())
    }

    pub fn decrypt_string(&self, encoded: &str) -> anyhow::Result<String> {
        String::from_utf8(self.decrypt(encoded)?).context("decrypted secret is not UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper() {
        let c = Cipher::new(&[9u8; 32]);
        let enc = c.encrypt_str("server-key").unwrap();
        assert_eq!(c.decrypt_string(&enc).unwrap(), "server-key");
        // nonce differs per call
        assert_ne!(enc, c.encrypt_str("server-key").unwrap());

        let mut raw = STANDARD.decode(&enc).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert!(c.decrypt_string(&STANDARD.encode(raw)).is_err());

        let other = Cipher::new(&[1u8; 32]);
        assert!(other.decrypt_string(&enc).is_err());
    }
}
