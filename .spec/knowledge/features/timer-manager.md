---
name: timer-manager
description: Native 单一定时内核（wallClock + tickFrame）与 Server/Client adapter——查调度、CallbackSlot、ABI 槽或切片消费者时读
metadata:
  type: doc
  status: 已交付
---

# Native 单一定时内核（wallClock + tickFrame）

确定性 gameplay 调度与单调墙钟到期共用一个内核：tickFrame 走 `advance`，wallClock 走 `pump(now_ms)`。经 `native-abi.json` 的 `timer_*` 槽到达托管侧。

## 背景 / 目标

ADR-056 §7：定时内核只有一个，在 NativeCore。C-4′ 把托管可达面纳入 ABI；`slotDispatchId` 为 u32；destroy 后句柄是 shutdown-tombstone（status 17）。契约真值是架构源 `lumio.native-timer-abi.v1` + `engine/abi/native-abi.json`。

## 设计

- **设计面**：单内核双模式。`TimerMode::TickFrame` 拥有 Tick/Frame；`TimerMode::WallClock` 拥有单调毫秒 deadline。两模式共用 TimerHandle / CallbackSlot / 错误码；manager 实例之间 handle 空间不互指。
- **交互面**：进程内 `scheduleOneShot` / `scheduleRepeating` / `cancel` / `advance` / `pump`；C ABI 另有 create/destroy/scope/slot/drain。CallbackSlot 生命周期 `unbound → armed → delivering → closed`。禁止函数指针。
- **实现面**：内核 `lumio-timer` 在 NativeCore；C ABI 插头在架构仓 `engine/native/modules/sdk-native/src/timer.rs`；经 `engine/abi/native-abi.json` 的 `timer_*` 槽到达托管侧。Bot 节奏 N=5 ticks（`BOT_CHAT_CADENCE_TICKS`）。Server 周期任务每 10 ticks。五分钟重连走 wallClock one-shot（`RECONNECT_RETENTION_MS`）。`slot_queue_full` 稳定拒绝并使该定时器终态，进程继续。

## 待解决

- 无。

## 相关

- [ADR 0008](../../decisions/0008-timer-kernel-enters-native-abi.md)（取代 [ADR 0007](../../decisions/0007-timer-manager-in-process-api.md)）
- [`crates/lumio-timer/README.md`](../../../crates/lumio-timer/README.md)
- 架构源 ADR-056 §7 / C-4′
