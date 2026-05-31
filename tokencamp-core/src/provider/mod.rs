use async_trait::async_trait;
use bytes::Bytes;
use reqwest::{header::HeaderMap, Method, StatusCode};

use crate::types::{ChatRequest, ModelResponse};

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("upstream error: status={status}, message={message}")]
    UpstreamError { status: u16, message: String },

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("timeout")]
    Timeout,
}

#[async_trait]
pub trait ProviderConfig: Send + Sync {
    async fn transform_request(
        &self,
        request: &ChatRequest,
        api_key: &str,
        headers: &mut HeaderMap,
    ) -> Result<(Method, String, serde_json::Value), ProviderError>;

    async fn transform_response(
        &self,
        status: StatusCode,
        headers: &HeaderMap,
        body: Bytes,
    ) -> Result<ModelResponse, ProviderError>;

    fn base_url(&self) -> &str;
}
