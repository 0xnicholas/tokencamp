use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::HeaderMap;
use serde_json::{json, Value};

use crate::provider::{ProviderConfig, ProviderError};
use crate::streaming::StreamWrapper;
use crate::types::{ChatRequest, ModelResponse, OpenAiChunk};

#[derive(Clone)]
pub struct HttpHandler {
    client: Arc<reqwest::Client>,
}

impl HttpHandler {
    pub fn new() -> Self {
        Self {
            client: Arc::new(
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(60))
                    .build()
                    .expect("Failed to create HTTP client"),
            ),
        }
    }

    pub fn client(&self) -> Arc<reqwest::Client> {
        self.client.clone()
    }

    pub async fn complete(
        &self,
        request: &ChatRequest,
        provider: &dyn ProviderConfig,
        api_key: &str,
    ) -> Result<ModelResponse, ProviderError> {
        let mut headers = HeaderMap::new();
        let (method, path, body) = provider
            .transform_request(request, api_key, &mut headers)
            .await?;

        let url = format!("{}{}", provider.base_url(), path);

        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let resp_headers = response.headers().clone();
        let resp_body = response.bytes().await?;

        provider
            .transform_response(status, &resp_headers, resp_body)
            .await
    }

    pub async fn complete_raw(
        &self,
        request: &ChatRequest,
        provider: &dyn ProviderConfig,
        api_key: &str,
    ) -> Result<Bytes, ProviderError> {
        let mut headers = HeaderMap::new();
        let (method, path, body) = provider
            .transform_request(request, api_key, &mut headers)
            .await?;

        let url = format!("{}{}", provider.base_url(), path);

        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let resp_body = response.bytes().await?;

        if !status.is_success() {
            let error: Value = serde_json::from_slice(&resp_body)
                .unwrap_or_else(|_| json!({"error":{"message":"unknown"}}));
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message: error["error"]["message"].as_str().unwrap_or("unknown").to_string(),
            });
        }

        Ok(resp_body)
    }

    pub async fn complete_stream_owned(
        self: Arc<Self>,
        provider: Arc<dyn ProviderConfig>,
        request: ChatRequest,
        api_key: String,
    ) -> Result<
        Pin<Box<dyn futures::Stream<Item = Result<OpenAiChunk, ProviderError>> + Send>>,
        ProviderError,
    > {
        let mut stream_request = request;
        stream_request.stream = Some(true);

        let mut headers = HeaderMap::new();
        let (method, path, body) = provider
            .transform_request(&stream_request, &api_key, &mut headers)
            .await?;

        let url = format!("{}{}", provider.base_url(), path);

        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let resp_body = response.bytes().await?;
            let error: Value = serde_json::from_slice(&resp_body)
                .unwrap_or_else(|_| json!({"error":{"message":"unknown"}}));
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message: error["error"]["message"].as_str().unwrap_or("unknown").to_string(),
            });
        }

        let transformer = provider
            .chunk_transformer()
            .ok_or(ProviderError::Timeout)?;
        let byte_stream = response.bytes_stream();
        let wrapper = StreamWrapper::new(byte_stream, stream_request, transformer);
        Ok(Box::pin(wrapper))
    }
}
