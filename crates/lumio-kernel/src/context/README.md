# kernel-context

> NativeCore 的生命周期根：一个 Context 代表一个资源域，统一拥有域内跨调用资源，
> 唯一裁决关闭时序（ADR 0002，评审 ARCH-P0-002 的修复）。

**RepositoryDeliveryPhase**：Foundation  
**ImplementationPriority**：I0  

完整契约（状态机、资源拥有表、七步关闭顺序、竞态赢家表、Conformance Fixture）见
[`kernel-context-lifecycle.md`](../../../../docs/specs/kernel-context-lifecycle.md)；本文只述边界。

## 负责范围

- 创建、运行、排空与关闭 Context：`Creating -> Running -> Quiescing -> Closed`（活动态可入 `Faulted`）。
- 统一拥有 Handle Arena namespace、内存预算/池、Worker 集与 Completion Queue、索引/工作区 registry、可选诊断 recorder。
- 定义 `ContextResource` port：`job`/`spatial`/`codec` 的跨调用资源实现该 port 并注册进 Context，由 Context 驱动取消、排空与销毁。
- 裁决关闭与并发操作的线性化（close vs submit/resolve/complete/close）。
- 维护 ContextId 单调不复用与 Epoch 退休规则。

## 不负责范围

- 不理解 World、Session、Tick 或任何领域语义；World/Session 如何映射到 Context 属跨仓 handoff 契约（待上游 OPEN-005）。
- 不拥有 Wall Clock/TickId；排空 deadline 使用私有单调时钟 port。
- 不替代各模块的资源内部实现，只经 port 持有与驱动。

## 输入、输出与所有权

Context 由调用方显式创建与关闭；`context_close` 幂等，首个调用者驱动固定七步关闭序列。
每个跨调用资源在所有权矩阵中只有一个 owner；调用方持有的只是 opaque handle。
关闭后计数必须归零或进入文档化的 retained-evidence 状态。

## 依赖与约束

依赖 `capability`、`handle`、`memory`（编译期）；`job`/`spatial`/`codec` 反向注册（编译期方向指向本模块），
避免 crate 层循环。禁止方向由 `cargo xtask check-dep-dag` 强制。

## 线程、错误与观测

状态迁移由本模块单点线性化；任何模块不得自行判断 Context 存活。`ContextNotReady`、
`ContextClosing`、`ContextClosed`、`ContextFaulted` 是可区分的稳定错误。关闭进度、
排空计数与 Abandon/reaper 状态以批量诊断记录输出。

## 测试与性能

- 关闭竞态三组（close vs resolve/submit/complete）固定 interleaving + 随机压力双跑。
- 重复 close 幂等、close_deadline 触发 Abandon、reaper 超限入 Faulted、半初始化失败无泄漏。
- ContextId 不复用、Generation 溢出槽位退休、容量耗尽错误可区分。
- 测量创建/关闭延迟、排空耗时分布与 reaper 队列水位。

## 版本演进

ContextId 的跨 ABI 公开表示、关闭错误码进入版本化契约前须经架构源冻结；
状态机与关闭顺序的任何变化按新 ADR 处理，不隐式演进。

## 相关

- [KernelContext 生命周期契约](../../../../docs/specs/kernel-context-lifecycle.md)
- [决策 ADR 0002](../../../../.spec/decisions/0002-kernel-context-lifecycle-root.md)
- [Handle 模块](../handle/README.md)
- [根 README](../../../../README.md)
