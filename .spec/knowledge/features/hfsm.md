---
name: hfsm
description: 无状态 HFSM 迁移计算器（lumio-hfsm）——建定义、宿主持 Snapshot、批量 evaluate 出计划；接入或写 driver 时读
metadata:
  type: doc
  status: 设计中
---

# 通用 HFSM：无状态迁移计算器

`lumio-hfsm` 把层级状态机的**迁移语义**（冒泡、优先级、迁移域、退出/进入、初始展开、激活代次）做成一份纯计算：宿主持有实例 Snapshot 与业务数据，Native 只算「收到这个事件后应退出什么、进入什么、请求什么动作」。全引擎只有这一套语义实现。

## 背景 / 目标

- 连接/加载/界面流程/离散行为阶段都在各自手写状态机，语义各异、不可验证。
- 目标：数据定义的状态图 + 无副作用的 Rust 计算器 + 宿主唯一所有权 + 无回调、有界、确定性的批量接口。语义契约见 [`hfsm-semantics.md`](../../../docs/specs/hfsm-semantics.md)。

## 设计

- **设计面**：合成根 + 单活动叶路径；三类迁移 Internal / Local / External，External 的迁移域是 source 与 target 的严格共同祖先；Exit 全部先于 Enter；Guard 三态（True / False / Missing），Missing 是错误；`required_guards(leaf, event)` 编译期导出，宿主只算活动路径需要的 Guard；死转移与动作上限在编译期拒绝；指纹 FNV-1a 64 跨版本稳定。
- **交互面**：`compile(spec, limits)` → `CompiledDefinition`；`evaluate_batch(items, limits, out, scratch)` 写调用方缓冲，容量不足整批不写并报所需大小；每 item 三类 `Start / Event / Stop`，输出 `Outcome + next SnapshotHeader + 路径片段 + ActionRecord 片段`；`DeliveryScope` 在消费时拒绝迟到投递（`StaleDelivery`）。
- **实现面**：crate `lumio-hfsm` 只依赖 `lumio-kernel`；`#![forbid(unsafe_code)]`；禁 `HashMap`；`HfsmDefinitionRegistry` 实现 `ContextResource`，定义句柄复用 kernel Handle 的 Context + 槽位 + 世代；不读时钟，定时转移经唯一定时内核（ADR 0008）由宿主投递事件；C ABI 插头与 `.hfsm.json` 工具在架构仓。

## 怎么用

### Rust 进程内

```rust
use lumio_hfsm::*;

// 1. 建定义（扁平数据；架构仓工具可从 .hfsm.json 生成这段）
let spec = DefinitionSpec {
    format_version: FORMAT_VERSION,
    initial: StateId(1),
    states: vec![
        StateSpec { id: StateId(1), parent: None, initial: Some(StateId(2)), entry: vec![], exit: vec![], name: None },          // Workflow
        StateSpec { id: StateId(2), parent: Some(StateId(1)), initial: None, entry: vec![], exit: vec![], name: None },          // Idle
        StateSpec { id: StateId(3), parent: Some(StateId(1)), initial: Some(StateId(4)), entry: vec![], exit: vec![ActionId(103)], name: None }, // Running
        StateSpec { id: StateId(4), parent: Some(StateId(3)), initial: None, entry: vec![ActionId(101)], exit: vec![], name: None }, // Loading
        StateSpec { id: StateId(5), parent: Some(StateId(3)), initial: None, entry: vec![ActionId(102)], exit: vec![], name: None }, // Executing
        StateSpec { id: StateId(6), parent: Some(StateId(1)), initial: None, entry: vec![], exit: vec![], name: None },          // Succeeded
        StateSpec { id: StateId(7), parent: Some(StateId(1)), initial: None, entry: vec![], exit: vec![], name: None },          // Failed
        StateSpec { id: StateId(8), parent: Some(StateId(1)), initial: None, entry: vec![], exit: vec![], name: None },          // Cancelled
    ],
    transitions: vec![
        TransitionSpec { id: TransitionId(1), source: StateId(2), event: EventKind(1), priority: 0, guard: Some(GuardId(1)), kind: TransitionKind::External { target: StateId(3) }, actions: vec![] }, // Idle --Start[CanStart]--> Running
        TransitionSpec { id: TransitionId(2), source: StateId(4), event: EventKind(2), priority: 0, guard: None, kind: TransitionKind::External { target: StateId(5) }, actions: vec![] }, // Loading --LoadSucceeded--> Executing
        TransitionSpec { id: TransitionId(3), source: StateId(4), event: EventKind(3), priority: 0, guard: None, kind: TransitionKind::External { target: StateId(7) }, actions: vec![] },
        TransitionSpec { id: TransitionId(4), source: StateId(5), event: EventKind(4), priority: 0, guard: None, kind: TransitionKind::External { target: StateId(6) }, actions: vec![] },
        TransitionSpec { id: TransitionId(5), source: StateId(5), event: EventKind(5), priority: 0, guard: None, kind: TransitionKind::External { target: StateId(7) }, actions: vec![] },
        TransitionSpec { id: TransitionId(6), source: StateId(3), event: EventKind(6), priority: 0, guard: None, kind: TransitionKind::External { target: StateId(8) }, actions: vec![] }, // Running --Cancel--> Cancelled
        TransitionSpec { id: TransitionId(7), source: StateId(1), event: EventKind(7), priority: 0, guard: None, kind: TransitionKind::Local { target: StateId(2) }, actions: vec![] },    // Workflow --Reset (Local)--> Idle
    ],
};
let limits = HfsmLimits::DEFAULT;
let def = compile(&spec, &limits)?;

// 2. 宿主持有 Snapshot；Start 从空配置开始
let mut items_out = vec![ItemPlan::EMPTY; 1];
let mut paths_out = vec![ActiveState::EMPTY; def.max_depth() as usize];
let mut actions_out = vec![ActionRecord::EMPTY; def.max_plan_actions() as usize];
let mut scratch = Scratch::new(&limits);

let start = BatchItem { machine_key: MachineKey(72), definition: &def, kind: ItemKind::Start { epoch: MachineEpoch(1) }, snapshot: None, guards: GuardFrame::new(&[]), scope: None };
let status = evaluate_batch(&[start], &limits, PlanOutput { items: &mut items_out, paths: &mut paths_out, actions: &mut actions_out }, &mut scratch)?;
let snapshot = Snapshot::from_plan(&items_out[0], &paths_out); // Workflow#1 / Idle#2

// 3. 每轮：算所需 Guard → evaluate → 解释 ActionRecord → 校验 base step 后提交 next Snapshot
let leaf = snapshot.leaf().unwrap();
let needed = def.required_guards(leaf, EventKind(1)); // [GuardId(1)]
let frame = [(GuardId(1), host_can_start())];
let ev = BatchItem { machine_key: MachineKey(72), definition: &def, kind: ItemKind::Event { event: EventKind(1) }, snapshot: Some(&snapshot), guards: GuardFrame::new(&frame), scope: None };
// ... evaluate_batch，读 items_out[0].outcome / actions_out[action_start..]，宿主把 ActionId(101) 解释为「开始加载」意图
```

### 宿主 driver 九步（托管侧同样适用）

1. owner 接纳并排队业务事件（有界、单 owner）。
2. 检查实例 / 操作代次，取不可变业务只读视图。
3. 按 `required_guards(叶, 事件)` 计算 GuardFrame。
4. `evaluate_batch` → 只读计划。
5. 解释 `ActionRecord` 为已有业务命令 / 外部操作意图（不执行 I/O）。
6. 校验 base `epoch` / `step_seq` 与只读视图仍有效。
7. 在既有提交边界发布 next Snapshot 与命令。
8. 提交后启动外部副作用；异步完成带 `DeliveryScope{state, activation_seq, epoch}` 令牌。
9. 完成事件重新入队，下一轮消费；内核在消费时按令牌拒绝迟到投递。

### 作用域令牌

外部 Activity 在**提交后仍存活**的状态 Entry 中启动（如 `Loading#17`），完成事件携带 `DeliveryScope{Loading, 17, epoch}`。若期间实例已 Cancel 并重启到 `Loading#18`，旧完成到达时 `evaluate` 返回 `StaleDelivery`，Snapshot 不变。父状态退出使其下所有子作用域令牌失效；子状态切换不影响父作用域。

## 待解决

- 架构仓侧：`fsm_*` / `hfsm_*` ABI 槽、POD 布局、`.hfsm.json` 生成器、托管 facade（H04–H08，不在本仓）。
- 跨机器协调（替代正交区域）的 driver 规则由架构仓 H06 定。

## 相关

- [`docs/specs/hfsm-semantics.md`](../../../docs/specs/hfsm-semantics.md)
- [ADR 0011](../../decisions/0011-hfsm-stateless-evaluator.md)
- [`crates/lumio-hfsm/README.md`](../../../crates/lumio-hfsm/README.md)
- 代码：`crates/lumio-hfsm/`
