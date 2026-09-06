# NativeCore 模块与编译边界

本仓是纯 Rust 源码集，不导出 C、不保存 ABI schema。跨语言唯一真值在 LumioGameEngine SDK。本仓的稳定源码边界是公开 Rust API；开发期可以联合消费者破坏式演进，不承诺 Rust 二进制 ABI。

## 依赖方向

- platform：零内部依赖，单调钟端口。
- kernel → platform：错误、Capability、Handle、内存预算、Context。
- job → kernel + platform：可执行作业与生命周期接入。
- timer：零内部依赖，输入刻度驱动，不读取日历时间。
- spatial → kernel，默认 rstar 0.12.2 仅在 Adapter 内使用。
- codec → kernel：默认字节辅助，prototype 才暴露解码接缝/工作区。
- diagnostics → kernel + platform：prototype 才暴露有界记录器。
- test-support → kernel + job + platform：仅 dev 消费。
- xtask：标准库启动 Python Cargo metadata 检查器。

生产方向与直接供应商白名单以 `tools/check_repository.py` 为机器真值；它枚举实际 workspace，含可选、平台和 build 依赖。Cargo.lock 锁传递版本。dev-only 依赖不计生产方向，但仍编译/测试。

## 所有权

KernelContext Arc 由装配方持有，registry 保存 ContextResource。JobSystem 反向使用 Weak，不制造引用环；创建/提交必须完成准入事务。空间资源 quiesce 与 destroy 对真实索引起作用。独立 TimerManager 由 SDK/调用方拥有，其生命周期不暗中绑定 Context。

共享字节预算覆盖明确预留的 Job 输入和输出，单实例任务容量覆盖排队、执行和未回收结果；不能解释为整个进程任意供应商内存均受账本约束。JobResult 转移后，所有者是调用方，字节租约同步转移。

## 交付口径

当前 Spatial 只包括 AABB 能力，不把完整 BVH/距离/邻域规划视为已经完成。Codec/Diagnostics 的 feature 隔离是真实代码隔离，不是 README 标签。Timer 的 SDK 消费路径存在，不证明此次破坏式更新已经跨仓通过。

当前实现与测试入口见根 README、job-state-machine、kernel-context-lifecycle、spatial-backend 和 docs/reviews/2026-09-06-native-remediation.md。测试、真实消费、负载结果分开记录。

## 文档模块 ↔ crate 映射

各模块的设计与边界文档已内聚到对应的 `crates/` 目录内，不再保留顶层 `modules/` 目录；编译边界按下表，后续调整须新 ADR：

| crate | 承载文档模块与路径 | 类型 |
| --- | --- | --- |
| `lumio-platform` | —（monotonic clock port 等） | rlib，private |
| `lumio-kernel` | 根 [`crates/lumio-kernel/README.md`](../../crates/lumio-kernel/README.md)（子原语：[`error`](../../crates/lumio-kernel/src/error/README.md)、[`capability`](../../crates/lumio-kernel/src/capability/README.md)、[`handle`](../../crates/lumio-kernel/src/handle/README.md)、[`memory`](../../crates/lumio-kernel/src/memory/README.md)、[`context`](../../crates/lumio-kernel/src/context/README.md)） | rlib |
| `lumio-job` | [`crates/lumio-job/README.md`](../../crates/lumio-job/README.md) | rlib |
| `lumio-spatial` | [`crates/lumio-spatial/README.md`](../../crates/lumio-spatial/README.md) | rlib |
| `lumio-timer` | [`crates/lumio-timer/README.md`](../../crates/lumio-timer/README.md) | rlib |
| `lumio-hfsm` | [`crates/lumio-hfsm/README.md`](../../crates/lumio-hfsm/README.md) | rlib |
| `lumio-codec` | [`crates/lumio-codec/README.md`](../../crates/lumio-codec/README.md) | rlib，**experimental/private** |
| `lumio-diagnostics` | [`crates/lumio-diagnostics/README.md`](../../crates/lumio-diagnostics/README.md) | rlib，**experimental/private** |
| `lumio-test-support` | — | dev-only |
