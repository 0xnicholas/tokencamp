use std::cell::RefCell;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use pin_project_lite::pin_project;

use crate::provider::{ChunkTransformer, ProviderError};
use crate::types::{ChatRequest, ModelResponse, OpenAiChunk};

pin_project! {
    pub struct StreamWrapper<S> {
        #[pin]
        inner: S,
        request: ChatRequest,
        transformer: ChunkTransformer,
        collected_chunks: RefCell<Vec<OpenAiChunk>>,
        buffer: Vec<u8>,
    }
}

impl<S> StreamWrapper<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>>,
{
    pub fn new(inner: S, request: ChatRequest, transformer: ChunkTransformer) -> Self {
        Self {
            inner,
            request,
            transformer,
            collected_chunks: RefCell::new(Vec::new()),
            buffer: Vec::new(),
        }
    }

    pub fn build_full_response(&self) -> Option<ModelResponse> {
        let chunks = self.collected_chunks.borrow();
        if chunks.is_empty() {
            return None;
        }
        let last = chunks.last().unwrap();
        let content: String = chunks
            .iter()
            .filter_map(|c| c.choices.first())
            .filter_map(|cc| cc.delta.content.clone())
            .collect();

        let finish_reason = chunks
            .iter()
            .filter_map(|c| c.choices.first().and_then(|cc| cc.finish_reason.clone()))
            .last()
            .unwrap_or_else(|| "stop".to_string());

        Some(ModelResponse {
            id: last.id.clone().unwrap_or_default(),
            object: "chat.completion".to_string(),
            created: last.created,
            model: last.model.clone().unwrap_or_default(),
            choices: vec![crate::types::Choice {
                index: 0,
                message: crate::types::Message {
                    role: "assistant".to_string(),
                    content,
                },
                finish_reason,
            }],
            usage: last.usage.clone().unwrap_or(crate::types::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }),
        })
    }
}

impl<S> Stream for StreamWrapper<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>>,
{
    type Item = Result<OpenAiChunk, ProviderError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                this.buffer.extend_from_slice(&bytes);

                // 尝试从 buffer 中提取完整的 SSE 事件
                if let Some((event_type, data)) = extract_sse_event(this.buffer) {
                    let chunk = (this.transformer)(this.request, &event_type, &data);
                    this.collected_chunks.borrow_mut().push(chunk.clone());
                    Poll::Ready(Some(Ok(chunk)))
                } else {
                    // buffer 中还没有完整事件，等待更多数据
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 从 SSE buffer 中提取完整事件。
/// SSE 格式: "event: <type>\ndata: <json>\n\n"
/// 返回 (event_type, data_json) 并将已消费的字节从 buffer 移除
fn extract_sse_event(buffer: &mut Vec<u8>) -> Option<(String, serde_json::Value)> {
    let text = String::from_utf8_lossy(buffer);

    // 查找完整的事件（以 \n\n 结尾）
    if let Some(event_end) = text.find("\n\n") {
        let event_text = &text[..event_end];
        let consumed = event_end + 2;

        let mut event_type = String::new();
        let mut data_str = String::new();

        for line in event_text.lines() {
            if let Some(t) = line.strip_prefix("event: ") {
                event_type = t.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data: ") {
                data_str = d.trim().to_string();
            }
        }

        // 从 buffer 中移除已消费的字节
        buffer.drain(..consumed);

        if data_str.is_empty() || data_str == "[DONE]" {
            return None;
        }

        if let Ok(data) = serde_json::from_str(&data_str) {
            return Some((event_type, data));
        }
    }

    None
}
