# ADR-015: Deployment 健康检查与探测

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp Router 需要感知每个 Deployment 的健康状态，以便在调度时排除不健康的端点。仅靠被动失败检测（调用失败后 cooldown）存在一个问题：如果一个 Deployment 变慢了但没完全挂掉，或者长时间没被调用，Router 不知道它的真实状态。

## 决策

采用 **主动健康探测 + 被动 cooldown** 的双重机制。

### 主动探测

后台定时任务每隔 `health_check_interval`（默认 30 秒）对所有 Deployment 发起轻量级健康检查：

```
健康检查任务（tokio::time::interval 定时循环，tokio::spawn 后台运行）:
  for deployment in all_deployments:
    ├── 检查 deployment 是否正在 cooldown → 跳过（无需探测已知不健康的）
    ├── 调用 Provider 端点的一个轻量级方法：
    │   - Chat Provider: /v1/models (极低成本，不产生 token 费用)
    │   - 若 Provider 不支持 /models → 用 HEAD /
    ├── 超时判定: 5s 内无响应 → 不健康
    ├── 错误判定: 5xx → 不健康
    └── 记录健康状态到 Redis: deployment:{id}:health → {status, last_check, latency_ms}
```

### 健康状态 TTL

健康检查结果写入 Redis，带 TTL：

```
deployment:{id}:health → {"status": "healthy", "last_check": 1717000000, "latency_ms": 120}
```

TTL 设为 `health_check_interval * 3`（90 秒）。如果 3 个周期没有更新（任务挂了或 Redis 写入失败），状态自动过期，Router 视为 `unknown`。

### Router 中的使用

`select_deployment()` 在过滤 cooldown 之后，可配置是否进行健康过滤：

```yaml
router_settings:
  enable_health_check_routing: true          # 启用健康过滤
  health_check_staleness_threshold: 60       # 超过 60s 未更新的状态视为 stale，降级为 unknown
```

健康过滤规则：
- `healthy` → 正常参与调度
- `unhealthy` → 排除（同 cooldown）
- `unknown`（状态过期或从未探测）→ 降级参与调度，但降低优先级（排在 healthy 之后）
- `stale`（探测间隔内未更新但未过期）→ 正常参与，但视为可能有风险

### 健康检查的懒执行

避免对从未被调用的 Deployment 做无意义探测：

```
Deployment 创建后 → 不立即探测
第一次被 Router 选中后 → 加入探测列表
连续 10 分钟无调用 → 移出探测列表（减少对 Provider 的无效请求）
```

### 探测与 Cooldown 的协作

| 场景 | 主动探测 | 被动 Cooldown |
|------|---------|---------------|
| Deployment 完全挂掉 | 探测失败 → 标记 unhealthy → 排除 | 调用失败 N 次 → cooldown → 排除 |
| Deployment 变慢 | 探测延迟升高 → 不排除，但 latency 指标可被 lowest-latency 策略感知 | 无明显失败，不触发 cooldown |
| Deployment 间歇性故障 | 可能探测成功（不幸赶上好的时刻）| 连续失败触发 cooldown，更可靠 |
| 长期无调用 | 定期探测保持状态新鲜 | 无调用则无信号 |

两者互补：主动探测提供基线健康，被动 cooldown 捕获瞬时故障。

## 后果

- **正面**: Router 对 Provider 健康有主动感知，避免将请求发到已知故障端点
- **负面**: 健康检查本身消耗资源：N 个 Deployment × 每 30s 一次 HTTP 请求 = 一定量的出站流量
- **缓解**: 懒执行（只探测活跃 Deployment）。如果 Deployment 数量超 100，考虑增加探测间隔或批量化
- **负面**: `/v1/models` 端点不一定总是可用（某些第三方代理不支持），需要 fallback 到 HEAD /

## 备选方案

- **仅被动 cooldown**: 简单，但首次请求打到故障 Deployment 的用户体验差
- **外部监控（Prometheus blackbox）**: 健康检查由外部系统负责，Router 查询 Prometheus。运维复杂但解耦
- **Provider 侧健康**: 依赖 Provider 自身返回的健康状态（很少有 Provider 提供）
