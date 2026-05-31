use std::time::Duration;

use bytes::Bytes;
use futures::Stream;
use reqwest::header::HeaderMap;
use serde_json::{json, Value};

use crate::provider::{ProviderConfig, ProviderError};
use crate::streaming::StreamWrapper;
use crate::types::{ChatRequest, ModelResponse};

pub struct HttpHandler {
    client: reqwest::Client,
}

impl HttpHandler {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
        }
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

    /// Passthrough 模式：跳过 transform_response，直接返回原始 bytes
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
            let error: Value =
                serde_json::from_slice(&resp_body).unwrap_or(json!({"error":{"message":"unknown"}}));
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message: error["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            });
        }

        Ok(resp_body)
    }

    /// 流式请求
    pub async fn complete_stream(
        &self,
        request: &ChatRequest,
        provider: &dyn ProviderConfig,
        api_key: &str,
    ) -> Result<
        StreamWrapper<impl Stream<Item = Result<Bytes, reqwest::Error>>>,
        ProviderError,
    > {
        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let mut headers = HeaderMap::new();
        let (method, path, body) = provider
            .transform_request(&stream_request, api_key, &mut headers)
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
            let error: Value =
                serde_json::from_slice(&resp_body).unwrap_or(json!({"error":{"message":"unknown"}}));
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message: error["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            });
        }

        let transformer = provider.chunk_transformer().ok_or(ProviderError::Timeout)?;
        let stream = response.bytes_stream();
        Ok(StreamWrapper::new(stream, stream_request, transformer))
    }
}
