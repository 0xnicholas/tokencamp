# Tokencamp Roadmap

**更新**: 2026-05-31

---

## 已完成版本 (v0.1 - v0.8)

| 版本 | 主题 | 核心交付 |
|------|------|---------|
| v0.1 | Hello Gateway | Rust + Axum 骨架，单 Provider 透传 |
| v0.2 | Multi-Provider | Anthropic 适配（Chat + Messages），SSE 流式，Retry/Cooldown |
| v0.3 | Production MVP | PostgreSQL，DualCache，Hook 系统，TPM/RPM 限流，成本追踪，Admin API |
| v0.4 | Smart Gateway | 5 种可插拔路由策略，Prometheus /metrics，Embeddings 端点 |
| v0.5 | Security | AES-256-GCM 加密，SecretManager trait，Credentials 表 |
| v0.6 | Media API | Images + Audio 端点 |
| v0.7 | Operations | CLI 工具（start/migrate/key-rotate/check），Prometheus 告警规则 |
| v0.8 | Optimization | Response Cache（SHA-256 语义缓存，5min TTL） |

---

## Post-0.x 方向

| 方向 | 说明 |
|------|------|
| Multi-tenant Gateway | 独立项目，作为 Tokencamp 上游代理层 |
| k-LLM 集成 | 多模型共识决策服务 |
| UI Dashboard | 用量监控、Key 管理、成本分析 |
| WebSocket 流式 | `ws://` 双向流式支持 |
