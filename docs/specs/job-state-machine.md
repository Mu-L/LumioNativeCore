# Job 执行、取消与结果回收（当前 Rust 实现）

## 唯一运行状态

JobSystem 由调度 mutex 串行发布 Queued、Running 和真实终态 Succeeded/Failed/Cancelled。Kernel 在锁外执行。独立 JobStateMachine 仍作为 CAS 原语提供，但不是第二份 JobSystem 权威状态。

TimedOut 保留为旧观察词汇，不是执行状态；JobResult.deadline_exceeded 记录截止时间观察。已完成工作保留真实结果，不因为外部稍晚观察就伪造超时。

## 执行与容量

TypedKernel.execute 接收输入切片、有界输出切片和 JobExecution；仅注册 ID 没有实现 execute 时返回 CapabilityUnavailable。submit_input 在发布前复制输入、预留输出和字节租约。第三方 Kernel 内部自行分配的工作内存不在这份字节账本内，供应方必须额外声明工作预算。

worker_count=0 为显式手动 pump；正数为有限 worker。queue_capacity 限制等待队列；min(max_handles,max_completion_items) 限制每个 JobSystem 的未回收任务。各系统配置不得超过 Context 上限；Context 的共享字节预算跨系统汇总。元数据/工作线程数量是按系统限制，并非整个进程统一配额。

完成但未 take_result 的任务仍占容量。没有消费者时产生背压，不无限追加历史。JobHandle 验证 Context 和 System，JobId 在进程内 checked increment，不复用。

## 取消与 Deadline

Queued 取消从队列移除并进入真实 Cancelled；Running 取消仅返回 Requested 并设置 token。Worker 在有限间隔调用 check_cancelled 才转终态；不能在请求取消时释放仍在执行的输入。

Kernel 成功返回且输出长度合法则按实际成功记录；取消请求不覆盖已经完成的实际计算。Kernel 返回 Cancelled/TimedOut 表示取消被观察。panic 被转换为失败结果；仅适用于 unwind 配置，abort 无法被捕获。

## Completion 与回收

`drain_completions` 按本次已就绪快照中的 JobId 排序，只返回通知，不释放任务。它不承诺跨多次异步 drain 的全局顺序；确定性消费者须定义等待的完整集合和消费 barrier。

`take_result(handle)` 仅接受终态，移除调度元数据，使 Handle 失效并返回 JobResult。结果字节和 reservation 同寿命；drop result 才返还预算。可直接 take_result，不必先 drain。

独立 CompletionBatch 不是 JobSystem 的第二份权威存储。它要求单调递增 ID 发布，活租约受容量限制，release 需先 drain，最近释放诊断只保留有界窗口；更老的释放返回 InvalidHandle。高水位阻止旧 ID 再发布，不永久保存墓碑。

## 关闭与消费迁移

JobSystem 注册为 ContextResource，反向仅持 Weak Context。调用方保留 Context 并明确驱动 close。停止后不再出队，排队任务取消；运行任务保持租约直到真实结束；quiesce 不 join 活线程，destroy 只 join 已退出线程。旧仅持 JobSystem 的装配必须同步修正。

回归：audit_execution、worker_never_executes_under_scheduler_lock、job_state_machine 及原有 CAS/Completion 用例。
