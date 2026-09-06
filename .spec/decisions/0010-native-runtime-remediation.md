# 0010 · 用真实运行闭环与有界生命周期替代占位交付

- 日期:2026-09-06
- 状态:生效

## 背景

源码审查发现 Job 未执行 kernel、Context Pending 仍进入销毁、旧 Timer 记录可误退休复用槽位、局部有界不等于生命周期有界，以及文档/测试名称高于实际保证。

## 决策

保留纯 Rust 内核与 SDK 唯一 ABI 边界。补充 0002/0004 的当前 Rust 实现裁决：Context 统一准入并提供可轮询 close，Pending 不销毁；运行 Job 取消是请求而非终态；结果只在真实终态后回收。当前不引入后台 Abandon/reaper，超时由宿主继续驱动或升级故障，不能伪报 Closed。

JobSystem 采用同一调度 mutex 发布状态，kernel 在锁外执行；独立 CAS 类型只是原语，不是第二份运行状态。Weak Context 打破 owner/resource 环，调用方必须保留根。输出租约可以随 JobResult 超出 Context 生命周期。

Timer 对候选数量预检并原子拒绝超额窗口，所有内部退休验证完整 Handle；业务策略和测试钩子默认不可见。rstar 使用真实已锁定的依赖，参考实现保持独立。原型 feature 默认关闭，但 all-features CI 仍运行其测试。

仓库门禁消费 Cargo metadata，而不是用白名单枚举代替实际成员或手工拆 TOML。规则与行为测试都不以改名、固定成功和跳过验证充当修复。

## 后果

开发期公开 Rust API 与旧 fixture 有迁移成本，明确记入修复记录。本次不修改其他仓的 ABI；SDK/Host 和生产负载须再次联测。按系统的任务配额不等于全进程配额，Kernel 内部临时分配不在输入/输出账本内，非合作任务不能被安全强制终止。未验证项不计为交付完成。
