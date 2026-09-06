# diagnostics

实验性的有界记录器与资源生命周期实现，只有 `prototype` feature 才导出。默认内核不依赖本 crate，不把 Trace/Bundle/Sink 等尚未完成集成写成生产能力。

本 crate 尚未接入统一 Kernel RecordPort；当前能够验证的是本地记录队列、容量、丢弃计数、记录复制和 ContextResource 关闭，不是完整引擎故障证据链。

原型测试在 all-features CI 中运行。SDK、Server、日志后端和 Failure Bundle 的装配由真实消费者独立验收。
