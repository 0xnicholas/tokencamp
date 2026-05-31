use std::time::Duration;

use reqwest::header::HeaderMap;

use crate::provider::{ProviderConfig, ProviderError};
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
}
