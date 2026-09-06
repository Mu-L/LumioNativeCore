# timer

一个 Rust 定时内核，支持输入的单调毫秒和逻辑刻度。C ABI 插头仍由 LumioGameEngine SDK 拥有，本仓只提供源码。

TimerHandle 的 Context/Index/Generation 在所有回收路径验证，包括迟到错误。advance 先算候选 firing 数并检查工作预算，再分配/排序；超额整体拒绝且不推进 committed_tick，调用方可拆小窗口重试，不静默丢确定性事件。

TimerBudget 同时限制 manager 范围的 scope、slot、timer、排队记录、推进候选和诊断；无效排队项在实际 drain 前仍计入物理容量。live timer 使用计数器，不逐 firing 全量扫描。

默认生产构建不暴露强改 Generation 的测试方法，不定义 Bot/Server 周期。兼容夹具在 test-support feature 内，all-features CI 仍运行。scope/slot 墓碑有上限；达到限制需显式销毁 manager，不承诺无限创建历史而永不耗尽。

跨仓 SDK 必须重编并验证；本次 Rust 回归不自动证明上层 ABI 绑定兼容。
