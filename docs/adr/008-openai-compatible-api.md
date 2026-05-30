# ADR-008: OpenAI 兼容 API 格式

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 作为 API Gateway 对外暴露 LLM 调用接口，需要选择一种 API 格式作为对外约定。客户端可能使用各种 LLM 工具链（LangChain、Vercel AI SDK、OpenAI SDK、自定义 HTTP 客户端等）。

## 决策

对外暴露 **OpenAI ChatCompletion API 兼容格式** (`/v1/chat/completions`, `/v1/embeddings`, `/v1/models` 等)。

同时作为 Core Library 内部的规范中间格式——所有 Provider 的请求和响应都先转换到 OpenAI 格式，再交给上游。

## 理由

1. **行业标准**: OpenAI 的 API 格式已经成为 LLM 调用的 de facto 标准。几乎所有 LLM 工具链（LangChain、LlamaIndex、Vercel AI SDK、Dify 等）都原生支持 OpenAI 格式。

2. **零切换成本**: 用户当前使用 OpenAI SDK 的代码，只需把 `base_url` 改为 Tokencamp 地址，其余代码不变。

```rust
// 从 OpenAI 切换到 Tokencamp，只改 base_url
let client = Client::builder()
    .with_base_url("https://api.tokencamp.com/v1")
    .with_api_key("sk-tc-xxx")
    .build()?;
```

3. **统一内部中间格式**: Core Library 内的 Router、Hook 和所有 Provider 适配器都操作同一种请求/响应格式（通过 `serde` 序列化为 Rust 结构体）。新增 Provider 只需要实现「OpenAI → Provider」的转换 trait，不需要关心其他 Provider 的格式。

4. **Gateway 逻辑简化**: Gateway 层的 Hook（内容护栏、成本追踪）只需要解析一种 request/response 格式。不需要每个 Provider 写一遍。

5. **与 LiteLLM 一致**: 参考项目 LiteLLM 同样使用 OpenAI 兼容格式，生态和工具链完全对齐。

## 端点清单

| 端点 | 用途 | MVP |
|------|------|-----|
| `POST /v1/chat/completions` | 对话补全（核心） | ✅ |
| `POST /v1/embeddings` | 文本向量化 | ✅ |
| `GET /v1/models` | 列出可用模型 | ✅ |
| `POST /v1/images/generations` | 图片生成 | Phase 2 |
| `POST /v1/audio/transcriptions` | 语音转文字 | Phase 2 |
| `POST /v1/audio/speech` | 文字转语音 | Phase 2 |

## Provider 特有参数透传

OpenAI 格式作为基础，Provider 特有参数（如 Anthropic 的 `thinking`、Google 的 `safety_settings`）通过 `optional_params` 字段透传：

```json
{
  "model": "claude-sonnet-4-5",
  "messages": [...],
  "thinking": {"type": "enabled", "budget_tokens": 1024}
}
```

这些扩展参数在 Gateway 层不解析，直接透传给 Provider 适配器的 `transform_request()`。

## 后果

- **正面**: 生态兼容性最强，切换成本为零，内部格式统一
- **负面**: OpenAI 格式无法完美表达所有 Provider 的能力（如 Anthropic 的 tool_use 有更多嵌套结构）
- **缓解**: 核心功能对齐 OpenAI 格式，Provider 特有功能通过扩展字段透传

## 备选方案

- **自定义格式**: 最灵活，可以设计一套完美抽象所有 Provider 的格式。但需要所有客户端适配，生态割裂
- **多格式并存**: 同时暴露 OpenAI 格式和 Provider 原生格式（如 `/v1/messages` 给 Anthropic）。更灵活但 Gateway 复杂度翻倍
