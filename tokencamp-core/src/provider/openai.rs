use async_trait::async_trait;
use bytes::Bytes;
use reqwest::{header::HeaderMap, Method, StatusCode};

use super::{ProviderConfig, ProviderError};
use crate::types::{ChatRequest, ModelResponse};

pub struct OpenAiConfig {
    base_url: String,
}

impl OpenAiConfig {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".to_string()),
        }
    }
}

#[async_trait]
impl ProviderConfig for OpenAiConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn transform_request(
        &self,
        request: &ChatRequest,
        api_key: &str,
        headers: &mut HeaderMap,
    ) -> Result<(Method, String, serde_json::Value), ProviderError> {
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key).parse().unwrap(),
        );
        headers.insert("Content-Type", "application/json".parse().unwrap());

        let body = serde_json::to_value(request)?;
        Ok((Method::POST, "/v1/chat/completions".to_string(), body))
    }

    async fn transform_response(
        &self,
        status: StatusCode,
        _headers: &HeaderMap,
        body: Bytes,
    ) -> Result<ModelResponse, ProviderError> {
        if !status.is_success() {
            let error: serde_json::Value = serde_json::from_slice(&body)?;
            let message = error["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }

        let response: ModelResponse = serde_json::from_slice(&body)?;
        Ok(response)
    }
}
