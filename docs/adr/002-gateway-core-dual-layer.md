# ADR-002: Gateway + Core Library 双层架构

**状态**: 已采纳

**日期**: 2026-05-29

**决策者**: Nicholas

## 背景

Tokencamp 需要同时满足两个场景：(1) 作为托管 SaaS 服务对外提供 API Gateway；(2) 允许用户将核心能力作为库直接使用。此外，需要保证核心 LLM 调用逻辑（协议转换、路由、流式处理）可以独立于 Web 框架进行测试和演进。

## 决策

采用 **Gateway（Axum 应用）+ Core Library（纯 Rust crate）** 双层架构，参考 LiteLLM 的 `proxy/` + `litellm/` 分离模式。

```
gateway/       → Axum 应用：认证、限流、计费、管理 API
tokencamp-core/ → 核心 crate：Router、Provider 适配器、协议转换、缓存
```

Gateway 负责「谁能调、调多少、花多少钱」；Core Library 负责「怎么调模型」。

## 理由

1. **关注点分离**: Gateway 的横切关注点（认证、限流、计费、审计）和 Core 的 LLM 调用逻辑（协议适配、流式处理、调度的 Router）是完全不同的领域，混合在一起会导致难以维护的「大泥球」。

2. **独立测试**: Core Library 的每个 Provider 适配器可以脱离 Gateway、数据库和网络进行单元测试。LiteLLM 在 `tests/llm_translation/` 下有 100+ 个文件，全部是纯逻辑测试，不需要启动服务。

3. **独立分发**: Core Library 可以作为 `cargo add tokencamp-core` 或发布到 crates.io 独立分发，用户不需要启动 Gateway 服务就能直接使用统一接口调用任意模型。

4. **技术栈解耦**: Core Library 是纯 Rust crate，不依赖任何 Web 框架。如果未来 Gateway 层需要用 gRPC 重写，或者拆分为独立服务，Core Library 保持不变。

5. **水平扩展友好**: Gateway 无状态（扩缩容不影响），Core 里的 Router 和调度逻辑通过 Redis 共享状态，两个层可以独立扩缩。

## 后果

- **正面**: 清晰的边界，每一层可以独立测试、独立部署、独立迭代
- **负面**: 初期需要多建一套 crate 结构和构建流程（`Cargo.toml` 中配置 workspace 区分两个 crate）
- **负面**: 跨层调用可能增加序列化开销（缓解：Gateway 和 Core 部署在同一进程内，Rust 类型零成本传递）

## 备选方案

- **单体 Axum 应用**: 简单，但 Provider 适配器和路由逻辑耦合在 Web 框架里，难以独立测试和分发
- **微服务（Gateway + Router + Provider 各为独立服务）**: 扩展性最强，但 MVP 阶段严重过度设计，运维复杂度高
