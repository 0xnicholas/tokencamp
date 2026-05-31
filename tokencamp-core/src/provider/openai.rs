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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatRequest, Message};
    use bytes::Bytes;

    #[tokio::test]
    async fn test_transform_request_adds_auth_header() {
        let config = OpenAiConfig::new(None);
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: None,
            max_tokens: None,
            stream: None,
        };
        let mut headers = HeaderMap::new();

        let (method, path, body) = config
            .transform_request(&request, "test-key", &mut headers)
            .await
            .unwrap();

        assert_eq!(method, Method::POST);
        assert_eq!(path, "/v1/chat/completions");
        assert_eq!(headers["authorization"], "Bearer test-key");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(body["model"], "gpt-4o-mini");
    }

    #[tokio::test]
    async fn test_transform_response_success() {
        let config = OpenAiConfig::new(None);
        let body = Bytes::from(
            r#"{
                "id": "chatcmpl-123",
                "object": "chat.completion",
                "created": 1717000000,
                "model": "gpt-4o-mini",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hi!"},
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "total_tokens": 15
                }
            }"#,
        );

        let result = config
            .transform_response(StatusCode::OK, &HeaderMap::new(), body)
            .await
            .unwrap();

        assert_eq!(result.id, "chatcmpl-123");
        assert_eq!(result.choices[0].message.content, "Hi!");
        assert_eq!(result.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn test_transform_response_upstream_error() {
        let config = OpenAiConfig::new(None);
        let body = Bytes::from(
            r#"{"error": {"message": "Rate limit exceeded", "type": "rate_limit"}}"#,
        );

        let err = config
            .transform_response(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), body)
            .await
            .unwrap_err();

        match err {
            ProviderError::UpstreamError { status, message } => {
                assert_eq!(status, 429);
                assert!(message.contains("Rate limit"));
            }
            _ => panic!("expected UpstreamError"),
        }
    }
}
