# ADR-003: Provider 适配器模式

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 需要接入多个 LLM Provider（OpenAI、Anthropic、Google、Azure 等），每个 Provider 有自己的 API 格式、认证方式、流式协议和错误码。需要一个统一的模式来封装这些差异，使得上游（Gateway 和 Router）不需要感知 Provider 细节。

## 决策

采用 **ProviderConfig trait + transform_request/transform_response** 的适配器模式。每个 Provider 实现 `ProviderConfig` trait，只负责请求和响应的格式转换。HTTP 调用、连接池管理、重试等通用逻辑下沉到共享的 HTTP Handler 层。

```
请求进入（OpenAI 格式）
   │
   ▼
HttpHandler::complete()               ← 共享的 HTTP 编排器
   │
   ├── provider.transform_request()    ← 每个 Provider 自己实现
   ├── HTTP POST → Provider API
   └── provider.transform_response()   ← 每个 Provider 自己实现
```

## 理由

1. **最小接口面积**: 每个 Provider 只需要实现两个方法：`transform_request()` 和 `transform_response()`。HTTP 连接、SSL、代理、连接池、超时等全部由共享的 `reqwest` 客户端 Handler 处理。

2. **可独立测试**: 翻译逻辑是纯函数，可以在单测中直接构造输入，断言输出，不需要 Mock HTTP 调用。Rust 的 `#[cfg(test)]` 模块提供原生测试支持。

3. **新增 Provider 成本低**: 添加一个新 Provider 只需要创建 `tokencamp-core/src/llms/{provider}/chat/transformation.rs`，实现 trait 的两个方法。不需要修改任何 HTTP 层代码。

4. **OpenAI 格式作为规范格式**: 所有 Provider 的请求和响应都先转换为 OpenAI ChatCompletion 格式。Gateway 和 Router 只看到这一种格式，大幅降低复杂度。

## 接口定义

```rust
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ProviderConfig: Send + Sync {
    /// 将 OpenAI ChatCompletion 格式转换为 Provider 原生格式
    async fn transform_request(
        &self,
        model: &str,
        messages: &[Value],
        optional_params: &Value,
        headers: &mut HeaderMap,
    ) -> Result<Value, ProviderError>;

    /// 将 Provider 原生响应转换为标准的 ModelResponse
    /// 注意：接收已读取的 body bytes 而非 reqwest::Response，
    /// 因为 Response body 只能消费一次（Rust 所有权模型）
    async fn transform_response(
        &self,
        model: &str,
        status: reqwest::StatusCode,
        headers: &HeaderMap,
        body: Bytes,
        request_data: &Value,
        messages: &[Value],
        optional_params: &Value,
    ) -> Result<ModelResponse, ProviderError>;
}
```

## 后果

- **正面**: 清晰的扩展点，新 Provider 接入成本极低
- **负面**: 某些 Provider 的特有功能（如 Anthropic 的 extended thinking）无法通过 OpenAI 格式完整表达（缓解：通过 `optional_params` 透传，在 Gateway 层暴露扩展字段）
- **负面**: OpenAI 格式作为中间层有轻微的性能开销（额外的 JSON Value 操作）。Rust 的 `serde_json::Value` 比 Python dict 更高效，但仍有分配开销。可通过 `serde` 的零拷贝反序列化优化热路径。

## 备选方案

- **每个 Provider 独立实现完整 HTTP 调用**: 更灵活，但代码重复严重，连接池、SSL、错误处理等逻辑每个 Provider 都要写一遍
- **Passthrough 模式**: 不做格式转换，直接透传原生请求和响应。对客户端更透明，但 Gateway 无法做统一的路由、限流和计费
