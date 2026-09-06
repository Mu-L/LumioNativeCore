# hfsm

> 无状态层级有限状态机迁移计算器：数据定义的状态图 + 宿主持有的 Snapshot + 批量、无回调、确定性的迁移计划输出。

**RepositoryDeliveryPhase**：NativeHeadless  
**ImplementationPriority**：I1  

内核 `lumio-hfsm` 在 NativeCore；C ABI 插头与 `.hfsm.json` 工具在架构仓 `engine/native/modules/sdk-native`；经 `engine/abi/native-abi.json` 到达托管侧。语义契约：[`docs/specs/hfsm-semantics.md`](../../docs/specs/hfsm-semantics.md)；决策：ADR 0010。

## 负责范围

- `DefinitionSpec` 编译与校验（结构环、initial、priority 唯一、死转移、动作上限、深度/数量上限）；不可变 `CompiledDefinition` 共享。
- 单事件迁移计算：冒泡、priority、Guard 三态、Internal / Local / External 迁移域、Exit→Transition→Enter 记录、初始展开、激活代次。
- 批量 `evaluate_batch`：caller-owned 输出，容量不足整批不写；`DeliveryScope` 消费时拒绝迟到投递。
- `HfsmDefinitionRegistry` 作为 `ContextResource`，定义句柄复用 kernel Handle 世代。
- 规范化指纹（FNV-1a 64）与 `SEMANTICS_VERSION` / `FORMAT_VERSION`。

## 不负责范围

- 不持有实例 Snapshot、事件队列、Activity、定时器注册；无 MachineCreate/Destroy。
- 不执行 Guard 或 Action；不回调托管代码；不读时钟；不做 JSON 解析。
- 不支持 history / 并行区域 / eventless / defer / 完成事件 / 动态改图。
- 不定义 ABI、状态码或槽位。

## 输入、输出与所有权

输入全部借用：定义 `&CompiledDefinition`、Snapshot `&Snapshot`、`GuardFrame` 切片；输出写入调用方 `ItemPlan` / `ActiveState` / `ActionRecord` 切片。纯计算无副作用；`BufferTooSmall` 报三项所需容量，同输入重算逐位一致。

## 依赖与约束

只依赖 `lumio-kernel`。`#![forbid(unsafe_code)]`；禁 `HashMap`。改变迁移语义、指纹编码或输出布局须新 ADR。

## 线程、错误与观测

`evaluate_batch` 对不可变输入可并行调用；注册表以 `Mutex` 保护 arena。错误分两级：整批拒绝 `HfsmError`，逐项 `Outcome::Rejected(ItemError)`。`CompileError` 定位到 state / transition。

## 测试与性能

- Conformance 矩阵 T01–T16、T27、T28、T32–T35 与性质测试（见语义契约 §11）。
- 单次计划复杂度 O(D + C + A)；性能证据由架构仓纵切提供，本仓不承诺数值。

## 相关

- [`.spec/knowledge/features/hfsm.md`](../../.spec/knowledge/features/hfsm.md)
- [Timer 模块](../lumio-timer/README.md)（定时转移只经事件投递）
- [根 README](../../README.md)
