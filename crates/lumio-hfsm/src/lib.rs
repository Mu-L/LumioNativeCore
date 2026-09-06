//! `lumio-hfsm`：无状态层级有限状态机迁移计算器（ADR 0010）。
//!
//! 输入不可变定义 + 宿主持有的 Snapshot + 事件与 GuardFrame，输出只读迁移计划。
//! 不持有实例、不执行业务、不回调宿主、不读时钟。
//! 语义契约见 `docs/specs/hfsm-semantics.md`。

#![forbid(unsafe_code)]

mod compile;
mod definition;
mod error;
mod evaluate;
mod fingerprint;
mod guard;
mod ids;
mod plan;
mod registry;
mod snapshot;

pub use compile::{CompiledDefinition, CompiledTransition, compile};
pub use definition::{DefinitionSpec, StateSpec, TransitionSpec};
pub use error::{CompileError, CompileErrorKind, HfsmError, HfsmResult, SnapshotError};
pub use evaluate::{BatchItem, DeliveryScope, ItemKind, Scratch, evaluate_batch};
pub use guard::{GuardFrame, GuardValue};
pub use ids::{
    ActionId, ActivationSeq, EventKind, GuardId, HfsmLimits, Lifecycle, MachineEpoch, MachineKey,
    StateId, StepSeq, TransitionId, TransitionKind,
};
pub use plan::{ActionPhase, ActionRecord, BatchStatus, ItemError, ItemPlan, Outcome, PlanOutput};
pub use registry::{DefinitionHandle, DefinitionInfo, HfsmDefinitionRegistry};
pub use snapshot::{ActiveState, Snapshot, SnapshotHeader};

/// 迁移语义版本（契约 §8 指纹编码的一部分）。
pub const SEMANTICS_VERSION: u32 = 1;
/// `DefinitionSpec` 格式版本。
pub const FORMAT_VERSION: u32 = 1;
