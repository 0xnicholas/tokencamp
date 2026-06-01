use sha2::{Sha256, Digest};
use tokencamp_core::{CacheLayer, ChatRequest, ModelResponse};

/// Response cache for identical LLM prompts
#[derive(Clone)]
pub struct ResponseCache {
    cache: std::sync::Arc<dyn CacheLayer>,
}

impl ResponseCache {
    pub fn new(cache: std::sync::Arc<dyn CacheLayer>) -> Self {
        Self { cache }
    }

    /// Generate cache key from request
    fn cache_key(request: &ChatRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(request.model.as_bytes());
        for msg in &request.messages {
            hasher.update(msg.role.as_bytes());
            hasher.update(msg.content.as_bytes());
        }
        if let Some(t) = request.temperature {
            hasher.update(&t.to_le_bytes());
        }
        if let Some(mt) = request.max_tokens {
            hasher.update(&mt.to_le_bytes());
        }
        format!("response:{}", hex::encode(hasher.finalize()))
    }

    /// Check cache for existing response
    pub async fn get(&self, request: &ChatRequest) -> Option<ModelResponse> {
        let key = Self::cache_key(request);
        if let Some(json) = self.cache.get(&key).await {
            serde_json::from_str(&json).ok()
        } else {
            None
        }
    }

    /// Store response in cache
    pub async fn set(&self, request: &ChatRequest, response: &ModelResponse) {
        let key = Self::cache_key(request);
        if let Ok(json) = serde_json::to_string(response) {
            self.cache.set(&key, &json, std::time::Duration::from_secs(300)).await; // 5min TTL
        }
    }
}
