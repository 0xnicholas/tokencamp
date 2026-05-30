# ADR-014: 流式响应处理

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

LLM API 的核心体验之一是流式输出（Server-Sent Events, SSE）——用户可以在生成过程中逐 token 看到结果，而不是等待完整响应。流式处理引入了三个独特挑战：(1) 不同 Provider 的 SSE 格式不同；(2) 需要在流未完成时计算 token 数和成本；(3) 客户端断开连接时需正确释放上游连接。

## 决策

流式处理分为两层：**Provider 层适配** 和 **Gateway 层透传**。

### Provider 层：StreamTransformer

每个 Provider 的流式 chunk 格式不同。`transform_response()` 负责将 Provider 的流式 chunk 转换为统一的 OpenAI SSE chunk 格式。

```rust
pub trait ProviderConfig {
    /// 返回一个 chunk 转换器，将 Provider chunk → OpenAI chunk
    fn chunk_transformer(&self) -> fn(ProviderChunk) -> OpenAiChunk;
}

/// 包装原始流，提供统一接口
/// 使用 RefCell 实现内部可变性——迭代时 push chunk，结束后读取
pub struct StreamWrapper<S> {
    inner: S,
    transformer: fn(ProviderChunk) -> OpenAiChunk,
    collected_chunks: RefCell<Vec<OpenAiChunk>>,
}

impl<S> StreamWrapper<S>
where
    S: Stream<Item = Result<ProviderChunk, StreamError>> + Unpin,
{
    /// 流结束后，从收集的所有 chunk 构造完整响应对象（用于 token 计数和成本计算）
    pub fn build_full_response(&self) -> ModelResponse {
        let chunks = self.collected_chunks.borrow();
        // 从所有 SSE chunks 中提取 usage 信息，合并为标准 ModelResponse
        ...
    }
}
```

### Gateway 层：SSE 透传

Gateway 收到 `StreamWrapper` 后，通过 Axum 的 `Sse` 响应类型直接透传给客户端：

```rust
async fn chat_completions_stream(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let response = router.acompletion_stream(request).await?;

    let stream = response.map(|chunk| {
        let json = serde_json::to_string(&chunk).unwrap();
        Ok(Event::default().data(json))
    });

    Ok(Sse::new(stream))
}
```

### 流式成本计算

流式响应在流结束后才能知道完整 token 数。`StreamWrapper::build_full_response()` 从收集的 chunks 中提取 usage 信息，构造标准 `ModelResponse`。`CostTracker` hook 在流结束后异步计算成本并写入 SpendLog。

### 客户端断开处理

```
客户端断开 HTTP 连接
  → Stream 被 drop（Rust 的 RAII 自动释放资源）
  → tokio 的 CancellationToken 或 Drop 触发上游 Provider 连接取消
  → 释放连接池中的连接（reqwest 的 Drop 行为）

如果流已完成（正常结束 [DONE]），不需要取消
如果流还在进行中（客户端提前关闭），中断上游请求
```

### 连接池管理

流式请求的 HTTP 连接不能被连接池复用（因为响应体没读完）。处理方式：

- 流式请求使用独立的 `reqwest::Client` 实例（默认不开启连接池）
- 连接在流结束后或取消时自动释放（Rust 的 RAII + Drop）
- 非流式请求共享带连接池的 `reqwest::Client` 以提升性能

## 后果

- **正面**: 客户端体验与直接调 OpenAI 一致。Provider 差异在 Core Library 内部消化
- **负面**: `StreamWrapper` 收集所有 chunk 以计算最终成本（通过内部 `RefCell<Vec<OpenAiChunk>>` 在迭代中可变写入），对长输出可能占用较多内存
- **缓解**: 大部分 LLM 输出 token 在 4096-16384 范围，内存占用可控（约 50-200KB）。对于极端长输出，可以配置 `max_output_tokens` 限制
- **负面**: 客户端提前断开后，上游 Provider 可能仍在生成 token（浪费成本）
- **缓解**: 发现客户端断开后立即可终止上游请求。某些 Provider（如 OpenAI）支持 `abort` 信号

## 备选方案

- **缓冲完整响应后再返回**: 简单但失去流式体验，且大响应内存占用高
- **不做格式转换，直接透传 Provider 原生 SSE**: 最快，但客户端需要适配每种 Provider 的格式，违背「统一 API」目标
