# ADR-011: 容错与韧性机制

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

调用外部 LLM API 不可避免会遇到失败：网络超时、Provider 限流（429）、服务不可用（503）、Token 配额耗尽等。Tokencamp 作为 API Gateway，需要在 Provider 失败时自动容错，而不是把错误直接返回给客户端。

## 决策

采用三层容错机制：**Cooldown（熔断）→ Retry（重试）→ Fallback（降级）**，按优先级依次触发。

### 执行流程

```
请求到达 Router
   │
   ▼
select_deployment()  ─── 排除 cooldown 中的 Deployment
   │
   ▼
调用 Provider
   │
   ├── 成功 → 返回 ✅
   │
   └── 失败 → 分类错误类型
         │
         ├── 可重试错误 (429, 5xx, 网络超时)
         │     │
         │     ├── 重试次数 < num_retries
         │     │     └── 等待 retry_after 秒 → 重新选择 deployment 重试
         │     │
         │     └── 重试次数耗尽
         │           └── 进入 fallback 逻辑
         │
         └── 不可重试错误 (4xx 非 429, 认证失败)
               └── 直接返回错误 ❌

fallback 逻辑:
   ├── 按 fallbacks 列表依次尝试备选模型
   │     gpt-5 失败 → gpt-5-fast 失败 → deepseek-v3
   ├── 每个 fallback 内部仍有自己的 retry 逻辑
   └── 所有 fallback 耗尽 → 返回 503 ❌

cooldown 触发:
   Deployment 连续失败 allowed_fails 次
   → 标记为 cooldown (TTL = cooldown_time 秒)
   → select_deployment() 自动跳过

cooldown 重置:
   一次成功调用 → 计数器清零
   冷却时间到期 → 计数器自动清零（Redis key TTL 过期）

   根据错误类型区分计数器行为：
   - 429 / 5xx / 网络超时：计入失败计数，触发 cooldown
   - 4xx 客户端错误（非 429）：不计入（不是 deployment 的问题）

   举例：deployment 连续失败 2 次，第 3 次成功 → 计数器清零
         deployment 连续失败 3 次 → cooldown 30s → 冷却到期后计数器为 0
```

## 配置参数

```yaml
router_settings:
  num_retries: 3              # 最大重试次数
  retry_after: 0.5            # 重试间隔基础值（秒），实际用指数退避
  allowed_fails: 3            # 进入 cooldown 前的连续失败次数
  cooldown_time: 30           # cooldown 持续时间（秒）
  fallbacks:                  # 降级链
    - gpt-5: ["gpt-5-fast"]
    - gpt-5-fast: ["deepseek-v3"]
```

## 重试策略细化

指数退避 + jitter：

```
等待时间 = min(retry_after * 2^attempt + random_jitter, max_retry_delay)
```

429（Rate Limit）响应优先使用 Provider 返回的 `Retry-After` 头。

## Cooldown 实现

Cooldown 状态存储在 Redis，key 格式：`deployment:{id}:cooldown`，value 为冷却截止时间戳，TTL 等于 `cooldown_time`。所有 Gateway 实例共享此状态，一个实例触发 cooldown 后其他实例立即感知。

## 后果

- **正面**: 客户端看到的是一个高可用的服务，Provider 故障被自动屏蔽
- **负面**: 重试和 fallback 会增加请求延迟。最坏情况下可能经历多次重试 + 多个 fallback 链
- **缓解**: 设置 `max_fallbacks` 限制降级深度。超过 3 层降级可能是配置问题，应触发告警

## 备选方案

- **无容错**: 错误直接透传。简单但不满足 SaaS 可用性要求
- **全自动 healing**: 用 ML 预测 Provider 健康度并预降级。过度设计，MVP 不需要
- **客户端重试**: 让客户端自己处理重试。但客户端不具备全局视角（如某些 deployment 已被其他请求触发 cooldown）
