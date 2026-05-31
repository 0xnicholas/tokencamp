use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cache::CacheLayer;
use crate::types::{ChatRequest, ModelResponse};

use super::{AuthContext, ProxyHook};

pub struct CostTracker {
    cache: Arc<dyn CacheLayer>,
}

impl CostTracker {
    pub fn new(cache: Arc<dyn CacheLayer>) -> Self {
        Self { cache }
    }
}

#[derive(Serialize, Deserialize)]
struct SpendEntry {
    key_id: String,
    request_id: String,
    model: String,
    provider: String,
    prompt_tokens: u32,
    completion_tokens: u32,
    cost: f64,
    duration_ms: Option<u64>,
    status: String,
}

#[async_trait::async_trait]
impl ProxyHook for CostTracker {
    async fn async_post_call_hook(
        &self, request: &ChatRequest, response: &ModelResponse, auth: &AuthContext,
    ) {
        let cost = if let Some(ref pricing) = auth.model_pricing {
            (response.usage.prompt_tokens as f64 * pricing.prompt_price
                + response.usage.completion_tokens as f64 * pricing.completion_price)
                / 1000.0
        } else {
            0.0
        };

        let entry = SpendEntry {
            key_id: auth.key_id.clone(),
            request_id: response.id.clone(),
            model: request.model.clone(),
            provider: String::new(), // filled by caller via auth context
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            cost,
            duration_ms: None,
            status: "success".into(),
        };

        if let Ok(json) = serde_json::to_string(&entry) {
            let _ = self.cache.rpush("spend:queue", &json).await;
        }
    }

    async fn async_on_error_hook(
        &self, request: &ChatRequest, _error: &crate::provider::ProviderError, auth: &AuthContext,
    ) {
        let entry = SpendEntry {
            key_id: auth.key_id.clone(),
            request_id: String::new(),
            model: request.model.clone(),
            provider: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost: 0.0,
            duration_ms: None,
            status: "error".into(),
        };
        if let Ok(json) = serde_json::to_string(&entry) {
            let _ = self.cache.rpush("spend:queue", &json).await;
        }
    }
}
