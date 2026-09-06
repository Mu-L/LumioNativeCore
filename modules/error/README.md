# error

> 统一 NativeCore 的稳定错误类别、错误码承载和诊断载荷边界。

**RepositoryDeliveryPhase**：Architecture Gate / Foundation  
**ImplementationPriority**：I0  

`ErrorCategory` 是本仓内部枚举；跨边界状态码由架构仓 `engine/abi/native-abi.json` 与其插头决定，本文不复制枚举或数值。

## 负责范围

- 将失败归类为可重试、可拒绝、可致命或其他架构源定义的稳定类别。
- 承载错误码、参数索引、所需长度、关联 Handle 和 TraceId 等诊断信息。
- 约束错误载荷的长度、编码、敏感字段和跨 ABI 表示。
- 为 Rust panic、边界校验失败和资源限制提供统一转换出口。

## 不负责范围

- 不决定调用方如何重试、断开 Session、回滚 World 或进入维护。
- 不替代 Audit Log、Txn Journal、Command Log 或 Failure Bundle 存储。
- 不把第三方异常类型暴露给托管调用方。

## 输入、输出与所有权

错误由产生失败的调用创建，并写入调用方提供的 Buffer 或返回结构；调用方负责读取和释放可拥有载荷。文本只作为诊断附加信息，不能成为机器判定的唯一依据。错误载荷截断、未知版本和非法编码必须明确拒绝。

## 依赖与约束

零外部依赖：`ErrorCategory` 与 `ErrorDetail` 都是本仓内部类型，不承载任何跨边界数值（ADR 0009）。跨边界状态码由架构仓插头对 `engine/abi/native-abi.json` 决定；本模块不定义、不镜像、不申请状态码。

## 线程、错误与观测

错误构造应为无阻塞、可重入操作；异步 Job 必须把取消、超时、队列满载和结果丢失区分开。诊断事件可引用错误信息，但 `error` 不依赖外部 Sink，也不承担审计或全局脱敏策略。

## 测试与性能

- 每个架构源错误类别的正向构造、解码和未知码行为。
- Buffer 不足、截断、非法 UTF-8、重复字段和超长载荷。
- panic/异常转换、并发错误构造和高频失败下的分配与延迟。

## 版本演进

新增或改变公共错误码、类别、字段和失败时序必须在架构源记录 ADR、Fixture 并发布新 Baseline。内部错误可以保留，但不得泄漏到稳定 ABI。

## 相关

- [仓库边界与架构契约](../../.spec/knowledge/standards/repository-architecture.md)
- [根 README](../../README.md)
