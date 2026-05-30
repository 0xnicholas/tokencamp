# ADR-006: Redis DualCache 作为共享状态层

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp Gateway 需要处理以下共享状态：

- **限流计数**: 每个 API Key 的 TPM/RPM 计数器（高并发读写）
- **API Key 缓存**: 避免每次请求都查 PostgreSQL
- **Deployment cooldown**: 失败的 Deployment 需要全局冷却
- **Spend 写入队列**: 批量写 SpendLog 到 PG 的缓冲队列
- **配置热更新**: 多个 Gateway 实例之间同步配置变更

这些状态需要在多实例部署时保持一致，但不需要 ACID 事务语义。

## 决策

使用 **Redis** 作为分布式共享状态层，封装为 **DualCache**（in-memory + Redis 双层缓存）模式。Redis 客户端使用 `redis-rs`（带 tokio 异步支持）。

```
读写路径:
  读: in-memory → miss → Redis → miss → PostgreSQL
  写: in-memory + Redis pipeline（异步批量同步）
```

参考 LiteLLM 的 `DualCache` 和 `InternalUsageCache` 设计，用 Rust trait 抽象缓存层：

```rust
#[async_trait]
pub trait CacheLayer: Send + Sync {
    async fn get(&self, key: &str) -> Option<String>;
    async fn set(&self, key: &str, value: &str, ttl: Duration);
    async fn del(&self, key: &str);
}

pub struct DualCache {
    in_memory: LruCache<String, CacheEntry>,  // lru crate
    redis: MultiplexedConnection,
}
```

## 理由

1. **性能**: 热点数据（API Key、限流计数）走 in-memory，Redis 作为分布式同步层。单次请求 0 次 Redis 调用（命中内存缓存）或 1 次（miss）。

2. **多实例一致性**: Gateway 是无状态 Pod，但限流计数和 cooldown 必须跨实例共享。Redis 的原子操作（INCR、TTL）天然适合这个场景。

3. **批量写入优化**: SpendLog 不逐条写 PG。先写到 Redis queue，每 60 秒批量 flush 到 PG。大幅降低 PG 写入压力。

4. **Redis 数据允许丢失**: 限流计数和 cooldown 状态丢失后可以从 PG 重建（重新查询 Key 信息、清空 cooldown）。确保「数据权威源」始终在 PG。

5. **Pub/Sub 配置同步**: 当 Deployment 变更时，通过 `PUBLISH channel:config_update` 通知所有 Gateway 实例从 PG 重新加载配置。

## DualCache 结构

```
DualCache
├── in_memory_cache: LRU Cache (进程内，极快)
└── redis_cache: Redis (跨实例，中等速度)

用途:
  - API Key 缓存：key_hash → UserAPIKeyAuth (TTL 5min)
  - TPM 计数器：key:{key_id}:tpm:{window} → count (TTL 60s)
  - RPM 计数器：key:{key_id}:rpm:{window} → count (TTL 60s)
  - Cooldown: deployment:{id}:cooldown → timestamp
  - Spend Queue: spend_queue → List<SpendEntry>
```

## 后果

- **正面**: 极低的查询延迟，多实例一致性，降低 PG 写入压力
- **负面**: 引入额外的运维组件（Redis），增加了系统复杂度
- **负面**: 内存缓存和 Redis 之间的同步窗口可能导致短暂不一致（如限流计数差几毫秒）
- **缓解**: 对于严格的一致性需求（如预算超限），在 pre_call hook 中做最终检查时直接查 PG

## 备选方案

- **纯 PostgreSQL**: 简单，但高并发下的限流计数查询会成为瓶颈。行锁和写放大严重
- **纯 in-memory**: 单实例下最快，但多实例部署时无法共享限流状态
- **etcd/Consul**: 一致性更强，但对于计数器场景太重，运维成本高
