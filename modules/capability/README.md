# capability

> 声明并校验平台、编译 Feature、Native 后端和资源能力，不表达业务模式。

**RepositoryDeliveryPhase**：Architecture Gate / Foundation  
**ImplementationPriority**：I0  

Capability 键是本仓内部的不透明数值，由嵌入方（架构仓 SDK）自行定义键空间；本文只说明本地读取和报告边界。

## 负责范围

- 汇总能力声明并分三层表达：`StaticCapabilities`（平台/编译 Feature/后端，进程内不变）、`ConfiguredLimits`（配置的资源上限，Context 创建时固定）、`RuntimeStatus`（动态资源余量，只读查询、不缓存进快照）。
- 提供请求能力与已提供能力之间的确定性匹配结果。
- 把缺失能力、版本不匹配和资源预算不足转换为稳定错误。
- 为 Loader、Host 和测试工具提供只读能力快照。

## 不负责范围

- 不定义 `IsLocal`、`IsOffline`、RoomMode、Role 或业务权限。
- 不替代签名、Hash、ReleaseCatalog 或 Host 的最终准入策略。
- 不在运行时静默启用未声明的 Feature。

## 输入、输出与所有权

能力快照只含 `StaticCapabilities` 与 `ConfiguredLimits`，由加载环境和构建产物生成，调用方以版本化只读结构读取；`RuntimeStatus` 走独立查询接口，不进入快照与兼容匹配。能力位集合不能由业务代码任意修改。快照不持有平台对象或托管引用。

## 依赖与约束

零外部依赖：键是不透明 `u32`，键空间与名称由嵌入方定义，本模块只做集合成员判定，不保存键名表。模块不得直接依赖网络、Voxel、Gameplay 或 Host 实现。

## 线程、错误与观测

查询和匹配应无阻塞、可重入且结果稳定。缺失必需能力、未知能力位、快照版本不兼容和资源预算不足必须可区分。报告可产生 Diagnostic Event，但不负责日志写入。

## 测试与性能

- 空集合、完全匹配、缺失能力、未知位和版本不兼容。
- 同一输入快照的确定性序列化、比较和并发读取。
- 大量能力查询下的分配、延迟和 Loader 启动开销。

## 版本演进

公共能力位和兼容语义只能通过架构源新增 Baseline；本地实现可以优化查询结构，但不得重编号、复用或隐式改变已发布能力位。

## 相关

- [错误模块](../error/README.md)
- [根 README](../../README.md)
