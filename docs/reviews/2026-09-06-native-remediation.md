# NativeCore 源码审查修复与迁移

基准：`7b3ef0df8f4ae894bf8494156adb6e6c4fa86103`。PR #9，范围为本次 NativeCore 审查，不修改其他仓或 ABI schema。用户批准开发期破坏式修改、提交 PR 并合并，后续集成由用户收尾。

## 修复对应

| 审查问题 | 实际变更 |
| --- | --- |
| Job 只查注册表便成功 | Rust execute、输入/输出、真实 worker、结果和回收 |
| Pending 仍 destroy | close 逐轮推进，Pending/超时保留资源 |
| register/close 竞态 | 同一个 admission 事务发布和冻结 |
| Timer 旧记录误回收新代 | 所有内部 retire 校验完整身份 |
| Job 跨实例 Handle 别名 | Context/System 身份与不复用 ID |
| 历史 Job/Completion 无界 | 终态仍占配额，take_result 移除；有界释放窗口 |
| Timer 补发分配失控 | 预计算预算、原子拒绝大窗口、总容量与诊断上限 |
| Running 取消伪终态/丢失 | 请求 token 与真实执行终态分离；CAS 原语重试 |
| 假 rstar 与未接后端 | 真实 rstar 0.12.2、可注入后端、独立参考实现 |
| 非法 AABB 绕过构造器 | 插入/查询真实边界验证；容量先检 |
| 业务策略/测试钩子入内核 | 默认 Timer 移出策略；显式 test-support |
| 假 feature 隔离和旧文档 | prototype feature、all-features 测试、现行规范同步 |
| 脆弱 DAG/TOML 检查 | Cargo metadata 真实成员/声明/目标，Python 反例测试 |
| 测试只证明形状 | 新执行/关闭/代次/背压反例；锁测试改实际重入 |

## 必须同步的调用方

1. 保留 KernelContext Arc；JobSystem 持 Weak，不再用引用环延长根生命周期。
2. 实现 TypedKernel.execute；仅注册 metadata 返回 CapabilityUnavailable。worker_count=0 明确表示手动泵。
3. submit_input 声明最大输出容量，真实终态后 take_result。不要把 Running 的 Requested 当作 Cancelled，不要提前释放输入。
4. close 返回 Quiescing 必须继续驱动；TimedOut 不代表资源已销毁。当前不提供强杀线程或后台 Abandon reaper。
5. 大幅推进 Timer 可能返回 ScheduleBudgetExceeded，committed_tick 不变；拆成较小窗口并消费，不能直接忽略错误。
6. 默认构建不含测试策略常量、强改 Generation 方法或 Codec/Diagnostics 原型导出。原型消费者必须显式 feature；生产策略迁到消费仓。
7. 独立 CompletionBatch 采用递增发布 ID、先 drain 后 release；保留诊断的窗口有界。JobSystem 自有的完成通知不是该独立容器。

## 资源与保证的准确范围

Job 输入/预留输出使用共享字节账本；外部 kernel 的内部临时分配需自行声明预算。任务、worker、队列限制按 JobSystem 实例执行，不伪称全进程自动汇总。结果转移给调用方后租约仍有效；允许其比 Context 活得久。调度不抢占不合作的 kernel。

Timer 元数据有硬上限，scope/slot 历史不无限复用；达到限制报错，不暗中回卷。Spatial 当前只是 AABB 能力，不把其他算法规划记为交付。Codec 解码/Diagnostics 集成仍是原型，不以名称宣称可用。

## 验证记录与限制

第一轮 GitHub Actions 已证明生产 workspace all-features 可以编译、Cargo metadata 与 Python 规则反例可运行；同时发现旧 Job fixture 生命周期假设、Timer 旧测试辅助 API 与 Clippy 问题。本 PR 随后补充修复和回归，最终 CI 以 PR check runs 为准，不能把前一轮成功复用成最终提交的通过证明。

本地环境没有 Rust 工具链，未执行本地 Cargo；未执行 SDK/Host 跨仓联测、Miri、真实目标设备、性能/长跑。因此本 PR 交付代码、文档与测试，不声称全引擎、所有平台或商业负载已验收。已失败或未运行项目必须在最终 PR 描述如实列明，不通过删除失败断言或更改保护策略伪装绿色。
