# kernel-context

Native 跨调用资源的生命周期根。持有配置、共享字节预算、资源 registry、准入锁和 close 进度，不拥有 World/Session。

register 与 close 用同一准入事务冻结资源集合。Pending 资源不销毁，TimedOut 保留后续重试机会；close 返回 Quiescing 时由宿主继续驱动，不伪造七阶段完成。回调不持 registry 锁，成功销毁不重复；失败保留证据。

当前 API 与失败语义见 [生命周期规范](../../docs/specs/kernel-context-lifecycle.md)，测试见 crates/lumio-kernel/tests/audit_close.rs。Drop 不替代显式关闭完成屏障。
