# codec

> 提供领域无关的批量 Diff、压缩、Canonical Buffer 和校验热路径。

**RepositoryDeliveryPhase**：NativeHeadless  
**ImplementationPriority**：I1  

`codec` 处于 pending（ADR 0005）：仓内只做 feature-gated 私有原型，不进公共 Header/export list；转 approved 只能由架构源批准驱动。职责缩窄为纯字节 Kernel（压缩/校验/diff），不承诺任何跨仓公共 Schema。

## 负责范围

- 对调用方提供的版本化字节批次执行 Diff、合并、压缩、解压和校验。
- 支持 Canonical 顺序、长度、Hash/Checksum 和受限解码。
- 把高频机械编码工作保持在批处理边界，避免逐 Entity、逐 Voxel 或逐包 FFI。
- 为上层生成的领域 Serializer 提供可复用的底层 Kernel。

## 不负责范围

- 不定义 RPC、Gameplay Schema、Voxel Revision、Snapshot/WAL 或权限语义。
- 不拥有领域字段、实体图、Chunk 状态或发布版本迁移策略。
- 不执行输入中的代码，不接受无界解压或隐式分配。

## 输入、输出与所有权

调用方提供输入和输出 Buffer，创建侧负责释放拥有的结果。解码前必须检查 Magic、SchemaVersion（若由调用方携带）、Length、压缩比、Hash/Checksum 和最大分配；输出不足返回所需长度，不静默截断。

## 依赖与约束

依赖 `error` 和 `memory`；不编译期依赖 `job`——编码算子作为 operation 经 registry 运行时绑定，跨调用工作区作为 ContextResource 注册进 `kernel-context`（ADR 0002）。压缩库、Hash 库和 Diff 实现通过 Adapter 隔离，供应商类型不能出现在 ABI。

## 线程、错误与观测

纯编码操作应可重入；共享字典、工作区和缓存的线程规则必须明确。截断、解压炸弹、校验失败和版本不兼容分别返回可诊断错误；重复字段、未知必需字段等 schema 语义判定归上层生成 Serializer（ADR 0005），不在本模块。记录输入/输出字节、压缩比、耗时和分配，不把内容明文写入诊断。

## 测试与性能

- Round-trip、空批次、截断和损坏输入。
- 压缩比上限、最大消息/分配限制和 Hash/Checksum 失败。
- 固定数据集测量吞吐、p95/p99、压缩比、分配和峰值内存，并与 Reference 实现差分。

## 版本演进

底层算法可替换，但 Canonical 顺序、错误边界和资源限制必须稳定。新增格式或改变解码语义应由领域 Schema 所有者和架构源共同发布版本；不得在本模块私自冻结 Snapshot/WAL 格式。

## 相关

- [仓库架构契约](../../.spec/knowledge/standards/repository-architecture.md)
- [Memory 模块](../lumio-kernel/src/memory/README.md)
- [错误模块](../lumio-kernel/src/error/README.md)
- [根 README](../../README.md)
