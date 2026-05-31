# Tokencamp

LLM API Gateway built with Rust + Axum.

## v0.1 — Hello Gateway

Minimal single-provider proxy. Proxies `/v1/chat/completions` to OpenAI.

### Quick Start

1. Set your OpenAI API key:
   ```bash
   export OPENAI_API_KEY=sk-...
   ```

2. Start the gateway:
   ```bash
   cargo run -p gateway
   ```

3. Send a request:
   ```bash
   curl -X POST http://localhost:3000/v1/chat/completions \
     -H "Authorization: Bearer sk-tc-dev-key-1" \
     -H "Content-Type: application/json" \
     -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Hello"}]}'
   ```

### Docker

```bash
cd docker
OPENAI_API_KEY=sk-... docker compose up
```

### Architecture

- `gateway/` — Axum HTTP server, auth, config
- `tokencamp-core/` — Provider adapters, HTTP handler (no web framework dependency)

### What's Next

See [ROADMAP.md](ROADMAP.md) for version plan.
