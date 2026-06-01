use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

/// Encrypt plaintext with AES-256-GCM
pub struct Encryptor {
    cipher: Aes256Gcm,
}

impl Encryptor {
    pub fn from_env() -> Result<Self, String> {
        let key_hex = std::env::var("ENCRYPTION_KEY")
            .map_err(|_| "ENCRYPTION_KEY not set".to_string())?;
        let key = hex::decode(&key_hex).map_err(|e| format!("invalid hex key: {}", e))?;
        if key.len() != 32 {
            return Err("ENCRYPTION_KEY must be 32 bytes (64 hex chars)".into());
        }
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("invalid key: {}", e))?;
        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self.cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("encryption failed: {}", e))?;
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<String, String> {
        if data.len() < 12 { return Err("data too short".into()); }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| format!("decryption failed: {}", e))?;
        String::from_utf8(plaintext).map_err(|e| format!("invalid utf8: {}", e))
    }
}
