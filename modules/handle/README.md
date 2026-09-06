# handle

> 提供带 `Index + Generation + Context` 校验的 Native 不透明 Handle 生命周期。

**RepositoryDeliveryPhase**：Foundation  
**ImplementationPriority**：I0  

Handle 只表示 NativeCore 内部资源，不表示 ECS Entity、NetEntity、World 或 Session 身份。

## 负责范围

- 分配、解析和释放不透明 Handle。
- 校验 Index、Generation、Context 和释放状态。
- 拒绝失效 Handle、跨 Context 使用和重复释放。
- 为异步 Job 或其他 Native 资源提供可检测的生命周期标识。

## 不负责范围

- 不创建或保存托管对象、Delegate、World 引用或业务实体。
- 不定义 Session 内领域身份是否复用。
- 不决定上层资源的销毁顺序、重试或故障处置。

## 输入、输出与所有权

创建者获得 Handle 并负责发起释放；模块负责在释放后使其失效。解析只返回当前 Context 下的有效内部资源视图，视图不得跨 FFI 调用或跨线程长期保存。错误结果使用版本化 ABI 载荷返回，不暴露内部地址。

## 依赖与约束

依赖 `error` 的稳定失败类别；不得依赖 Voxel、Runtime、Server、Client 或 Game。Context 的创建、排空与关闭时序由 `kernel-context` 统一裁决（ADR 0002）；Generation 溢出槽位永久退休、ContextId 单调不复用，规则见 [`kernel-context-lifecycle.md`](../../docs/specs/kernel-context-lifecycle.md) §4，并有版本化测试约束。

## 线程、错误与观测

并发解析、释放和回收必须有清晰的线性化规则；调用方不得在资源内部锁上调用托管代码。失效、重复释放、Context 不匹配和资源类型不匹配分别可诊断。模块只输出事件字段，不拥有外部日志保留。

## 测试与性能

- 创建、解析、释放、重复释放和失效访问。
- Generation 回收、Context 隔离、并发读写和销毁竞态。
- 长时间分配/释放压力下的泄漏、内存增长和句柄解析 p95/p99。

## 版本演进

Handle 的不透明性和失效语义属于 ABI 契约；改变宽度、Context 语义或释放结果必须经过架构源变更流程。内部槽位布局可以演进，但不得成为公共契约。

## 相关

- [错误模块](../error/README.md)
- [kernel-context 模块](../kernel-context/README.md)
- [根 README](../../README.md)
