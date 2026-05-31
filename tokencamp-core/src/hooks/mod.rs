pub mod parallel_request_limiter;
pub mod cost_tracker;

use serde::{Deserialize, Serialize};

use crate::types::{ChatRequest, ModelResponse};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub key_id: String,
    pub key_name: Option<String>,
    pub tpm_limit: Option<u32>,
    pub rpm_limit: Option<u32>,
    /// 成本追踪的定价信息
    #[serde(skip)]
    pub model_pricing: Option<ModelPricing>,
}

#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub prompt_price: f64,
    pub completion_price: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("rate limit exceeded ({0})")]
    RateLimitExceeded(String),
    #[error("{0}")]
    Internal(String),
}

impl From<HookError> for crate::provider::ProviderError {
    fn from(e: HookError) -> Self {
        crate::provider::ProviderError::UpstreamError {
            status: 429,
            message: e.to_string(),
        }
    }
}

#[async_trait::async_trait]
pub trait ProxyHook: Send + Sync {
    async fn async_pre_call_hook(
        &self, _request: &ChatRequest, _auth: &AuthContext,
    ) -> Result<(), HookError> {
        Ok(())
    }

    async fn async_post_call_hook(
        &self, _request: &ChatRequest, _response: &ModelResponse, _auth: &AuthContext,
    ) {}

    async fn async_on_error_hook(
        &self, _request: &ChatRequest, _error: &crate::provider::ProviderError, _auth: &AuthContext,
    ) {}
}
