# timer

> 提供确定性 Tick/Frame Timer Manager（one-shot/repeating/cancel、CallbackSlot 投递）以及 Server/Client 适配器。

**RepositoryDeliveryPhase**：NativeHeadless  
**ImplementationPriority**：I1  

内核 `lumio-timer` 在 NativeCore；C ABI 插头在架构仓 `engine/native/modules/sdk-native/src/timer.rs`；经 `engine/abi/native-abi.json` 的 `timer_*` 槽到达托管侧。

## 负责范围

- 固定 Tick/Frame 的 one-shot 与 repeating 调度、取消、scope/generation 校验。
- 不透明 `TimerHandle`（index + generation + context）。
- 受控 CallbackSlot：预注册分发 id + 有界投递队列；Native 不调用函数指针或托管回调。
- Server Timer Manager 与 Client Timer Manager 适配器：scope 注册、slot 绑定、分发点排空。
- 切片消费者接线：Client 侧 Bot 发言节奏；Server 侧至少一个 Tick 域周期任务。

## 不负责范围

- 不拥有墙钟 deadline、五分钟断线保留窗口或进程生命周期绑定（宿主 Timer 服务 / R-00350）。
- 不引入 GameTime/RealTime/Scaled/Unscaled 时间域矩阵。
- 不接受任意函数指针、C# delegate 或热路径 Gameplay 回调。
- 不拥有 Chat、Bot、Session 或产品语义；消费者只通过预注册 DispatchId 可见。

## 输入、输出与所有权

调用方驱动 `advance(toTick)`。开火窗口是 `(committedTick, toTick]`。`advance` 返回确定性 `FiringRecord` 全序并入队；`drain` 在声明的分发点排空。单条 `slot_queue_full` / `slot_closed` / `late_completion` / `slot_dispatch_mismatch` 是逐条稳定拒绝，定时器终态，Manager 继续，进程不退出。

## 依赖与约束

依赖私有 `lumio-platform` 时钟类型，仅供并列的宿主墙钟门面使用；Tick Manager 本身不读墙钟。不编译期依赖 `job` 或 `diagnostics` 实现。generation 溢出是进程级 fail-stop，不是稳定错误码。

## 线程、错误与观测

调度与排空均 `&mut self`，单线程仿真线程使用。错误码词表以契约 `errorCodes` 为准。切片 trace 记录 Client Bot 节奏、Server 周期任务与宿主重连到期，三层不得混写。

## 测试与性能

- 契约 `testCases` / `invalidCases` 逐例确定性断言。
- Bot 节奏与 Server 周期任务出现在切片 trace；五分钟重连留在宿主单调时钟。
- `slot_queue_full` 稳定拒绝 + 定时器终态，不是 fail-stop。

## 版本演进

改变开火窗口、CallbackSlot 生命周期或错误码必须先改架构源契约。本仓只消费镜像，不另写语义真值。

## 相关

- 架构源 ADR-055 与 `engine/wire/native-timer-abi-v1.json`
- [Job 模块](../lumio-job/README.md)
- [根 README](../../README.md)
