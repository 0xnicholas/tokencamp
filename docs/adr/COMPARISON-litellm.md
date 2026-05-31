# Tokencamp vs LiteLLM 对比分析

**日期**: 2026-05-30

---

## 1. 技术栈对比

| 维度 | Tokencamp | LiteLLM |
|------|-----------|---------|
| 语言 | Rust 1.85+ | Python 3.10+ |
| Web 框架 | Axum | FastAPI |
| 异步运行时 | Tokio | asyncio + uvicorn/gunicorn |
| 数据库 | SQLx + PostgreSQL 16 | Prisma ORM + PostgreSQL |
| 缓存 | redis-rs + LRU (DualCache) | redis-py + in-memory (DualCache) |
| 配置 | YAML + Redis Pub/Sub 热更新 | YAML / 环境变量 / DB |
| 编译方式 | AOT 编译为二进制 | 解释执行 |
| 内存占用 | ~20-50MB（Python 的 1/5-1/10） | ~200-500MB（多进程） |
| 冷启动 | 毫秒级 | 秒级（含 import 开销） |
| 并发模型 | 真多线程（无 GIL） | 多进程 (gunicorn) 弥补 GIL |
| 类型安全 | 编译期（所有权+类型系统） | 运行时（类型注解，可选） |

**核心差异**：Tokencamp 用编译期安全换开发速度，LiteLLM 用开发速度换运行时性能。两者在同一问题的两端做权衡。

---

## 2. 架构对比

### 分层模型

```
Tokencamp:                          LiteLLM:
                                    
Gateway (axum)                      proxy/ (FastAPI)
  ├── Hook 链                        ├── auth/ (user_api_key_auth)
  ├── Router                         ├── hooks/ (max_budget_limiter, etc.)
  └── Core Library 调用              ├── route_llm_request.py
       │                             └── → litellm.acompletion()
       ▼                                   
tokencamp-core (纯 crate)           litellm/ (SDK)
  ├── Router                         ├── router.py (Router + Router.cache)
  ├── Provider 适配器                ├── llms/{provider}/chat/transformation.py
  ├── HttpHandler                    ├── llms/custom_httpx/llm_http_handler.py
  └── DualCache                      └── caching/
```

**关键区别**：
- Tokencamp 的 Core Library 是纯 trait 抽象，**编译期保证** Provider 适配器实现完整性
- LiteLLM 的 SDK 是动态 Python 模块，**运行时发现** Provider，更灵活但缺少编译期检查
- Tokencamp 的 Router 在 Core Library 中，LiteLLM 的 Router 在 SDK 中——两者定位一致
- LiteLLM 的 `proxy/` 层功能远多于 Tokencamp 的 `gateway/` 层（见功能对比）

### 请求处理流程

```
Tokencamp 路径:                     LiteLLM 路径:

POST /v1/chat/completions           POST /v1/chat/completions
  → Axum 路由解析                     → FastAPI 路由解析
  → Hook 链 (pre):                    → user_api_key_auth()
    认证 / 限流 / 护栏                   Redis 查 Key → miss → PG
  → Router.select_deployment()        → hooks: max_budget_limiter
    排除 cooldown + 策略选择              parallel_request_limiter
  → HttpHandler (reqwest)             → route_llm_request()
    → transform_request()               → Router.route_request()
    → HTTP POST Provider                → litellm.acompletion()
    → transform_response()                 → BaseLLMHTTPHandler
  → Hook 链 (post):                           → transform_request()
    成本追踪 / 日志                              → HTTP POST Provider
                                                 → transform_response()
                                               ← ModelResponse
                                        ← response_cost 注入 _hidden_params
                                      → async_success_handler()
                                        → DBSpendUpdateWriter (Redis queue)
```

**差异点**：
- LiteLLM 的成本计算在 SDK 层内置（`completion_cost()`），Tokencamp 在 Hook 中作为关注点注入
- LiteLLM 的成本通过 `_hidden_params` 隐式传递，Tokencamp 通过显式的 Hook context 传递
- LiteLLM 的认证和限流是独立步骤，Tokencamp 统一为 Hook 链

---

## 3. 功能覆盖对比

### 核心 Gateway 功能

| 功能 | Tokencamp | LiteLLM | 备注 |
|------|:---------:|:-------:|------|
| `/v1/chat/completions` | ✅ | ✅ | 两者核心 |
| `/v1/embeddings` | ✅ | ✅ | |
| `/v1/models` | ✅ | ✅ | |
| SSE 流式响应 | ✅ | ✅ | |
| API Key 认证 | ✅ | ✅ | |
| TPM/RPM 限流 | ✅ | ✅ | |
| Provider 适配器 | ✅ (trait) | ✅ (class) | 接口风格不同 |
| 路由策略 | ✅ 5 种 | ✅ 5+ 种 | shuffle/lowest-cost/lowest-latency/usage-based/tag-based |
| 容错 (cooldown/retry/fallback) | ✅ | ✅ | |
| 成本追踪 (SpendLog) | ✅ | ✅ | |
| 批量写入 (Redis queue + flush) | ✅ | ✅ | 设计几乎相同 |
| DualCache | ✅ | ✅ | 设计几乎相同 |
| YAML 配置 | ✅ | ✅ | |
| 热更新 (Deployment) | ✅ | ✅ | |
| 健康检查 | ✅ | ✅ | |
| Prometheus Metrics | ✅ | ✅ | |
| 结构化日志 (JSON) | ✅ | ✅ | |
| OpenTelemetry Tracing | ✅ | ✅ | LiteLLM 有更多的 callback 集成 |
| 密钥管理 (加密存储) | ✅ | ✅ | |
| Secret Manager 集成 | ✅ | ✅ | |

### 高级功能

| 功能 | Tokencamp | LiteLLM | 备注 |
|------|:---------:|:-------:|------|
| 多租户 (Org/Team/User) | ❌ 已移除 | ✅ | LiteLLM 内置层级：Organization → Team → User → Key |
| 预算管理 (Budget) | ❌ | ✅ | 支持 key/team/user 级别预算 |
| 预算重置 (定时) | ❌ | ✅ | 后台任务周期性重置 |
| `/v1/messages` (Anthropic 原生) | ❌ | ✅ | Passthrough |
| `/v1/images/generations` | Phase 2 | ✅ | |
| `/v1/audio/*` | Phase 2 | ✅ | |
| `/v1/batches` | ❌ | ✅ | |
| `/v1/files` | ❌ | ✅ | |
| `/v1/fine_tuning` | ❌ | ✅ | |
| `/v1/rerank` | ❌ | ✅ | |
| `/v1/responses` (OpenAI) | ❌ | ✅ | |
| `/v1/vector_stores` | ❌ | ✅ | |
| Vertex AI passthrough | ❌ | ✅ | |
| Gemini passthrough | ❌ | ✅ | |
| 通用 Passthrough | ❌ | ✅ | `/*` 直通任意 Provider |
| LLM Response Cache | ❌ | ✅ | LiteLLM 有完整的语义缓存 |
| Prompt Management | ❌ | ✅ | |
| Guardrails (内容安全) | ❌ | ✅ | |
| Custom SSO (OIDC/Google) | ❌ | ✅ | |
| Slack/Email 告警 | ❌ | ✅ | 每周/每月 spend 报告 |
| API Key 自动轮换 | ❌ | ✅ | |
| Spend Report (CSV 导出) | ❌ | ✅ | |
| Langfuse / Datadog 集成 | ❌ | ✅ | LiteLLM 有 20+ integrations |
| Fine-tuning API | ❌ | ✅ | |
| MCP 协议支持 | ❌ | ✅ | 实验性 |
| Skills 注入 | ❌ | ✅ | |
| A2A 协议 (Agent-to-Agent) | ❌ | ✅ | 实验性 |

### 数据模型对比

```
Tokencamp (简化后，4 表):             LiteLLM (Prisma schema):
                                      
ApiKey                                LiteLLM_VerificationToken (Key)
SpendLog                              LiteLLM_SpendLogs
Deployment                            LiteLLM_OrganizationTable
Credential                            LiteLLM_TeamTable
                                      LiteLLM_UserTable
                                      LiteLLM_BudgetTable
                                      LiteLLM_EndUserTable
                                      LiteLLM_ModelTable (Deployment)
                                      LiteLLM_Config (pass_through)
                                      + 10+ 其他表
```

Tokencamp 刻意简化了数据模型（4 表 vs LiteLLM 的 20+ 表），这是 ADR-012 决策的直接结果。

---

## 4. 设计哲学对比

| | Tokencamp | LiteLLM |
|---|---|---|
| **定位** | 聚焦单一场景：高性能 LLM API Gateway | 全功能 AI Gateway 平台 |
| **原则** | 「Gateway 不做多租户，不做过于丰富的功能」 | 「Everything in one box」 |
| **复杂度管理** | ADR 驱动的显式边界决策 | 有机增长，功能逐步沉淀 |
| **扩展方式** | Trait 对象 + trait 实现（编译期保证） | 类继承 + 动态注册（运行时灵活） |
| **测试策略** | 纯函数优先，Provider 适配器可脱离网络单测 | 同样重视单测（100+ 翻译测试文件） |
| **新增 Provider 成本** | 实现 trait 的两个方法 | 实现 Config 类的两个方法 |
| **运维复杂度** | 低（PostgreSQL + Redis + 二进制部署） | 中高（Prisma 迁移 + 多后台任务 + 20+ 集成） |

---

## 5. Tokencamp 的独特优势

1. **编译期安全**：SQLx 编译时 SQL 验证 + Rust 所有权模型 = 整类运行时错误在编译期消除。LiteLLM 即使有大量测试，也无法达到同等级别的保证。

2. **资源效率**：20-50MB 内存 vs 200-500MB，毫秒冷启动 vs 秒级。在边缘部署、高密度 K8s 装箱场景下优势显著。

3. **架构纯度**：由于没有历史包袱，架构决策可以更纯粹地遵循「关注点分离」。例如 Hook 系统和 Router 的边界、双层架构的明确职责。

4. **可预测延迟**：无 GC、无 GIL。在高并发流式场景下，延迟不会因为 GC 暂停出现尖刺。

## 6. LiteLLM 的独特优势

1. **生态完整性**：20+ integrations、passthrough、预算管理、多租户、告警、SSO 等开箱即用。Tokencamp 需要大量额外开发才能达到同等覆盖。

2. **生产成熟度**：已有数千家企业使用，经过大规模生产验证。代码中的 edge case 处理远比 Tokencamp 丰富。

3. **LLM SDK 生态**：Python 的 OpenAI SDK、Anthropic SDK 等是官方维护的，Rust 生态中这些基本是社区 crate，质量参差不齐。

4. **开发速度**：Python 的快速原型能力在 AI 场景下有天然优势。新增 Provider、新增 endpoint 的迭代速度明显更快。

---

## 7. 结论

Tokencamp 不是 LiteLLM 的替代品，而是**不同工程哲学下的同领域产品**：

- 选择 Tokencamp 如果：性能是硬指标、需要编译期安全保证、场景聚焦在核心 chat completion 路由、有 Rust 团队
- 选择 LiteLLM 如果：需要全功能平台、依赖丰富的集成生态、团队是 Python 栈、需要快速迭代

Tokencamp 从 LiteLLM 参考了大量高质量的设计决策（DualCache、批量写入、Hook 系统、路由策略、Provider 适配器），但主动选择了一条**更窄但更精**的路径——去掉多租户、去掉非核心 endpoint、用 Rust 的编译期安全换取运行时可靠性。
