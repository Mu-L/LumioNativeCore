# HFSM 语义、数据模型与批量计算契约（设计现状）

> 对应决策：[`0010`](../../.spec/decisions/0010-hfsm-stateless-evaluator.md)。
> 来源：`Lumio_HFSM_Design_v0.1.md`（Proposal 0.1，2026-09-06）经本仓评审收口。
> 状态：语义已定；跨语言布局、状态码与槽位由架构仓 `native-abi.json` 另行定义，本文不分配。
> 常量：`SEMANTICS_VERSION = 1`、`FORMAT_VERSION = 1`（`lumio-hfsm` 公开导出）。

## 1. 定位与价值主张

`lumio-hfsm` 是**无状态的层级有限状态机迁移计算器**：输入不可变定义、宿主持有的实例 Snapshot、事件与 GuardFrame，输出一份只读迁移计划（下一 Snapshot + 有序动作记录）。它不持有实例、不执行业务、不回调宿主、不读时钟。

价值主张只有一条：**全引擎只有一套迁移语义**（候选选择、父级冒泡、迁移域、退出/进入顺序、初始展开、激活代次）。不承诺比宿主手写 switch 更快；Rust 侧宿主（架构仓 `sdk-native` 内的连接/会话类流程）是零 FFI 成本的第一类消费者，托管侧经批量 ABI 消费同一实现。

适用：连接/认证/重连、资源加载、界面流程、服务启停、离散行为阶段、工具任务流程。不负责：ECS 存储、GAS 结算、网络、事务、持久化、线程调度、寻路、动画、鉴权。

## 2. 状态树与稳定配置

- 存在一个**合成根**（不在 `DefinitionSpec.states` 里，没有 id）。业务状态的 `parent: None` 即挂在合成根下。合成根没有动作，不能作迁移目标或源。
- 每个业务状态恰有一个父。非叶状态必须指定唯一 `initial`，且 `initial` 必须是其**直接子状态**；叶状态不得有 `initial`。`DefinitionSpec.initial` 是合成根的 initial，必须是顶层状态。
- **稳定配置** = 从某个顶层状态到某个叶状态的一条完整路径（`active_path`，不含合成根）。不能同时激活兄弟状态，不能停在复合状态上。
- 进入复合状态时沿 `initial` 链展开到叶。

## 3. 事件选择

从活动叶开始逐级向父查找。每层只看该 `EventKind` 的候选转移，按 `priority` 从小到大取**第一个 Guard 为 True** 的转移。

- 同 `(source, event)` 的 `priority` 必须唯一，编译期拒绝重复（`DuplicatePriority`）。不以数组顺序、名字或任何隐式规则决胜。
- 本层候选 Guard 全为 False → 继续查父。**深度优先于 priority**：父层 priority=0 不覆盖已匹配的子层 priority=10。
- Guard 三态：`True` / `False` / `Missing`。`Missing` 不是 False，是逐项错误 `GuardMissing{guard}`；本 item 无有效计划，其余 item 不受影响。
- 事件已在定义中登记但逐层都无 True 候选 → `Unhandled`（合法决策，见 §6.4）。事件未在定义中出现过 → `UnknownEvent`（输入错误）。
- 一次计算对一个实例**最多选一条转移**；不广播、不扫描非活动子树。
- 编译期为每个 `(叶状态, EventKind)` 导出**所需 Guard 有序集** `required_guards(leaf, event)`：沿冒泡路径逐层、层内按 priority、去重。宿主只需计算这些 Guard；这是 v0.1 §5.3「算整图全部 Guard」的收窄（ADR 0010 D2）。

## 4. 三类迁移与迁移域

| 类型 | 目标约束 | 退出 / 进入 |
| --- | --- | --- |
| `Internal` | 无目标 | 不退出、不进入；只产转移动作；激活代次全部不变 |
| `Local { target }` | `source` 必须是复合状态，`target` 必须是 `source` 的**严格后代** | 保留 `source`；退出 `source` 之下的整条活动子路径；进入到 `target` 的路径，再沿 `initial` 展开 |
| `External { target }` | `target` 是任意业务状态 | 迁移域 D = `source` 与 `target` 的**严格共同祖先**；退出 D 之下活动路径；进入 D 之下到 `target` 的路径，再沿 `initial` 展开 |

严格共同祖先：先取普通 LCA；若 LCA 等于 `source` 或 `target`，取其父（可为合成根）。迁移域按**转移声明的 `source`** 计算，不按当前活动叶。合成根永不退出。`Local` 即使 `target` 已在当前活动子路径中也退出重进，不得优化成 `Internal`。

### 4.1 六个边界例子（Conformance 向量，T02–T06）

```text
Root
└─ A (initial: A1)
   ├─ A1
   └─ A2
```

活动路径均为 `A#1 / A1#2`（`#n` 为 ActivationSeq），`NextActivationSeq = 3`。

| 迁移 | 记录序列 | 下一路径 |
| --- | --- | --- |
| A1 → A2，External | Exit(A1#2) · Transition(src A1#2) · Enter(A2#3) | A#1 / A2#3 |
| A → A2，External | Exit(A1#2) · Exit(A#1) · Transition(src A#1) · Enter(A#3) · Enter(A2#4) | A#3 / A2#4 |
| A → A2，Local | Exit(A1#2) · Transition(src A#1) · Enter(A2#3) | A#1 / A2#3 |
| A1 → A1，External | Exit(A1#2) · Transition(src A1#2) · Enter(A1#3) | A#1 / A1#3 |
| A1，Internal | Transition(src A1#2) | A#1 / A1#2 |
| A1 → A，External | Exit(A1#2) · Exit(A#1) · Transition(src A1#2) · Enter(A#3) · Enter(A1#4) | A#3 / A1#4 |

每个 Exit / Enter 行展开为该状态 `exit` / `entry` 列表中的动作，按定义顺序；Transition 行展开为转移 `actions`。

### 4.2 动作记录顺序

```text
Exit：从叶到迁移域的下一层（内→外）
→ Transition：转移声明中的显式顺序
→ Enter：从迁移域下一层到目标，再沿 initial 到叶（外→内）
```

所有 Exit 在所有 Enter 之前。退出动作引用**旧**代次；进入动作引用**新**代次；转移动作引用声明 `source` 的旧代次（仅作逻辑来源，不代表该作用域在提交后仍存活）。

### 4.3 Start / Stop / 生命周期

- `Start{epoch}`：允许于 `snapshot = None`、`Lifecycle::NotStarted` 或 `Lifecycle::Stopped`；`Running` 上再 Start = `LifecycleViolation`。从空配置沿 `initial` 链展开，Enter 记录外→内，逐个分配新代次；`epoch` 取 item 给定值；`step_seq` 与 `next_activation_seq` 从旧 Snapshot 延续（重启后的代次严格大于历史代次）。
- `Event`：要求 `Lifecycle::Running`，否则 `LifecycleViolation`。
- `Stop`：要求 `Running`；产出从叶向外的 Exit 记录，清空路径，`lifecycle = Stopped`，`step_seq + 1`。之后普通事件被拒绝。
- 框架故障（评估失败）与业务 `Failed` 状态是两回事；内核不自动迁入任何错误状态。

## 5. 数据模型

### 5.1 DefinitionSpec（扁平 Rust 数据，不含 JSON）

```rust
pub struct StateSpec {
    pub id: StateId,                 // 图内唯一
    pub parent: Option<StateId>,     // None = 合成根之下
    pub initial: Option<StateId>,    // 复合状态必填且为直接子；叶必须 None
    pub entry: Vec<ActionId>,
    pub exit: Vec<ActionId>,
    pub name: Option<String>,        // 仅诊断，不进指纹
}
pub struct TransitionSpec {
    pub id: TransitionId,            // 图内唯一
    pub source: StateId,
    pub event: EventKind,
    pub priority: u32,               // 同 (source, event) 内唯一
    pub guard: Option<GuardId>,
    pub kind: TransitionKind,        // Internal | Local{target} | External{target}
    pub actions: Vec<ActionId>,
}
pub struct DefinitionSpec {
    pub format_version: u32,         // 必须 == FORMAT_VERSION
    pub initial: StateId,            // 合成根的 initial，须为顶层状态
    pub states: Vec<StateSpec>,
    pub transitions: Vec<TransitionSpec>,
}
```

`StateId / GuardId / ActionId / EventKind / TransitionId` 只在所属图内有意义；本仓不设全局注册表。`.hfsm.json` 的解析、生成器与绑定清单全部在架构仓。

### 5.2 CompiledDefinition（不可变、可共享）

导出：`fingerprint()`、`parent_of(state)`、`depth_of(state)`、`is_leaf(state)`、`initial_chain(state)`（从 `state.initial` 到叶）、`candidates(state, event)`（按 priority 升序的 TransitionId）、`required_guards(leaf, event)`、`transition(id)`、`max_plan_actions()`、`max_depth()`、`name_of(state)`、`state_count()` / `transition_count()`。不预计算 state×state 矩阵。内部只用 `Vec` / `BTreeMap`。

### 5.3 Snapshot（宿主唯一持有）

```rust
pub struct ActiveState { pub state: StateId, pub activation_seq: ActivationSeq }
pub struct Snapshot {
    pub fingerprint: u64,                // 必须等于定义指纹
    pub machine_key: MachineKey,         // 业务实例稳定身份，宿主分配，必填
    pub epoch: MachineEpoch,             // 销毁重建 / 恢复 / 换版本后的异步隔离代次，宿主分配
    pub step_seq: StepSeq,               // 已提交合法事件的顺序号
    pub next_activation_seq: ActivationSeq, // 下次进入状态分配的代次，从 1 起
    pub lifecycle: Lifecycle,            // NotStarted | Running | Stopped
    pub active_path: Vec<ActiveState>,   // 顶层→叶，不含合成根；非 Running 时为空
}
```

同一父状态持续激活时其代次不变；退出重进的状态分配新代次。任一计数器溢出 → `CounterOverflow`，不回绕。校验规则见 §7.2。

### 5.4 GuardFrame

`GuardFrame<'a>` 包装 `&'a [(GuardId, bool)]`；`lookup(guard) -> True | False | Missing`。同一 `GuardId` 出现两次 → `GuardFrameInvalid`。宿主按 `required_guards(leaf, event)` 计算即可，多给不报错。

### 5.5 Plan / ActionRecord

```rust
pub enum Outcome { Started, Transitioned { transition: TransitionId }, Unhandled, Stopped, Rejected(ItemError) }
pub enum ItemError {
    StaleDelivery, LifecycleViolation, UnknownEvent,
    GuardMissing { guard: GuardId }, GuardFrameInvalid,
    MachineKeyMismatch, InvalidSnapshot(SnapshotError), CounterOverflow,
}
pub enum ActionPhase { Exit, Transition, Enter }
pub struct ActionRecord {
    pub item_index: u32, pub ordinal: u32, pub phase: ActionPhase,
    pub action: ActionId, pub state: StateId, pub activation_seq: ActivationSeq,
}
pub struct SnapshotHeader { /* Snapshot 除 active_path 外全部字段 + path_len */ }
pub struct ItemPlan {
    pub outcome: Outcome,
    pub next: Option<SnapshotHeader>,        // Rejected 时为 None
    pub path_start: u32, pub path_len: u32,  // 指向 PlanOutput.paths
    pub action_start: u32, pub action_len: u32, // 指向 PlanOutput.actions
}
```

业务 payload 不穿过内核：宿主保留原事件，用 `item_index` 把动作关联回去。

## 6. 批量计算契约

```rust
pub enum ItemKind { Start { epoch: MachineEpoch }, Event { event: EventKind }, Stop }
pub struct DeliveryScope { pub state: StateId, pub activation_seq: ActivationSeq, pub epoch: MachineEpoch }
pub struct BatchItem<'a> {
    pub machine_key: MachineKey,
    pub definition: &'a CompiledDefinition,
    pub kind: ItemKind,
    pub snapshot: Option<&'a Snapshot>,   // Start 可为 None
    pub guards: GuardFrame<'a>,
    pub scope: Option<DeliveryScope>,     // 仅 Event 使用
}
pub struct PlanOutput<'a> { pub items: &'a mut [ItemPlan], pub paths: &'a mut [ActiveState], pub actions: &'a mut [ActionRecord] }
pub fn evaluate_batch(items: &[BatchItem<'_>], limits: &HfsmLimits, out: PlanOutput<'_>, scratch: &mut Scratch)
    -> Result<BatchStatus, HfsmError>;
```

### 6.1 整批拒绝（不写任何输出）

`LimitsInvalid`、`BatchTooLarge{len, max}`、`DuplicateMachineInBatch{key}`（同批同 `machine_key` 出现两次）、`BufferTooSmall{required_items, required_paths, required_actions}`。

`BufferTooSmall` 在全部 item 计算完成后才判定，输出**一个都不写**；同一输入扩容后重算结果逐位一致（纯计算，无消费、无副作用）。

### 6.2 逐项 outcome

单个 item 的非法 Snapshot / Guard / 生命周期 / 作用域问题以 `Outcome::Rejected(ItemError)` 报告；该 item 无 `next`、无动作，其余 item 正常。是否整组提交由宿主既有相位规则决定。

### 6.3 DeliveryScope 与 StaleDelivery

`Event` 带 `scope` 时先校验：`scope.epoch == snapshot.epoch` 且 `(scope.state, scope.activation_seq)` 出现在 `active_path` 中，否则 `StaleDelivery`，Snapshot 完全不变。这是在**消费时**检查，与宿主入队时的检查互不替代。宿主侧 `OperationId` 的终态去重不在内核。

### 6.4 Unhandled

合法决策：无动作，`next` = 原 Snapshot 但 `step_seq + 1`（路径与代次不变）。`StaleDelivery`、输入错误、缓冲不足不推进 `step_seq`。

### 6.5 确定性与非重入

输出只由 `(定义, Snapshot, ItemKind, GuardFrame, scope)` 决定；同一批内 item 之间互不影响；不含地址、时间戳或随机迭代顺序。宿主保证每实例单 owner 推进、同批一实例一次、新事件进下一轮。

## 7. 编译期与 Snapshot 校验清单

### 7.1 `compile(spec, limits) -> Result<CompiledDefinition, CompileError>`

| `CompileErrorKind` | 触发 |
| --- | --- |
| `LimitsInvalid` | 任一上限为 0 |
| `FormatVersionUnsupported` | `format_version != FORMAT_VERSION` |
| `EmptyDefinition` | 无状态 |
| `DuplicateStateId` / `DuplicateTransitionId` | id 重复 |
| `UnknownState` | parent / initial / source / target / 根 initial 引用不存在的状态 |
| `RootInitialNotTopLevel` | 根 initial 有 parent |
| `InitialNotDirectChild` | `initial` 不是直接子 |
| `CompositeWithoutInitial` / `LeafWithInitial` | 复合缺 initial / 叶带 initial |
| `StructuralCycle` | parent 链成环 |
| `DepthExceeded` / `StateCountExceeded` / `TransitionCountExceeded` | 超 `HfsmLimits` |
| `DuplicatePriority { other }` | 同 (source, event) priority 重复 |
| `UnreachableTransition { shadowed_by }` | 同 (source, event) 内，无 Guard 转移之后（priority 更大）的转移 |
| `LocalTargetNotStrictDescendant` | Local 的 target 不是 source 的严格后代（含 source 为叶） |
| `ActionsPerPlanExceeded { transition, required }` | 某转移最坏动作数（退出链 + 转移 + 进入链到叶）> `max_actions_per_plan`；Start 的初始展开同样计入 |

任何失败不产出半个定义。重复事件形成的业务环（Start/Reset 循环）合法，不属结构环。

### 7.2 Snapshot 校验（evaluate 前）

`FingerprintMismatch`、`LifecycleMismatch`（Running 却空路径 / 非 Running 却有路径）、`PathTooDeep`、`UnknownState`、`PathNotChain`（首元素不是顶层，或相邻元素不构成父子）、`PathNotEndingAtLeaf`、`ActivationSeqInvalid`（为 0，或 ≥ `next_activation_seq`）、`CounterOverflow`。`machine_key` 与 item 不一致 → `MachineKeyMismatch`。

## 8. 确定性与指纹

- **指纹算法**：FNV-1a 64（offset `0xcbf29ce484222325`，prime `0x100000001b3`），对规范化字节流计算，跨 Rust 版本稳定。
- **规范化编码**（全部小端）：`b"LUMIO-HFSM\0"` · `FORMAT_VERSION:u32` · `SEMANTICS_VERSION:u32` · `initial:u32` · `state_count:u32` · 状态按 id 升序，每个：`id` · `parent | 0xFFFF_FFFF` · `initial | 0xFFFF_FFFF` · `entry_len` · `entry[]` · `exit_len` · `exit[]` · `transition_count:u32` · 转移按 `(source, event, priority)` 升序，每个：`id` · `source` · `event` · `priority` · `guard | 0xFFFF_FFFF` · `kind_tag:u32`（0 Internal / 1 Local / 2 External）· `target | 0xFFFF_FFFF` · `actions_len` · `actions[]`。名称与数组原始顺序不进指纹。
- **容器规则**：crate 内禁用 `HashMap` / `HashSet`；影响输出的任何遍历只走 `Vec` / `BTreeMap`。
- 指纹相同只证明状态图语义相同，不证明宿主 handler 未变。

## 9. 与 Context、Timer 的关系

- `HfsmDefinitionRegistry` 实现 `ContextResource`（`name = "hfsm"`）：`create(spec)` 编译并存入 `HandleArena<Arc<CompiledDefinition>>`，返回 `DefinitionHandle`（Context + 槽位 + 世代）；`lease(handle)` 返回 `Arc` 克隆作为在途只读租约，`release` 后已发出的租约不失效；`cancel_requested` 后拒绝 `create`；`quiesce` 立即 `Quiesced`（evaluate 是同步纯计算，无在途状态）；`destroy` 清空 arena，之后所有操作返回 `ContextDestroyed`。跨 Context、重复释放、过期世代按 kernel Handle 规则拒绝，包成 `HfsmError::Handle(ErrorCategory)`。
- 定时：内核不读时钟、不注册超时。定时转移 = 宿主向唯一定时内核（ADR 0008）注册，到期后宿主以带 `DeliveryScope` 的事件投递。

## 10. V1 不支持

History（浅/深）、正交并行区域、Push/Pop 状态栈、eventless（自动）迁移、完成事件 / 最终状态、状态级 Defer 队列、动态修改在用定义、脚本表达式 / VM、Native 内异步 Activity、第二个定时器、Native 侧实例存储（无 MachineCreate/Destroy）。并发业务用多台独立机器 + 显式消息组合。

## 11. Conformance 矩阵

| 编号 | 场景 | 必须观察到 |
| --- | --- | --- |
| T01 | Start 多层 initial | 只进入 initial 链，Enter 外→内，代次递增 |
| T02–T06 | §4.1 六例 | 记录序列与下一路径逐项相等 |
| T07 | 子 Guard 假、父 Guard 真 | 冒泡到父转移 |
| T08 | 子父同时匹配 | 子优先，不因父 priority 更小被覆盖 |
| T09 | 同 source/event priority 重复 | `compile` 拒绝并指出两条转移 |
| T10 | Guard 缺失 / 帧重复 | `GuardMissing` / `GuardFrameInvalid`，不降级为 False |
| T11 | 合法事件 Unhandled | 无动作，`next.step_seq` = 旧 + 1，路径不变 |
| T12 | Running 上 Start / Stop 后 Event | `LifecycleViolation`，不重复 Entry |
| T13 | BufferTooSmall 后原输入重试 | 首次零输出；扩容后结果逐位一致 |
| T15 | 同批同实例两 item | `DuplicateMachineInBatch` 整批拒绝 |
| T16 | 旧 Loading#17 完成到达 #18 | `StaleDelivery`，Snapshot 不变 |
| T27 | 路径断裂 / 计数溢出 | 有诊断的拒绝，不 wrap、不截断 |
| T28 | 坏 Handle / 跨 Context / 提前释放 | 明确拒绝；已发租约在释放后仍可用 |
| T32 | 父子结构环与合法迁移环 | 拒绝结构环；Start/Reset 业务环可编译 |
| T33 | 死转移 / 动作超限 | `UnreachableTransition` / `ActionsPerPlanExceeded` 在编译期 |
| T34 | 指纹规范化 | 改名 / 改数组顺序不变；改 initial / guard / kind / actions 变 |
| T35 | required_guards | 与冒泡路径逐层 priority 序一致，去重 |
| P | 性质（随机图 + 事件序列） | 合法配置总为顶层→叶单路径；Exit 全在 Enter 前；保留状态代次不变、重进代次变；Internal 不改路径；纯计算不改输入；`step_seq` 每合法事件 +1 |

T14、T17–T26、T29–T31 属宿主 / 架构仓验收（v0.1 §16），不在本仓。
