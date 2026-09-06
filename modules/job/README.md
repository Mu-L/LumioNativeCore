# job

有界 Rust Kernel 执行、协作取消、结果消费与回收。无 managed callback，不定义游戏 barrier、Tick phase 或业务重试。

实现位于 crates/lumio-job。TypedKernel 不再只有 ID；必须实现 execute 才能成功计算。输入复制、输出容量预留、未回收任务背压、System/Context 身份隔离都在真实 JobSystem 路径中执行。Context 是生命周期 owner，必须由装配方保留。

现行规则见 [Job 状态机](../../docs/specs/job-state-machine.md) 与 [Context 生命周期](../../docs/specs/kernel-context-lifecycle.md)。实现和定向回归不等于 SDK/Host 联测或性能验收；后两者须单独执行。
