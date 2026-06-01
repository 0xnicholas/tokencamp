use async_trait::async_trait;
use std::collections::HashMap;

use crate::encryptor::Encryptor;

/// Trait for resolving provider API keys. Supports:
/// - Environment variable (existing)
/// - Encrypted database (v0.5)
/// - External Secret Manager (AWS/GCP, future)
#[async_trait]
pub trait SecretManager: Send + Sync {
    /// Look up the API key for a given provider name.
    /// Returns None if not found.
    async fn get_secret(&self, provider: &str) -> Option<String>;
}

/// Environment-based secret manager (current default)
pub struct EnvSecretManager;

#[async_trait]
impl SecretManager for EnvSecretManager {
    async fn get_secret(&self, provider: &str) -> Option<String> {
        std::env::var(&format!("{}_API_KEY", provider.to_uppercase())).ok()
    }
}

/// Encrypted DB-backed secret manager
pub struct EncryptedSecretManager {
    encryptor: Encryptor,
    cache: HashMap<String, String>,
}

impl EncryptedSecretManager {
    pub fn new(encryptor: Encryptor) -> Self {
        Self { encryptor, cache: HashMap::new() }
    }

    /// Load and decrypt a credential from DB
    pub async fn load_from_db(&mut self, provider: &str, encrypted: Vec<u8>) -> Result<String, String> {
        let plain = self.encryptor.decrypt(&encrypted)?;
        self.cache.insert(provider.to_string(), plain.clone());
        Ok(plain)
    }

    /// Store encrypted credential to DB
    pub async fn store_to_db(
        &self,
        db: &crate::db::DbPool,
        provider: &str,
        plaintext: &str,
    ) -> Result<(), String> {
        let encrypted = self.encryptor.encrypt(plaintext)?;
        sqlx::query(
            "INSERT INTO credentials (provider, encrypted_value) VALUES ($1, $2) \
             ON CONFLICT (provider) DO UPDATE SET encrypted_value = $2"
        )
        .bind(provider)
        .bind(&encrypted)
        .execute(db.pool())
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[async_trait]
impl SecretManager for EncryptedSecretManager {
    async fn get_secret(&self, provider: &str) -> Option<String> {
        self.cache.get(provider).cloned()
    }
}
