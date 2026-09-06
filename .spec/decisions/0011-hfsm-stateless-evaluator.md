# 0011 · 新增 lumio-hfsm：无状态 HFSM 迁移计算器，宿主持有 Snapshot，无回调无时钟

- 日期:2026-09-06
- 状态:生效

## 背景

引擎各层（连接/加载/界面/离散行为）各自手写状态机，语义不一致且不可验证。用户提供 `Lumio_HFSM_Design_v0.1.md`：数据定义的层级状态图 + 无业务副作用的 Rust 迁移计算器 + 宿主唯一持有实例状态 + 显式 Guard/Action 绑定 + 有界非重入事件驱动。本仓评审后收口为本决策。

仓库约束：零外部依赖（`EXTERNAL_ALLOWLIST` 为空）；不保存托管回调；定时内核只有一个（ADR 0008）；跨语言 ABI 只在架构仓（ADR 0009）；`lumio-kernel` 在 DAG 底层。

## 决策

**准入**：新增 crate `lumio-hfsm`（文档模块 `modules/hfsm`），DAG 位置与 `lumio-timer` / `lumio-spatial` 平级，只依赖 `lumio-kernel`。登记 `Cargo.toml` members、`xtask allowed_deps`、`native-core-module-map.md`。

**模型**：Native 是无状态纯计算器，只有 Definition 是 Context 所属资源；实例 Snapshot、业务数据、事件队列、Activity、定时器注册全部归宿主；无 MachineCreate/Destroy。语义契约见 `docs/specs/hfsm-semantics.md`。

**相对 v0.1 的收口**：

- D1 价值主张 = 全引擎唯一一套迁移语义，不承诺优于宿主 switch 的性能；Rust 侧宿主是零 FFI 成本的第一类消费者。
- D2 Guard 范围按活动叶收窄：编译期导出 `required_guards(leaf, event)`；缺任一 = `GuardMissing` 错误。
- D3 动作上限与死转移（无 Guard 转移遮蔽后续）在编译期拒绝，evaluate 不因动作数失败。
- D4 指纹 = 手写 FNV-1a 64 对规范化编码（不含名称与数组顺序）；不用 `DefaultHasher`（跨版本不稳定）。
- D5 crate 内禁 `HashMap`；影响输出的遍历只走 `Vec` / `BTreeMap`。
- D6 自带 `HfsmError`（Timer 先例）；Handle 错误包为 `HfsmError::Handle(ErrorCategory)`。
- D7 `Unhandled` 也输出 next Snapshot（仅 `step_seq + 1`）。
- D8 `MachineKey` 必填；批内去重整批拒绝；跨批由宿主负责。
- D9 公开 `SEMANTICS_VERSION` / `FORMAT_VERSION` 常量。
- D10 本仓 V1 完成门 = 语义规范 + Definition/compile + Snapshot/evaluator + 注册表 + 架构仓 `lumio-engine-native` 构建测试通过；ABI、托管 facade、宿主 adapter、纵切、平台矩阵（v0.1 H04–H08）归架构仓/宿主，本仓文档不承载 v0.1 §10 / §12.3 / §14 / T17–T26。
- D11 拒绝 `statig` / `kaori-hsm`：在 Rust 内持有状态、以 trait/宏组织，与宿主持有 Snapshot 相悖，且供应商白名单为空。自研 table-driven evaluator。
- D12 `.hfsm.json` 解析、生成器、绑定清单全在架构仓；本仓 `DefinitionSpec` 是扁平 Rust 数组。
- D13 跨机器协调（替代正交区域）写进宿主 driver 规则（架构仓）。

**V1 范围**：树状层级、单活动叶、复合默认子、子优先冒泡、显式 priority、Internal / Local / External、Entry / Exit / Transition 动作记录、批量 caller-owned 输出、作用域代次与 StaleDelivery。不做 history / 并行 / eventless / defer / 完成事件 / 动态改图 / 脚本 VM / Native 异步 / 第二定时器。限制档位：深度 32、状态 4096、迁移 16384、单计划动作 128、单批 1024。

## 后果

- 宿主必须自己持有 Snapshot 并在既有提交边界发布；内核不提供事务、队列或总线。
- 引入新公开 Rust API 即改 SDK 编译输入：本仓收口须在架构仓复跑 `cargo build/test -p lumio-engine-native`。
- 正交区域等需求用多台独立机器组合，跨机器协调成本落在宿主。
- 性能优势不作为验收口径；性能证据由架构仓纵切与 benchmark 提供。
