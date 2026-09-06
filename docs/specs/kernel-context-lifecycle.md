# KernelContext 生命周期（当前 Rust 实现）

生命周期根、准入与注册属于 KernelContext。跨语言表示由 SDK 决定，本文件不登记 ABI 编号。实现决策见 ADR 0010。

## 创建与所有权

`create` 验证配置，保存限制及共享字节预算；`create_with_clock` 注入 Deadline 所用单调钟。Context ID checked increment，溢出报错不复用。调用方必须保留 Arc；JobSystem 使用 Weak 反向引用，ContextResource 登记保存强引用，无引用环。

## 准入

新增资源和 Job 发布在 `admit_work` 的同一把锁下完成。close 持该锁进入 Quiescing 并冻结资源快照。登记先赢则进入关闭集合；关闭先赢则登记失败。`ensure_accepting_work` 仅是状态快照，不能替代完整操作的准入 guard。

resource.name 与资源回调不在准入/registry 锁内执行。admission guard 不得跨 ABI、不得包裹用户回调或阻塞等待。

## 可续推进关闭

`close(reason, deadline)` 每次执行一轮，不隐藏后台 reaper。首次关闭停止准入，发送取消请求并冻结资源集；每个资源只有报告 Quiesced 才可以进入销毁步骤。任何 Pending 都返回 Quiescing 或 TimedOut，绝不继续 destroy。调用方可用新的 deadline 再驱动。

资源全部静默后，逆登记顺序 destroy。成功销毁的条目不再重做；含糊的销毁失败/回调 panic 会保留错误，不重复副作用，不谎报 Closed。只有全部成功才释放 registry 并缓存 Closed 报告。并发或重入 close 返回 ContextClosing，不在回调中死锁等待同一 close 锁。

Deadline 来自创建时注入的时钟域；NONE 可使用配置默认 deadline。资源 quiesce 本身必须有限返回；本实现无法抢占不合作的外部 trait 实现。

## 资源回收

JobSystem 在 quiesce 时取消排队任务并请求运行中任务合作取消；只有真实执行结束和 worker 退出才报告静默。destroy 回收未消费结果并 join 已退出的线程。已 take_result 的结果归调用方，字节租约随结果销毁释放。

SpatialResource 关闭先停止新操作，以读写锁观察现有查询结束，destroy 真正取走并释放索引。

Drop 只尽力停止，不能当成完成屏障。需要关闭完成保证的调用方必须显式驱动 close。当前没有线程强杀、无界等待、后台 Abandon 队列或自动恢复承诺。

## 回归

`crates/lumio-kernel/tests/audit_close.rs` 覆盖 Pending、超时重试和晚登记；`crates/lumio-job/tests/audit_execution.rs` 覆盖真实运行中取消、输入租约与 close；既有关闭顺序与重复释放用例仍保留。
