# ADR-007: YAML 驱动配置 + 热更新

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 的运维配置包括：模型列表（Deployment）、路由策略、Hook 启用列表、降级链、限流参数等。这些配置需要：
- 启动时加载
- 运行时通过管理 API 修改
- 多实例间同步变更
- 可版本控制（Git）

## 决策

采用 **YAML 文件作为配置源 + PostgreSQL 作为运行时状态 + Redis Pub/Sub 作为热更新通知** 的三层配置架构。

### 配置流

```
启动:  YAML → Config 对象 → Router 初始化 + DB 写入初始 Deployment
运行时: API 修改 → PostgreSQL 更新 → Redis PUBLISH → 各实例 reload
```

### 配置优先级

```
环境变量 ${VAR} > YAML 文件 > 代码默认值
```

YAML 中可以使用 `${TOKENCAMP_MASTER_KEY}` 语法引用环境变量，启动时解析。

## 理由

1. **YAML 可读性**: 模型列表、路由策略等配置结构天然是树状的，YAML 的可读性远好于 JSON 或环境变量。

2. **GitOps**: YAML 文件放在 `config/` 目录，纳入 Git 版本控制。配置变更可走 PR review，可追溯。

3. **热更新**: API 驱动的运行时配置变更（如新增 Deployment）通过 PG + Redis Pub/Sub 推送到所有实例。不需要重启或重载配置。

4. **环境变量注入敏感信息**: `api_key: ${OPENAI_API_KEY}` 这样的模式确保密钥不写入 YAML 文件，也不进入 Git 历史。

5. **单一配置源**: 启动时从 YAML 加载所有配置，包括模型列表、路由策略、Hook 设置。避免配置分散在多个文件中。

## 配置示例

```yaml
general_settings:
  master_key: ${TOKENCAMP_MASTER_KEY}

model_list:
  - model_name: gpt-5
    litellm_params:
      model: openai/gpt-5
      api_key: ${OPENAI_API_KEY}

router_settings:
  routing_strategy: "lowest-latency"
  cooldown_time: 30

hooks:
  enabled:
    - parallel_request_limiter
    - cost_tracker
```

## 热更新范围

配置分为两类：

| 类型 | 示例 | 存储位置 | 热更新 |
|------|------|---------|--------|
| Deployment 配置 | model_list, litellm_params | YAML → PG | ✅ 通过管理 API + Redis Pub/Sub |
| 运行时参数 | routing_strategy, cooldown_time, hooks.enabled | YAML 仅作启动默认值 | 不热更新，需重启 |

运行时参数不热更新的原因：这些参数影响 Router 和 Hook 的实例化行为（如更换路由策略需要重新初始化 Router），热更新的收益不足以覆盖状态不一致的风险。Deployment 列表的增删改是高频运维操作，必须支持热更新。

## 后果

- **正面**: 声明式配置，可版本控制，Deployment 支持热更新
- **负面**: 配置文件 + 数据库双重状态源，需要保证一致性（启动时 YAML 覆盖 DB？DB 覆盖 YAML？）
- **解决**: 启动时 YAML 是「声明期望状态」，对比 DB 现状后执行 diff（新增的不存在的、跳过已存在的、标记 YAML 中有但 DB 中没有需要清理的）

## 备选方案

- **纯环境变量**: 适合简单场景，但模型列表这种复杂结构用环境变量极难表达
- **纯数据库**: 运行时灵活，但无法 Git 版本控制，且首次部署需要手动插数据
- **纯 YAML（无热更新）**: 简单，但每次改模型配置都需要重启，SPOF
