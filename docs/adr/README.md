# Tokencamp 架构决策记录 (ADR) 索引

**项目**: Tokencamp — LLM API Gateway  
**更新**: 2026-05-31  
**总 ADR 数**: 16  
**当前版本**: v0.8 (全部计划版本已完成)

---

## 架构总览

Tokencamp 是一个 Rust + Axum 构建的 LLM API Gateway，对外提供 OpenAI 兼容的 `/v1/chat/completions` 端点，内部通过 Router + Provider 适配器统一接入多模型提供商，并通过 Hook 系统注入认证、限流、计费等横切关注点。

```
客户端
  │  POST /v1/chat/completions (OpenAI 兼容, ADR-008)
  ▼
┌─────────────────────────────────────┐
│          Hook 链 (ADR-004)           │
│  pre:  限流 → 认证 → 护栏           │
│  post: 成本追踪 → 日志              │
│  error: 告警                        │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│        Router (ADR-010)             │
│  策略: shuffle / lowest-cost /      │
│        lowest-latency / usage-based │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│    Provider 适配器 (ADR-003)         │
│  OpenAI 格式 ↔ Provider 原生格式    │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│         LLM Providers               │
│  OpenAI / Anthropic / Google / ...  │
└─────────────────────────────────────┘

数据层 (ADR-005, 006, 009):
  PostgreSQL (权威源) + Redis (缓存/队列/限流) + DualCache

运维 (ADR-007, 011, 013, 015, 016):
  YAML 配置 + 热更新 | 三层容错 | 分层密钥 | 健康探测 | Prometheus+OTel
```

---

## 架构基础

| ADR | 决策 | 要点 |
|-----|------|------|
| [001](./001-rust-axum-tech-stack.md) | Rust + Axum + Tokio | 零成本抽象、无 GC、真多线程并发；内存占用为 Python 的 1/5-1/10 |
| [002](./002-gateway-core-dual-layer.md) | Gateway（axum）+ Core Library（纯 crate）双层 | Gateway 管「谁能调、调多少」；Core 管「怎么调」；可独立测试和分发 |

## 数据与缓存

| ADR | 决策 | 要点 |
|-----|------|------|
| [005](./005-prisma-postgresql-data-model.md) | SQLx + PostgreSQL 16 | 编译期 SQL 验证、类型安全、原生异步；实体：ApiKey / SpendLog / Deployment / Credential |
| [006](./006-redis-dual-cache.md) | Redis DualCache（内存 LRU + Redis） | 限流计数、Key 缓存、cooldown、spend 队列；数据权威源在 PG |
| [009](./009-spend-tracking-batch-write.md) | Redis Queue + 定时批量 Flush 到 PG | 请求路径不阻塞在 PG 写入；60s 批量 flush；幂等去重 |

## 请求处理

| ADR | 决策 | 要点 |
|-----|------|------|
| [003](./003-provider-adapter-pattern.md) | ProviderConfig trait 适配器模式 | 每个 Provider 只需实现 transform_request + transform_response；OpenAI 格式为规范中间格式 |
| [004](./004-hook-system.md) | Hook 注册表（pre/post/error 三阶段） | 横切关注点与路由解耦；按需 YAML 启用；比 tower Layer 更精细 |
| [008](./008-openai-compatible-api.md) | OpenAI ChatCompletion 兼容格式 | 行业标准；客户端零切换成本（改 base_url 即可）；内部统一中间格式 |
| [010](./010-router-strategies.md) | 可插拔 RoutingStrategy trait | shuffle / lowest-cost / lowest-latency / usage-based / tag-based；策略与 cooldown 过滤职责分离 |
| [014](./014-streaming.md) | StreamWrapper + SSE 透传 | Provider 层 chunk 转换 → Gateway 层 Sse 透传；RefCell 收集用于成本计算；客户端断开即释放 |

## 运维与安全

| ADR | 决策 | 要点 |
|-----|------|------|
| [007](./007-yaml-config-hot-reload.md) | YAML 配置源 + PostgreSQL 运行时 + Redis Pub/Sub 热更新 | Deployment 支持热更新；运行时参数需重启；GitOps 友好 |
| [011](./011-resilience-patterns.md) | Cooldown（熔断）→ Retry（重试）→ Fallback（降级）三层容错 | 连续失败 N 次触发 cooldown；指数退避 + jitter；Redis 共享 cooldown 状态 |
| [013](./013-secret-management.md) | 分层密钥：环境变量 / 加密数据库 / 外部 Secret Manager | AES-256-GCM 加密；密钥轮换支持；生产推荐外部 Secret Manager |
| [015](./015-health-checks.md) | 主动健康探测 + 被动 cooldown 双重机制 | 每 30s 探测活跃 Deployment；懒执行减少无效请求；互补覆盖 |
| [016](./016-observability.md) | Prometheus Metrics + 结构化 JSON Logging + OpenTelemetry Tracing | 9 个核心指标；Span 层级覆盖全请求路径；告警规则内置 |

## 范围决策

| ADR | 决策 | 要点 |
|-----|------|------|
| [012](./012-multi-tenant-isolation.md) | 多租户从本项目移除，延期至独立项目 | 职责分离；Gateway 聚焦统一接入和调度；简化数据模型 6 表→4 表 |

---

## 决策关系图

```
001 (技术栈)
 ├─► 002 (双层架构)
 │    ├─► 003 (Provider 适配器)
 │    │    └─► 008 (OpenAI 格式) ──► 014 (流式处理)
 │    ├─► 004 (Hook 系统)
 │    │    └─► 009 (成本追踪)
 │    └─► 010 (路由策略)
 │         └─► 011 (容错机制) ──► 015 (健康检查)
 │
 ├─► 005 (数据层)
 │    ├─► 006 (Redis 缓存)
 │    └─► 009 (批量写入)
 │
 ├─► 007 (配置热更新)
 ├─► 013 (密钥管理)
 └─► 016 (可观测性)

012 (多租户移除) ──► 影响 005 (数据模型简化), 009 (移除 Team 聚合)
```

---

## 外部项目

| 项目 | ADR | 关系 |
|------|-----|------|
| k-LLM 多模型共识决策 | [kllm/adr/001](../kllm/adr/001-kllm-architecture.md) | 独立上层项目，通过 HTTP 调用 Tokencamp 作为模型端点 |

## 参考资料

| 文档 | 说明 |
|------|------|
| [../ROADMAP.md](../ROADMAP.md) | 版本规划与路线图 |
| [COMPARISON-litellm.md](./COMPARISON-litellm.md) | Tokencamp vs LiteLLM 架构与功能对比 |

## 术语

| 术语 | 含义 |
|------|------|
| Deployment | 一个模型的一个具体端点（如 `gpt-5` via OpenAI, `gpt-5` via Azure） |
| Provider | LLM 提供商（OpenAI, Anthropic, Google 等） |
| Router | 根据策略从多个 Deployment 中选一个 |
| Hook | 在请求生命周期特定阶段注入的横切逻辑单元 |
| DualCache | 内存 LRU + Redis 双层缓存模式 |
| Cooldown | Deployment 连续失败后暂时排除的熔断机制 |
