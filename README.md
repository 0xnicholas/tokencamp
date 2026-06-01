# Tokencamp

LLM API Gateway built with Rust + Axum.

**Current**: v0.8 | **52 commits** | **18 tests** | **Zero build errors**

## Features

- **OpenAI-compatible API**: `/v1/chat/completions`, `/v1/models`, `/v1/embeddings`, `/v1/images/generations`, `/v1/audio/*`
- **Anthropic support**: Chat Completions + Messages passthrough
- **SSE streaming** with chunk transformation
- **5 routing strategies**: simple_shuffle, lowest_cost, lowest_latency, usage_based, tag_based
- **Rate limiting** (TPM/RPM per API key) with ParallelRequestLimiter hook
- **Cost tracking** with async batch write to PostgreSQL
- **Resilience**: retry + cooldown + fallback chains
- **Admin API**: `/admin/keys`, `/admin/deployments`
- **DualCache**: in-memory LRU + Redis
- **Response cache**: SHA-256 semantic cache with 5min TTL
- **AES-256-GCM encryption** for provider credentials
- **Prometheus /metrics** endpoint
- **Structured JSON logging** (tracing-subscriber)
- **CLI tool**: start, migrate, key-rotate, config-check
- **Health checks** with background probing

## Quick Start

```bash
export OPENAI_API_KEY=sk-...
cargo run -p gateway
```

```bash
curl -X POST http://localhost:3000/v1/chat/completions \
  -H "Authorization: Bearer sk-tc-dev-key-1" \
  -d '{"model":"deepseek-chat","messages":[{"role":"user","content":"Hello"}]}'
```

## Docker

```bash
cd docker && docker compose up
```

## Architecture

```
gateway/        Axum HTTP server, auth, hooks, router, admin API
tokencamp-core/ Provider adapters, HTTP handler, streaming, cache, routing strategies
cli/            tokencamp CLI
docs/           16 ADRs, specs, plans
```

## Docs

- [Architecture Decision Records](docs/adr/README.md) (16 ADRs)
- [Roadmap](ROADMAP.md)
- [LiteLLM Comparison](docs/adr/COMPARISON-litellm.md)
- [k-LLM Design](docs/kllm/adr/001-kllm-architecture.md)
