# ADR-001: k-LLM 多模型共识决策架构

**状态**: 已采纳

**日期**: 2026-05-30

**决策者**: Nicholas

## 背景

单一 LLM 的输出存在固有偏见和错误风险——模型可能在特定领域有盲区、产生幻觉、或被训练数据中的偏见影响。Palantir 等公司的 AI 实践表明：**不过度依赖单一模型**，通过多个异构 LLM 独立处理同一任务，再通过对齐与表决消除偏见和错误，可以显著提升输出的可靠性和质量。

k-LLM 是将此范式工程化为独立服务的项目。与 Tokencamp API Gateway 的关系：k-LLM 是上游编排层，Tokencamp 是下游模型调用层。两者职责正交，独立演进。

## 决策

k-LLM 作为**独立项目**构建，不与 Tokencamp Gateway 耦合。其核心架构为：

```
API 层 ─► 编排引擎 ─► 模型调用层（通过 Tokencamp 或其他 OpenAI 兼容端点）
              │
              ├── 模板查找
              ├── 并行调用 k 个模型
              ├── 对齐层：k 个异构响应 → 统一结构
              └── 共识引擎：Voting / Ranking / Confidence-weighted / Reranking
```

### 技术栈

- **Rust + Axum + Tokio**：与 Tokencamp 一致，高性能异步编排
- **PostgreSQL**：模板定义、验证集、调用记录、监控指标
- **Redis**：模板热更新通知、限流计数（可选，单实例模式不需要）
- **reqwest**：HTTP 客户端，调用下游模型端点

### 与 Tokencamp 的边界

| | k-LLM | Tokencamp Gateway |
|---|---|---|
| 核心操作 | 1 请求 → k 模型 → k 响应 → 1 结果 | 1 请求 → 1 模型 → 1 响应 |
| 关注点 | 结果对齐、共识投票、分歧处理 | 认证、限流、路由、计费、容错 |
| 延迟模型 | 取决于最慢模型 | 选最快模型 |
| 成本模型 | 成本 × k | 优化单次调用成本 |

两者通过 HTTP 集成。k-LLM 持有 Tokencamp 的 API Key（独立 service account），将 Tokencamp 视为普通的 `/v1/chat/completions` 端点。

**所有模型调用——包括 k 个主模型、对齐模型、Critic 模型——统一通过 Tokencamp 走**。不做直连 Provider 的特例，保持调用链路一致，统一享受 Gateway 的认证、限流、容错和计费能力。

## 理由

1. **职责分离**：编排逻辑（对齐、投票、合成）和网关逻辑（认证、限流、容错）是完全不同的领域。混合在一起会使两个领域同时复杂化，且无法独立测试和演进。

2. **接口范式不兼容**：Tokencamp 的 `RoutingStrategy::select_deployment()` 返回单个 Deployment，而 k-LLM 需要 k 个并行结果。这不是策略差异，是接口层面的根本分歧。

3. **独立迭代**：k-LLM 的共识算法和对齐策略会持续演进（新投票方法、更好的对齐模型），与 Provider API 变更的节奏完全不同。

4. **部署灵活**：k-LLM 可以对接任意 OpenAI 兼容端点（Tokencamp、直连 Provider、甚至其他 Gateway），不绑定单一基础设施。

## 核心设计

### MVP 范围限定：非流式

**MVP 仅支持非流式（non-streaming）**。调用方提交 prompt，等待完整结果返回。理由：

1. **流式与共识机制根本冲突**：Voting 必须等 k 个模型全完成，无法在流式中间给调用方任何有意义的 token
2. **体验语义不清**：如果 k=3 个模型逐 token 生成不同内容，调用方看到的是什么？合并？交错？都有严重的体验问题
3. **Phase 2 再议**：流式场景需要完全不同的共识模型（如实时跨模型一致性评分），属于独立的设计空间

### 编排引擎

编排引擎是 k-LLM 的核心，包含 5 个组件：

**1. 模板查找**

模板是 k-LLM 的核心配置单元，预定义了「哪种场景用哪几个模型、怎么投票」。调用方指定 `template`，编排引擎查找配置并驱动后续流程。

**2. 并行调用**

使用 `FuturesUnordered` 并发发出 k 个 HTTP 请求到 Tokencamp。每个请求携带 `request_id` 用于追踪。

**部分失败处理**：

```
如果 k 个模型中 n 个失败/超时：
  有效模型数 = k - n
  
  如果 有效模型数 ≥ minimum_models（模板配置，默认 ceil(k/2)+1）：
    ─► 继续表决，标记 degraded = true
  如果 有效模型数 < minimum_models：
    ─► 返回错误 502，不进行表决（模型太少无共识意义）
```

`minimum_models` 默认值 `ceil(k/2)+1` 保证至少简单多数仍成立。例如 k=3 时至少需要 2 个，k=5 时至少需要 3 个。模板可以覆盖为更严格的值。

**3. 对齐层**

k 个模型的输出在文本形式上是异构的，需要对齐到统一结构才能投票。三种对齐方式：

| 对齐类型 | 方法 | 适用场景 |
|---------|------|---------|
| `structured` | 调用对齐模型（如 gpt-4o-mini，通过 Tokencamp）按模板 schema 抽取结构化 JSON | 分类、判断、结构化输出 |
| `free_text` | 不对齐，k 个原始输出直接传给 Critic 评审 | 创意/内容生成 |
| `classification` | 规则匹配 + 关键词提取，不调用额外模型 | 简单的二分类/多分类 |

**对齐模型调用**：对齐模型也通过 Tokencamp 调用，与其他模型共享同一认证通道。k 个响应的结构化提取可以**并发**进行——一个对齐模型调用同时处理 k 个原始输出（将 k 个输出组合成一个 batch prompt），成本为一次轻量模型调用而非 k 次。对齐模型的成本计入总成本上限。

**对齐模型失败处理**：

```
对齐模型调用失败：
  1. 重试 1 次（间隔 200ms）
  2. 仍失败 → 降级为 classification（规则匹配）
  3. classification 在此模板上不可用（schema 复杂）→ 返回错误 502
```

**值标准化器**：结构化提取之后，不同模型可能用不同术语表达同一概念（如 "SQL注入" / "SQL_INJECTION" / "sql injection"）。值标准化器基于模板配置的同义词映射做 fuzzy match 统一：

```yaml
alignment:
  value_normalization:
    vulnerability_type:
      "SQL注入": ["SQL_INJECTION", "sql injection", "SQL Injection", "sqli"]
      "XSS": ["跨站脚本", "Cross-Site Scripting", "xss attack"]
```

标准化后的值才进入共识引擎，避免术语差异被错误判定为「不一致」。

**4. 共识引擎**

四种共识策略：

| 策略 | 逻辑 | 适用场景 | 成本 |
|------|------|---------|------|
| **Voting** | 加权多数表决，达到 `vote_threshold` 采纳 | 分类、判断 | k × cost |
| **Ranking** | 加权 Borda count 排序融合 | 多候选方案优选 | k × cost |
| **Confidence-weighted** | 按模型输出的置信度加权投票 | 模型能输出 logprobs/confidence | k × cost |
| **Reranking / Critic** | Critic 模型阅读所有输出后排序或打分 | 质量差异微妙、需深度评审 | (k+1) × cost + 对齐 |

**权重在各策略中的语义**：

| 策略 | 权重语义 |
|------|---------|
| Voting | 每票乘以权重。weight=0.8 的模型，其票计为 0.8 而非 1。阈值检查基于加权票数/k |
| Ranking | Borda 得分乘以权重。weight=0.8 的模型，其排名得分打 8 折 |
| Confidence-weighted | 权重是置信度的乘数：`最终权重 = confidence × model_weight` |
| Reranking / Critic | 权重不影响 Critic 判断（Critic 独立评审），仅记录在 trace 中供分析 |

Voting 和 Ranking 是对称策略（k 个模型地位平等，权重只是微调），Confidence-weighted 和 Reranking 是非对称策略（存在一个额外的信号源或评审者）。

**Tie-breaker / Reviewer / Critic 角色固定使用模板中配置的模型，不为其开启 k-LLM**，以避免「谁评审评审者」的递归问题。

**Confidence-weighted 降级规则**：

```
如果模板要求 confidence，但部分模型不输出置信度：
  ─► 对这些模型，confidence 视为 0.5（中性）
  ─► 标记 low_confidence_fields 中注明哪些模型缺置信度

如果所有模型都不输出置信度（如所用模型均不支持 logprobs）：
  ─► 整个策略降级为 Voting（使用模型权重作为票权）
  ─► 返回中标注 strategy_degraded: "confidence_weighted → voting, reason: no model output confidence"
```

**5. 成本控制**

三层成本检查。**总成本包含所有模型调用：k 个主模型 + 对齐模型 + Critic 模型**。

**成本预检**（请求进入时）：

```
估算 = k × prompt_tokens × 各模型 avg_price + 对齐模型估算 + Critic 估算（如适用）
如果 估算 > max_cost → 拒绝 422
```

**成本追踪**（调用进行中）：

由于 MVP 为非流式，token usage 在响应完成时从 Tokencamp 返回的 `usage` 字段直接获取，无需本地 tokenizer。如果 Tokencamp 未返回 usage（罕见），降级为根据响应文本长度估算（`char_count / 4`）。

```
已消耗 = 所有已完成调用的 usage.total_tokens × 各模型单价
如果 已消耗 > max_cost：
  ─► 取消所有未完成的调用
  ─► 用已完成的结果继续表决
  ─► 返回中标注 cost_capped: true
```

**事后记录**：完整成本（含所有模型调用明细）写入 `kllm_invocations`。

### 数据流

```
POST /v1/k-llm/completions { "template": "code-audit", "prompt": "..." }

─► 模板查找 → 确定 k 个模型 + 策略 + 阈值 + 成本上限
─► 成本预检 → 估算（含对齐模型 + Critic 估算）vs max_cost
─► 并行调用 → k 个 HTTP 请求并发到 Tokencamp
─► 收集结果 → 等待全部返回或超时
      ↓ 有效模型数 ≥ minimum_models？
      ├── 否 → 返回 502
      └── 是 ↓
─► 对齐层 → k 个原始输出 → 统一结构化格式 + 值标准化
─► 共识引擎 → Voting / Ranking / Confidence-weighted / Reranking
─► 构造响应 → 返回结论 + consensus 详情 + cost + traces
```

**响应结构**：

```json
{
  "template": "code-audit",
  "result": {
    "verdict": "存在风险",
    "risk_level": "高",
    "explanation": "..."
  },
  "consensus": {
    "strategy": "voting",
    "models_participated": 3,
    "degraded": false,
    "verdict": {
      "存在风险": {"count": 3, "weighted": 2.8},
      "安全": {"count": 0, "weighted": 0},
      "threshold_met": true
    },
    "risk_level": {
      "高": {"count": 2, "weighted": 1.8},
      "中": {"count": 1, "weighted": 0.8},
      "threshold_met": false
    },
    "low_confidence_fields": ["risk_level"]
  },
  "cost": {
    "total": 0.12,
    "currency": "USD",
    "breakdown": {
      "gpt-5": 0.035,
      "claude-sonnet-4-5": 0.025,
      "gemini-2.5-pro": 0.045,
      "alignment_model": 0.005,
      "critic_model": 0.01
    }
  },
  "latency_ms": 1500,
  "request_id": "kllm_req_abc123"
}
```

### 模板管理

模板生命周期：定义 → 验证 → 上线 → 监控 → 迭代 → 废弃。

**定义**：YAML 配置，包含 models（含权重）、consensus 策略、alignment 方式、cost 限制、minimum_models。

**验证**：上线前用标注数据集跑验证。验证集是一组 `{input, expected_output}` 对，由领域专家标注。验证流程：

```
1. 用 draft 模板跑所有验证案例
2. 对比 k-LLM 输出 vs 标注
3. 计算指标：
   - 准确率（k-LLM 输出与标注一致的比例）
   - 单模型最佳准确率（k 个模型各自独立跑验证集，取最高分）
   - 增值率 = (kLLM准确率 - 最佳单模型准确率) / (1 - 最佳单模型准确率)
   - 共识率（Voting 达到阈值的比例）
4. 增值率 ≥ 上线阈值 → 允许上线。否则不值得 k 倍成本
```

增值率的设计逻辑：如果单模型已经 95% 准确率，k-LLM 最多能提升 5 个百分点。如果 k-LLM 只提升了 0.5%，增值率 = 0.5/5 = 10%。若模板要求增值率 ≥ 20%，则拒绝。

**监控**：持续采集调用量、成功率、共识率、平均成本/延迟、降级率、模型贡献度。关键是**模型贡献度**——如果某个模型在特定模板上永远是少数派（贡献度 < 0.3），触发替换建议。

**热更新**：模板变更走 DB + Redis Pub/Sub，不重启服务。

### 数据库 Schema

```sql
-- 模板定义
CREATE TABLE kllm_templates (
    id              TEXT PRIMARY KEY,
    config          JSONB NOT NULL,            -- 完整 YAML → JSON
    status          TEXT NOT NULL DEFAULT 'draft',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 验证集
CREATE TABLE kllm_validations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id     TEXT REFERENCES kllm_templates(id),
    cases           JSONB NOT NULL,            -- [{input, expected_output}, ...]
    last_result     JSONB,                     -- 最近一次验证结果（指标 + 分歧列表）
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 调用记录
CREATE TABLE kllm_invocations (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id         TEXT NOT NULL,
    request_id          TEXT NOT NULL UNIQUE,
    result              JSONB,                 -- 完整响应（含 result, consensus, cost）
    cost_total          DOUBLE PRECISION,
    latency_ms          INTEGER,
    models_participated INTEGER,
    minimum_models      INTEGER,
    degraded            BOOLEAN DEFAULT false,
    cost_capped         BOOLEAN DEFAULT false,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 模板聚合指标（定时刷新，每 5 分钟）
CREATE TABLE kllm_template_metrics (
    template_id         TEXT PRIMARY KEY,
    total_calls         INTEGER DEFAULT 0,
    success_rate        DOUBLE PRECISION,
    consensus_rate      DOUBLE PRECISION,
    avg_cost            DOUBLE PRECISION,
    p50_latency_ms      INTEGER,
    p95_latency_ms      INTEGER,
    degradation_rate    DOUBLE PRECISION,      -- degraded=true 的比例
    model_contributions JSONB,                 -- {"gpt-5": 0.9, "claude": 0.85, ...}
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 部署拓扑

```
调用方 → k-LLM Service (axum)
           ├── PostgreSQL (模板 + 调用记录 + 指标)
           ├── Redis (热更新 + 限流，单实例可选)
           └── Tokencamp Gateway → LLM Providers
                                    ↑
              对齐模型、Critic 模型 ———┘ (统一通过 Tokencamp)
```

k-LLM 是无状态编排层，水平扩展简单。所有实例共享 PostgreSQL + Redis，监听 Pub/Sub 同步模板变更。单实例模式不需要 Redis。

所有模型调用——无论是 k 个主模型、对齐模型还是 Critic——都经过 Tokencamp。这使调用链路可追踪、成本可审计、容错可复用。k-LLM 本身不做模型直连。

### 延迟分析

端到端延迟 = max(模型1延迟, ..., 模型k延迟) + 对齐层延迟 + 共识引擎延迟

```
典型场景 (k=3, Voting, 结构化对齐):
  = max(1.5s, 1.0s, 2.0s)     ← 并行调用，取最慢
  + 0.3s                        ← 对齐模型（一次调用处理 k 个输出）
  + ~0ms                        ← Voting 纯计算
  = 2.3s

高成本场景 (k=3, Reranking, 结构化对齐):
  = max(1.5s, 1.0s, 2.0s)     ← 并行调用
  + 0.3s                        ← 对齐模型
  + 0.8s                        ← Critic 评审
  = 3.1s
```

对实时性要求高的场景，使用 Voting 策略 + Classification 对齐（无对齐模型调用），延迟 ≈ max(模型延迟)。

## 后果

- **正面**：多模型共识显著提升输出可靠性；模板化降低使用门槛；与 Tokencamp 解耦，独立迭代；无状态架构易于扩展；所有模型调用链路统一，成本可审计
- **负面**：成本和延迟是单模型的 k 倍，不是所有场景都值得；非流式 MVP 首次响应时间较长
- **缓解**：模板验证环节的增值率指标确保只在「值得 k 倍成本」的场景启用；`minimum_models` 和成本上限防止失控
- **负面**：对齐层的准确性直接影响共识质量——如果对齐提取错误，后续投票建立在错误基础上
- **缓解**：结构化提取失败时降级为 classification；值标准化器的同义词映射由领域专家维护；自由文本场景避免中间提取，直接交给 Critic 评审
- **负面**：Reranking 策略引入额外模型调用成本（k+1+对齐），且 Critic 本身可能有偏见
- **缓解**：Critic 角色固定在模板中配置，可替换和验证

## 备选方案

- **作为 Tokencamp 的内置路由策略**：实现简单，但 `RoutingStrategy` trait 的接口范式（返回单个 Deployment）与 k-LLM 的 k 个并行调用根本冲突。强行兼容会使 trait 膨胀，破坏 Router 的简洁性。
- **纯客户端实现**：由调用方自己并行调 k 个模型并投票。不依赖额外服务，但每个调用方都要实现对齐和共识逻辑，无法沉淀模板知识，也无法跨调用方共享监控和迭代。
- **作为 Tokencamp 的 Hook**：在 post_call hook 中做多模型并联。但 Hook 的定位是「单请求生命周期内的横切关注点」，不适合发起额外请求（增延迟、改语义）。
