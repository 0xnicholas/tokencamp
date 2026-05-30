# ADR-010: Router 可插拔调度策略

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 的核心目标之一是「高效分发」——当一个模型有多个 Deployment 时（如 gpt-5 部署在 OpenAI 原生和 Azure 两个端点），需要一套策略来决定选哪个。不同场景对策略的需求不同：成本敏感型场景选最便宜的，延迟敏感型选最快的，高吞吐场景需要负载均衡。

## 决策

Router 接受一个可插拔的 `routing_strategy` 参数，运行时按策略选择 Deployment。所有策略共享同一个 trait `RoutingStrategy`。

```rust
use async_trait::async_trait;

#[async_trait]
pub trait RoutingStrategy: Send + Sync {
    /// 从候选列表中选出一个 Deployment
    async fn select_deployment(
        &self,
        model_name: &str,
        deployments: &[Deployment],
        request_kwargs: &Value,
    ) -> Result<Deployment, RouterError>;

    /// 记录成功调用，供延迟/吞吐量策略更新指标
    async fn track_success(&self, deployment: &Deployment, response: &ModelResponse);

    /// 记录失败，供策略调整权重
    async fn track_failure(&self, deployment: &Deployment, error: &RouterError);
}
```

## 内置策略

| 策略名 | 逻辑 | 适用场景 |
|--------|------|---------|
| `simple-shuffle` | 随机加权选择，权重可配置 | MVP 默认，无状态 |
| `lowest-cost` | 按 `model_info.cost_per_1k` 排序，选最便宜的 | 成本敏感 |
| `lowest-latency` | 基于滑动窗口的平均延迟，选最快的 | 用户体验优先 |
| `usage-based` | 按当前 TPM/RPM 剩余容量，选负载最低的 | 高吞吐均衡 |
| `tag-based` | 按 Deployment 标签匹配请求标签 | 按能力/价格标签路由 |

## Router 与策略的职责边界

Router 在调用策略的 `select_deployment()` 之前，先执行一系列过滤：

```
请求 model_name="gpt-5"
  → Router 查找所有匹配 "gpt-5" 的 Deployment（通过 alias 解析）
  → 过滤 cooldown 中的 Deployment（deployment:{id}:cooldown 在 Redis 中存在）
  → 剩余的候选列表传给策略

策略只负责：从给定的候选列表中，根据自身逻辑选出一个最优的
策略不负责：模型匹配、cooldown 过滤
```

这个边界设计确保策略是纯粹的「选择算法」，不掺杂运维和权限逻辑。

## 策略选择机制

Router 初始化时根据 `routing_strategy` 参数实例化对应策略，运行时对每个请求调用 `select_deployment()`。

```rust
let router = Router::new(
    model_list,
    Box::new(LowestLatencyStrategy::new(dual_cache)),
);
```

添加自定义策略：实现 `RoutingStrategy` trait，以 trait object 传入 Router 构造函数。

## 指标收集

`lowest-latency` 和 `usage-based` 策略依赖运行时指标：

- **延迟**: 在 `track_success()` 中记录响应时间，维护滑动窗口均值（最近 100 次调用）
- **TPM/RPM**: 通过 Redis INCR + TTL 计数，`select_deployment()` 时读取当前窗口值

策略不直接操作 Redis，而是通过 `RoutingStrategy` trait 的 `DualCache` 抽象层。这使策略可以在单测中用 `FakeCache`（纯 in-memory 实现）替代 Redis。

## 后果

- **正面**: 调度策略和 Router 核心逻辑解耦，可独立演进和测试
- **负面**: `usage-based` 和 `lowest-latency` 策略依赖 Redis 收集指标，纯单机部署下精度下降
- **缓解**: MVP 使用 `simple-shuffle`（无状态），延迟和用量策略在 Phase 2 引入，此时 Redis 已就绪

## 备选方案

- **硬编码策略**: 代码中 if-else 选择，简单但新增策略需要改 Router 源码
- **外部决策服务**: 调度策略作为独立微服务，Router 查询它做决策。灵活但增加延迟和故障点
