# ADR-012: 多租户功能延期至独立项目

**状态**: 已采纳（替代原多租户隔离模型）

**日期**: 2026-05-30

**决策者**: Nicholas

## 背景

原 ADR-012 定义了四层逻辑隔离的多租户模型（Organization → Team → ApiKey → Budget → SpendLog），用于支持多个付费客户共享同一套 Gateway 基础设施。

## 决策

**多租户功能从本项目移除**，由独立的项目负责。当前 Tokencamp 聚焦单一场景：作为 LLM API Gateway 提供统一接入、智能调度和流式处理能力，不内置多租户管理。

## 理由

1. **职责分离**: Gateway 的核心职责是「高效、可靠地调用 LLM」，多租户的认证/计费/隔离是独立领域。混合在一起会同时增加两个领域的复杂度。

2. **独立演进**: Gateway 和多租户层的迭代节奏不同。Gateway 需要紧跟 Provider API 变更（新模型、新协议），多租户层依赖计费模型和定价策略的调整。

3. **部署灵活**: 单租户部署场景（企业内部 Gateway）不需要多租户模块。分离后可按需组合。

## 影响范围

以下多租户相关概念从当前 ADR 中移除：

- `Organization` / `Team` 实体及其层级关系
- `org_id` / `team_id` 数据分区字段
- `max_budget` 预算限制
- `allowed_models` 模型白名单（每 Key 的模型访问限制）
- 多租户资源争抢缓解（优先级队列、全局限流）

保留的简化身份模型：

```
ApiKey → SpendLog（每 Key 独立追踪用量）
```

API Key 仅用于认证和 TPM/RPM 限流，不绑定租户层级。

## 后果

- **正面**: Gateway 核心逻辑更简洁，数据模型从 6 表（Org/Team/Key/Budget/SpendLog/Deployment）简化为 3 表（Key/SpendLog/Deployment）
- **正面**: 减少了多层 Hook 和权限检查链
- **负面**: 多租户能力需要独立项目建成后才能使用
- **缓解**: 独立项目作为 Gateway 的上游代理层部署，不在 Gateway 内部耦合

## 备选方案

- **保留多租户但标记为 Phase 3**: 仍会增加代码复杂度，且需要同时维护两套心智模型
- **通过配置可选启用**: 试图用 feature flag 控制多租户开关，但数据模型和 Hook 链的差异过大，flag 无法有效简化核心路径
