# Tokencamp Roadmap

**更新**: 2026-05-31

---

## 版本策略

采用语义化版本。**0.x 为开发阶段**，每个小版本引入一组正交功能，版本之间不重叠、不推翻。每个版本有明确主题，可独立交付和演示。

---

## v0.1 — 「Hello Gateway」单模型透传 ✅

证明 Rust + Axum 技术栈可行性。单 Provider 透传，YAML 配置。

---

## v0.2 — 「Multi-Provider」多模型 + 流式 ✅

Anthropic 适配（Chat 转换 + Messages Passthrough）、SSE 流式、Cooldown + Retry。

---

## v0.3 — 「Production MVP」DB + 缓存 + 限流 ✅

PostgreSQL 集成、DualCache、Hook 系统、TPM/RPM 限流、成本追踪、Admin API、Fallback。

---

## v0.4 — 「Smart Gateway」高级路由 + 可观测性 ✅

5 种可插拔路由策略、Embeddings 端点、Prometheus /metrics、结构化日志、部署管理 API。

---

## v0.5 — 「Security」密钥安全

**主题**: 密钥加密存储和安全加固。

| 功能 | 说明 |
|------|------|
| 密钥加密存储 | AES-256-GCM，Credential 表，`ENCRYPTION_KEY` 环境变量 |
| Secret Manager 集成 | AWS Secrets Manager / GCP Secret Manager trait 实现 |
| 安全审计 | 依赖审计、密钥泄露扫描、日志脱敏验证 |

---

## v0.6 — 「Media API」多媒体端点 + 管理完善

**主题**: 图片/音频生成端点，管理 API 补全。

| 功能 | 说明 |
|------|------|
| `POST /v1/images/generations` | 图片生成端点 |
| `POST /v1/audio/transcriptions` | 语音转文字端点 |
| `POST /v1/audio/speech` | 文字转语音端点 |
| 管理 API 完善 | 用量查询、Credential CRUD、Spend 报表 |

---

## v0.7 — 「Operations」运维工具 + 文档

**主题**: CLI 工具、告警、部署文档。

| 功能 | 说明 |
|------|------|
| CLI 工具 | `tokencamp` CLI（启动、迁移、密钥轮换、配置校验） |
| 告警规则 | PrometheusRule 内置，对接 AlertManager |
| 部署文档 | K8s 清单、生产部署指南 |
| API 文档 | OpenAPI / Swagger |

---

## v0.8 — 「Optimization」性能 + 缓存

**主题**: 语义缓存、连接池优化、流式增强。

| 功能 | 说明 |
|------|------|
| Response Cache | 语义缓存，降低重复调用成本 |
| 连接池优化 | HTTP 连接复用调优 |
| WebSocket 流式 | `ws://` 双向流式支持 |

---

## Post-0.x 方向

| 方向 | 说明 |
|------|------|
| Multi-tenant Gateway | 独立项目，作为 Tokencamp 上游代理层 |
| k-LLM 集成 | 多模型共识决策服务 |
| UI Dashboard | 用量监控、Key 管理、成本分析 |

---

## 版本依赖关系总览

```
v0.1 ✅ → v0.2 ✅ → v0.3 ✅ → v0.4 ✅
                                  │
                                  ▼
                              v0.5 (安全)
                                  │
                                  ▼
                              v0.6 (多媒体)
                                  │
                                  ▼
                              v0.7 (运维)
                                  │
                                  ▼
                              v0.8 (优化)
```

---

## 当前实现进度

| 版本 | 状态 | commits |
|------|:--:|---------|
| v0.1 | ✅ | — |
| v0.2 | ✅ | — |
| v0.3 | ✅ | — |
| v0.4 | ✅ | 45 total |
