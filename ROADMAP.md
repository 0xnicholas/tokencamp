# Tokencamp Roadmap

**更新**: 2026-05-30

---

## 版本策略

采用语义化版本。**0.x 为开发阶段**，每个版本引入一组正交功能，版本之间不重叠、不推翻。**1.0 为生产就绪 (GA)**。

版本边界原则：
- 每个版本有一个明确的**主题**，让团队和早期用户有清晰预期
- 版本之间按**依赖关系**排序（如 Redite 必须在限流和高级路由之前）
- 每个版本必须是**可独立交付和演示**的增量

---

## v0.1 — 「Hello Gateway」单模型透传

**主题**: 证明 Rust + Axum 技术栈在 LLM Gateway 场景下的可行性。最小可用单元。

**时间**: 2-3 周

### 功能

| 功能 | 说明 |
|------|------|
| `POST /v1/chat/completions` | OpenAI 兼容，非流式 |
| 单 Provider 适配 (OpenAI) | 仅 transform_request + transform_response |
| API Key 认证 | 简单的 key 比对，存 YAML 配置 |
| YAML 静态配置 | 模型列表、api_key 从配置文件读取 |
| 项目骨架 | Cargo workspace (gateway + tokencamp-core)、迁移管理 |

### 不做

- 流式响应
- 多 Provider
- Redis
- 任何 Hook
- 任何路由策略（只有一个 Deployment，不需要选）
- 成本追踪
- 健康检查

### 交付物

```
gateway/               ← Axum 应用骨架
tokencamp-core/        ← ProviderConfig trait + OpenAI 实现
config/default.yaml    ← 单模型配置
```

### 依赖链

```
v0.1 ─► v0.2 的基础
```

---

## v0.2 — 「Multi-Provider」多模型接入 + 流式

**主题**: 接入多个 Provider，支持流式输出。至此，核心的模型调用链路完整。

**时间**: 3-4 周

### 功能

| 功能 | 说明 |
|------|------|
| 多 Provider 适配 | Anthropic、Google Gemini、Azure OpenAI |
| `POST /v1/chat/completions` SSE 流式 | StreamWrapper + SSE 透传 |
| 简单路由 | `simple-shuffle` 策略（无状态，随机选） |
| Cooldown 熔断 | 连续失败 N 次 → 标记 cooldown → Router 自动跳过 |
| Retry 重试 | 指数退避 + jitter，可重试错误自动重试 |
| ProviderConfig trait 完整实现 | 含 chunk_transformer、流式 transform_response |
| `GET /v1/models` | 列出可用模型 |

### 不做

- Redis（cooldown 存内存，单实例）
- 成本追踪
- 限流
- Fallback 策略（仅 cooldown + retry）

### 交付物

```
tokencamp-core/src/llms/anthropic/   ← Anthropic 适配器
tokencamp-core/src/llms/gemini/      ← Google Gemini 适配器
tokencamp-core/src/llms/azure/       ← Azure OpenAI 适配器
gateway/src/router/                  ← Router + simple_shuffle
gateway/src/resilience/              ← Cooldown + Retry 逻辑
```

### 依赖链

```
v0.1 ─► v0.2 ─► v0.3 的基础
          └─► Streaming 能力影响后续 Hook 设计（post_call 需等待流结束）
```

---

## v0.3 — 「Production MVP」缓存 + 限流 + 成本

**主题**: 引入 Redis，补齐生产环境必需的限流、成本追踪和健康检查。至此，具备 SaaS 服务的基本能力。

**时间**: 4-5 周

### 功能

| 功能 | 说明 |
|------|------|
| Redis DualCache | in-memory LRU + Redis 双层缓存 |
| API Key 缓存 | key_hash → ApiKey 信息，TTL 5min |
| TPM/RPM 限流 | pre_call hook: `ParallelRequestLimiter`，Redis INCR + TTL |
| 成本追踪 | post_call hook: `CostTracker` → Redis queue → 批量 Flush PG |
| PostgreSQL 集成 | SQLx 迁移、api_keys 表、spend_logs 表 |
| 健康检查 | 主动探测 + 被动 cooldown，Redis 共享状态 |
| Hook 系统 | ProxyHook trait + 注册表 + pre/post/error 三阶段 |
| Fallback 降级 | 配置 fallback 链，重试耗尽后降级到备选模型 |

### 不做

- 高级路由策略（lowest-latency, usage-based）
- Prometheus / OTel
- 加密存储
- 配置热更新
- Embeddings 端点

### 交付物

```
gateway/src/hooks/                   ← Hook 注册表 + ParallelRequestLimiter + CostTracker
gateway/src/cache/                   ← DualCache (in-memory + redis)
gateway/src/db/                      ← SQLx 查询 + 迁移文件
gateway/src/health/                  ← HealthChecker
migrations/001_initial_schema.up.sql ← api_keys + spend_logs
```

### 依赖链

```
v0.2 ─► v0.3 ─► v0.4 的基础
          └─► Redis 就绪后，高级路由策略和热更新才能开始
```

---

## v0.4 — 「Smart Gateway」高级路由 + 热更新 + 可观测性

**主题**: 智能调度、运行时配置变更和全链路可观测性。至此，功能对标 LiteLLM 的核心 Gateway 能力。

**时间**: 4-5 周

### 功能

| 功能 | 说明 |
|------|------|
| `lowest-latency` 策略 | 滑动窗口延迟追踪，基于 DualCache |
| `usage-based` 策略 | 按 TPM/RPM 剩余容量负载均衡 |
| `lowest-cost` 策略 | 按 model_info 单价排序 |
| `tag-based` 策略 | 按 Deployment 标签匹配请求标签 |
| 配置热更新 | Deployment 增删改通过管理 API → PG → Redis Pub/Sub |
| Prometheus Metrics | `/metrics` 端点，9 个核心指标 |
| 结构化 JSON 日志 | tracing-subscriber JSON layer |
| OpenTelemetry Tracing | Span 层级覆盖全请求路径，采样率 10% |
| `POST /v1/embeddings` | 文本向量化端点 |

### 不做

- 完整的管理 API
- 图片/音频端点
- 密钥加密存储
- 告警规则对接外部

### 交付物

```
gateway/src/router_strategies/       ← lowest_latency, usage_based, lowest_cost, tag_based
gateway/src/admin_api/               ← Deployment CRUD
gateway/src/observability/           ← Metrics + Tracing 初始
```

### 依赖链

```
v0.3 ─► v0.4 ─► v1.0 的基础
          └─► 所有核心 Gateway 功能在此版本就绪
```

---

## v1.0 — 「GA」生产就绪

**主题**: 安全加固、API 完整化、运维自动化和正式文档。对外承诺 API 稳定性。

**时间**: 4-6 周

### 功能

| 功能 | 说明 |
|------|------|
| 密钥加密存储 | AES-256-GCM，Credential 表，`ENCRYPTION_KEY` 环境变量 |
| Secret Manager 集成 | AWS Secrets Manager / GCP Secret Manager trait 实现 |
| `POST /v1/images/generations` | 图片生成端点 |
| `POST /v1/audio/transcriptions` | 语音转文字端点 |
| `POST /v1/audio/speech` | 文字转语音端点 |
| 管理 API 完整化 | Key CRUD、Credential CRUD、用度查询 |
| 告警规则 | PrometheusRule 内置，对接 AlertManager |
| CLI 工具 | `tokencamp` CLI（启动、迁移、密钥轮换） |
| 完整文档 | API 参考、部署指南、Provider 接入指南 |
| 安全审计 | 依赖审计、密钥泄露扫描、日志脱敏验证 |

### 不做

- 多租户（独立项目）
- k-LLM 集成（独立项目）
- Response Cache（Phase 2 考虑）
- 任何 Pass-through 端点
- UI Dashboard

### 交付物

```
docs/                    ← 完整文档站
cli/                     ← tokencamp CLI crate
docker-compose.prod.yml  ← 生产部署模板
k8s/                     ← K8s 部署清单
```

---

## 版本依赖关系总览

```
v0.1 (Hello Gateway)
  │  单模型透传，证明技术栈可行性
  ▼
v0.2 (Multi-Provider)
  │  多 Provider + 流式 + 简单路由 + Cooldown/Retry
  ▼
v0.3 (Production MVP)             ← 引入 Redis
  │  DualCache + 限流 + 成本追踪 + PG + 健康检查 + Fallback
  ▼
v0.4 (Smart Gateway)              ← Redis 就绪后解锁
  │  高级路由 + 热更新 + Prometheus/OTel + Embeddings
  ▼
v1.0 (GA)
     安全加固 + 完整 API + 文档 + CLI
```

---

## Post-1.0 方向（不承诺时间）

| 方向 | 说明 |
|------|------|
| Response Cache | 语义缓存（相同 prompt 直接返回缓存结果），降低 LLM 调用成本 |
| WebSocket 流式 | 支持 `ws://` 双向流式 |
| Multi-tenant Gateway | 独立项目，作为 Tokencamp 上游代理 |
| k-LLM 集成 | 在 Tokencamp 生态中提供 k-LLM 原生支持 |
| UI Dashboard | 用量监控、Key 管理、成本分析 |

---

## 技术债务与风险

| 风险 | 版本 | 缓解 |
|------|------|------|
| SQLx 宏依赖编译时数据库连接 | v0.3 | 使用 `query_as` 非宏版本作为 fallback；CI 运行 PG 服务 |
| Redis 单点故障 | v0.3 | Redis 数据允许丢失（限流/cooldown 可从 PG 重建）；v1.0 考虑 Sentinel |
| Provider SDK 生态弱于 Python | v0.2+ | 所有 Provider 适配手动实现 HTTP（用 reqwest），不依赖第三方 SDK |
| 流式连接泄漏 | v0.2 | Rust RAII + Drop 自动释放；连接池隔离 |
| 编译时间随 crate 增长 | v0.3+ | Workspace 拆分；增量编译；sccache |
