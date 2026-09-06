# Decisions(决策记录 · ADR)

本目录是全仓内部决策记录的唯一落点；跨仓公共语义只在 LumioGameEngine 维护。

## 怎么写一条 ADR

一个决策一个 `NNNN-<slug>.md`，递增编号，无 frontmatter。包含日期、状态、背景、决策和后果。旧正文不改写；取代时只更新状态和索引，历史保留。功能文档描述当前实现，不承担第二份决策记录。

## 索引

| 编号 | 决策 | 状态 |
| --- | --- | --- |
| [0001](0001-contract-layering-and-symbol-surface.md) | contract-types 与 ffi 门面 | 被 0009 取代 |
| [0002](0002-kernel-context-lifecycle-root.md) | kernel-context 生命周期根 | 生效；当前 Rust 关闭实现由 0010 补充 |
| [0003](0003-ffi-buffer-classes-and-leases.md) | Buffer 三分类与异步租约 | 生效 |
| [0004](0004-job-state-machine-and-clock-port.md) | Job 状态机与单调时钟 | 生效；当前调度/取消实现由 0010 补充 |
| [0005](0005-codec-diagnostics-pending-and-dual-status.md) | codec/diagnostics 原型与双状态 | 生效 |
| [0006](0006-capability-keys-have-no-raw-constructor.md) | 生成 Capability 键 | 被 0009 取代 |
| [0007](0007-timer-manager-in-process-api.md) | Timer 进程内 API | 被 0008 取代 |
| [0008](0008-timer-kernel-enters-native-abi.md) | 唯一定时内核经 SDK 导出 | 生效 |
| [0009](0009-exit-legacy-contract-regime.md) | 退出合同镜像制度 | 生效 |
| [0010](0010-native-runtime-remediation.md) | 真实执行、有界生命周期、原型隔离和验证 | 生效 |
