# memory

> 管理调用方 Buffer、Allocator 边界、受限内存池和 Native 资源统计。

**RepositoryDeliveryPhase**：Foundation  
**ImplementationPriority**：I0  

内存所有权是 ABI 契约的一部分；本模块不保存托管对象，也不替上层拥有持久化数据。

## 负责范围

- 支持调用方提供 Buffer、所需长度查询和对齐约束。
- 提供有界 Allocator、临时批次和可回收内存池。
- 记录分配、释放、峰值和失败统计，供上层诊断和 Benchmark 消费。
- 在预算或容量达到上限时返回可分类错误。

## 不负责范围

- 不拥有 World、Chunk、ECS、Snapshot/WAL 或产品数据的长期存储。
- 不无限扩容、不隐藏跨线程所有权转移，也不释放调用方仍持有的 Buffer。
- 不把第三方 allocator 类型暴露到 ABI。

## 输入、输出与所有权

释放方按 allocator provenance 判定——谁分配谁释放，NativeCore 永不释放调用方内存，调用方只能经 release API 归还 Native 内存。借用 Buffer 只在声明的调用范围内有效；返回的地址、长度和对齐必须满足 ABI 约束。异步 Job 只能接收 `NativeOwnedBufferHandle`，借用字节须在 submit 时复制——Buffer 三分类与租约协议见 [`ffi-buffer-ownership.md`](../../../../docs/specs/ffi-buffer-ownership.md)（ADR 0003）。释放操作重复调用必须返回可诊断结果，不得静默忽略错误。

## 依赖与约束

依赖 `error` 的容量失败类别；Buffer 布局规则是本模块自己的定义，不再来自任何生成契约。`job`、`spatial`、`codec` 可以消费本模块的分配能力；内存模块不依赖它们，避免资源层循环。

## 线程、错误与观测

每种分配器必须声明线程安全、回收时机和最大预算。OOM、对齐错误、Buffer 不足、重复释放和越界必须在边界处拒绝。统计通过结构化记录交给 `diagnostics` 或 Host，不在分配路径等待 Sink。

## 测试与性能

- 对齐、长度、借用范围、释放和双重释放。
- 多线程分配/释放、池耗尽、碎片化、长时间泄漏和峰值 RSS。
- 记录分配次数、字节数、p95/p99 延迟和不同批次大小下的吞吐。

## 版本演进

改变释放方、对齐、Buffer 生命周期或最大分配语义属于 ABI 兼容变更。内部池布局和回收算法可以替换，但必须保持相同的所有权与失败结果。

## 相关

- [FFI Buffer 所有权契约](../../../../docs/specs/ffi-buffer-ownership.md)
- [仓库架构契约](../../../../.spec/knowledge/standards/repository-architecture.md)
- [错误模块](../error/README.md)
- [Job 模块](../../../lumio-job/README.md)
- [根 README](../../../../README.md)
