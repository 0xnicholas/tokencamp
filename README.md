# Tokencamp

LLM API Gateway built with Rust + Axum.

## v0.4

Multi-provider API Gateway with pluggable routing strategies, rate limiting, and cost tracking.

### Features

- **OpenAI-compatible API**: `/v1/chat/completions`, `/v1/models`, `/v1/embeddings`
- **Anthropic support**: `/v1/chat/completions` (conversion) + `/v1/messages` (passthrough)
- **SSE streaming**: stream:true responses
- **5 routing strategies**: simple_shuffle, lowest_cost, lowest_latency, usage_based, tag_based
- **Rate limiting**: TPM/RPM per API key
- **Cost tracking**: spend logs with async batch write to PostgreSQL
- **Resilience**: retry with exponential backoff, cooldown, fallback chains
- **Admin API**: `/admin/keys/generate`, `/admin/keys`, `/admin/keys/{id}`
- **DualCache**: in-memory LRU + Redis
- **Health checks**: background deployment probing

### Quick Start

```bash
# Set API keys
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...

# Start
cargo run -p gateway

# Use
curl -X POST http://localhost:3000/v1/chat/completions \
  -H "Authorization: Bearer sk-tc-dev-key-1" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-chat","messages":[{"role":"user","content":"Hello"}]}'
```

### Docker

```bash
cd docker
OPENAI_API_KEY=sk-... docker compose up
```

### Architecture

- `gateway/` — Axum HTTP server, auth, config, hooks, router, admin API
- `tokencamp-core/` — Provider adapters, HTTP handler, streaming, cache, routing strategies

### Configuration

```yaml
# config/default.yaml
router_settings:
  routing_strategy: "lowest_latency"  # or simple_shuffle, lowest_cost, usage_based, tag_based
hooks:
  enabled:
    - parallel_request_limiter
    - cost_tracker
model_list:
  - model_name: deepseek-chat
    provider: openai
    litellm_params:
      model: deepseek-chat
      model_info:
        prompt_price: 0.27
        completion_price: 1.10
```

### What's Next

See [ROADMAP.md](ROADMAP.md) for version plan.
