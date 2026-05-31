use async_trait::async_trait;
use bytes::Bytes;
use reqwest::{header::HeaderMap, Method, StatusCode};
use serde_json::{json, Value};

use super::{ChunkTransformer, ProviderConfig, ProviderError};
use crate::types::{ChatRequest, ChunkChoice, Delta, ModelResponse, OpenAiChunk};

pub struct AnthropicConfig {
    mode: AnthropicMode,
    base_url: String,
    litellm_model: String,
}

pub enum AnthropicMode {
    Chat,
    Messages,
}

impl AnthropicConfig {
    pub fn new(base_url: Option<String>, litellm_model: String, mode: AnthropicMode) -> Self {
        Self {
            mode,
            litellm_model,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        }
    }
}

#[async_trait]
impl ProviderConfig for AnthropicConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn is_passthrough(&self) -> bool {
        matches!(self.mode, AnthropicMode::Messages)
    }

    fn chunk_transformer(&self) -> Option<ChunkTransformer> {
        match self.mode {
            AnthropicMode::Chat => Some(anthropic_chunk_to_openai),
            AnthropicMode::Messages => None,
        }
    }

    async fn transform_request(
        &self,
        request: &ChatRequest,
        api_key: &str,
        headers: &mut HeaderMap,
    ) -> Result<(Method, String, Value), ProviderError> {
        headers.insert(
            "x-api-key",
            api_key
                .parse()
                .map_err(|_| ProviderError::SerializationError(serde_json::Error::io(
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid api key"),
                )))?,
        );
        headers.insert(
            "anthropic-version",
            "2023-06-01"
                .parse()
                .map_err(|_| ProviderError::SerializationError(serde_json::Error::io(
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid header"),
                )))?,
        );
        headers.insert(
            "Content-Type",
            "application/json"
                .parse()
                .map_err(|_| ProviderError::SerializationError(serde_json::Error::io(
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid header"),
                )))?,
        );

        match self.mode {
            AnthropicMode::Chat => {
                let body = convert_chat_to_anthropic(request, &self.litellm_model);
                Ok((Method::POST, "/v1/messages".to_string(), body))
            }
            AnthropicMode::Messages => {
                let body = serde_json::to_value(request)?;
                Ok((Method::POST, "/v1/messages".to_string(), body))
            }
        }
    }

    async fn transform_response(
        &self,
        status: StatusCode,
        _headers: &HeaderMap,
        body: Bytes,
    ) -> Result<ModelResponse, ProviderError> {
        if !status.is_success() {
            let error: Value = serde_json::from_slice(&body)?;
            let message = error["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(ProviderError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }
        // Anthropic Chat 模式响应需要转换为 ModelResponse
        let anthropic_resp: Value = serde_json::from_slice(&body)?;
        let content = anthropic_resp["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["text"].as_str())
            .unwrap_or("")
            .to_string();

        let prompt_tokens = anthropic_resp["usage"]["input_tokens"]
            .as_u64()
            .unwrap_or(0) as u32;
        let completion_tokens = anthropic_resp["usage"]["output_tokens"]
            .as_u64()
            .unwrap_or(0) as u32;

        Ok(ModelResponse {
            id: anthropic_resp["id"].as_str().unwrap_or("").to_string(),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: anthropic_resp["model"].as_str().unwrap_or("").to_string(),
            choices: vec![crate::types::Choice {
                index: 0,
                message: crate::types::Message {
                    role: "assistant".to_string(),
                    content,
                },
                finish_reason: anthropic_resp["stop_reason"]
                    .as_str()
                    .unwrap_or("stop")
                    .to_string(),
            }],
            usage: crate::types::Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }
}

fn convert_chat_to_anthropic(request: &ChatRequest, model: &str) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(1024),
        "messages": [],
    });

    let system_texts: Vec<_> = request
        .messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| json!({"type": "text", "text": m.content}))
        .collect();
    if !system_texts.is_empty() {
        body["system"] = json!(system_texts);
    }

    let messages: Vec<_> = request
        .messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();
    body["messages"] = json!(messages);

    if let Some(t) = request.temperature {
        body["temperature"] = json!(t);
    }
    if request.stream.unwrap_or(false) {
        body["stream"] = json!(true);
    }

    body
}

fn anthropic_chunk_to_openai(
    _request: &ChatRequest,
    event_type: &str,
    data: &Value,
) -> OpenAiChunk {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    match event_type {
        "message_start" => OpenAiChunk {
            id: data["message"]["id"].as_str().map(|s| s.to_string()),
            object: "chat.completion.chunk".to_string(),
            created: now,
            model: data["message"]["model"].as_str().map(|s| s.to_string()),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
            usage: None,
            done: false,
        },
        "content_block_delta" => {
            let text = data["delta"]["text"].as_str().unwrap_or("");
            OpenAiChunk {
                id: None,
                object: "chat.completion.chunk".to_string(),
                created: now,
                model: None,
                choices: vec![ChunkChoice {
                    index: data["index"].as_u64().unwrap_or(0) as u32,
                    delta: Delta {
                        role: None,
                        content: Some(text.to_string()),
                    },
                    finish_reason: None,
                }],
                usage: None,
                done: false,
            }
        }
        "message_delta" => {
            let usage = data["usage"].as_object().map(|u| crate::types::Usage {
                prompt_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                total_tokens: 0,
            });
            OpenAiChunk {
                id: None,
                object: "chat.completion.chunk".to_string(),
                created: now,
                model: None,
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: None,
                    },
                    finish_reason: data["delta"]["stop_reason"]
                        .as_str()
                        .map(|s| s.to_string()),
                }],
                usage,
                done: false,
            }
        }
        "message_stop" => OpenAiChunk {
            id: None,
            object: "chat.completion.chunk".to_string(),
            created: now,
            model: None,
            choices: vec![],
            usage: None,
            done: true,
        },
        _ => OpenAiChunk {
            id: None,
            object: "chat.completion.chunk".to_string(),
            created: now,
            model: None,
            choices: vec![],
            usage: None,
            done: false,
        },
    }
}
