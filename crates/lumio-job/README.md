# job

> 提供有界 Worker、Typed Job、取消、超时和 Completion Batch 的 Native 调度原语。

**RepositoryDeliveryPhase**：Foundation  
**ImplementationPriority**：I0  

Job 结果由上层在规定 Barrier 消费；Native Worker 不回调托管 Hot Gameplay。

## 负责范围

- 管理有界队列、Worker 生命周期和 Typed Job 状态。
- 支持提交、取消、Deadline/Timeout、完成批次和结果查询。
- 明确队列满载、取消竞态、超时和 Worker 故障的结果。
- 为 `spatial`、`codec` 等纯 Kernel 提供可选调度承载。

## 不负责范围

- 不定义 Tick Phase、Processor 依赖、Session 调度或业务重试。
- 不调用 C#、保存托管 Delegate、跨 FFI 持有 Rust 锁或写入 World。
- 不把无界线程、无界队列或隐式后台任务作为默认策略。

## 输入、输出与所有权

提交方移交 `NativeOwnedBufferHandle`（或由 submit 复制借用字节）建立输入租约，并通过不透明 Job Handle 查询结果；租约持续到真实终态 reap，取消/超时不提前释放（ADR 0003）。Completion Batch 在声明的消费边界前保持有效；取消或超时后不得自动写入已销毁的 World。队列满载应返回稳定错误和可观测的容量信息。

## 依赖与约束

依赖 `handle`、`error`、`memory` 与私有单调时钟 port（跨 ABI 只收相对 duration，不拥有 Wall Clock/TickId）；Worker 集作为 ContextResource 注册进 `kernel-context`（ADR 0002）。Job 不强制依赖具体 Kernel；调用方可在 `NativeJobBarrier` 或之后应用结果。线程亲和、重入性、最大并发和资源预算必须写进契约。

## 线程、错误与观测

Worker 只执行 Rust 内部闭包或架构源注册的 Typed Kernel；公共 ABI 不接受调用方函数指针、managed delegate 或任何回调。状态转换在 Job 槽位上单点 CAS 线性化，状态集、竞态赢家表与超时观察语义见 [`job-state-machine.md`](../../docs/specs/job-state-machine.md)（ADR 0004）；重复取消、超时后完成、结果丢失和 Worker 关闭都要可区分。队列指标、耗时和取消原因以批量 Diagnostic Event 输出。

## 测试与性能

- 提交、完成、取消、超时、队列满载和关闭期间提交。
- 取消与完成竞态、Worker 故障、结果批次顺序和资源释放。
- 固定 Worker/队列配置测量吞吐、等待 p95/p99、峰值内存和取消延迟。

## 版本演进

改变 Job 状态机、结果可见时机、取消/超时语义或线程亲和性必须经过 ABI/契约变更流程。Worker 实现、队列算法和线程数可优化，但不能破坏有界和 Barrier 约束。

## 相关

- [Job 状态机契约](../../docs/specs/job-state-machine.md)
- [仓库架构契约](../../.spec/knowledge/standards/repository-architecture.md)
- [Handle 模块](../lumio-kernel/src/handle/README.md)
- [Memory 模块](../lumio-kernel/src/memory/README.md)
- [kernel-context 模块](../lumio-kernel/src/context/README.md)
- [根 README](../../README.md)
