//! Read-only plan output (contract §5.5, §6). Nothing here is a business success receipt.

use crate::error::SnapshotError;
use crate::ids::{ActionId, ActivationSeq, GuardId, StateId, TransitionId};
use crate::snapshot::SnapshotHeader;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemError {
    StaleDelivery,
    LifecycleViolation,
    UnknownEvent,
    GuardMissing { guard: GuardId },
    GuardFrameInvalid,
    MachineKeyMismatch,
    InvalidSnapshot(SnapshotError),
    CounterOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Filler for caller-owned buffers; `evaluate_batch` never produces it.
    NotEvaluated,
    Started,
    Transitioned {
        transition: TransitionId,
    },
    Unhandled,
    Stopped,
    Rejected(ItemError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionPhase {
    Exit,
    Transition,
    Enter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionRecord {
    pub item_index: u32,
    /// 0-based position within the item's plan.
    pub ordinal: u32,
    pub phase: ActionPhase,
    pub action: ActionId,
    /// Exit: exited state (old seq). Enter: entered state (new seq). Transition: declared source (old seq).
    pub state: StateId,
    pub activation_seq: ActivationSeq,
}

impl ActionRecord {
    pub const EMPTY: Self = Self {
        item_index: 0,
        ordinal: 0,
        phase: ActionPhase::Exit,
        action: ActionId(0),
        state: StateId(0),
        activation_seq: ActivationSeq(0),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemPlan {
    pub outcome: Outcome,
    /// `None` when rejected.
    pub next: Option<SnapshotHeader>,
    pub path_start: u32,
    pub path_len: u32,
    pub action_start: u32,
    pub action_len: u32,
}

impl ItemPlan {
    pub const EMPTY: Self = Self {
        outcome: Outcome::NotEvaluated,
        next: None,
        path_start: 0,
        path_len: 0,
        action_start: 0,
        action_len: 0,
    };
}

/// Caller-owned output slices. On `BufferTooSmall` none of them is touched.
pub struct PlanOutput<'a> {
    pub items: &'a mut [ItemPlan],
    pub paths: &'a mut [crate::snapshot::ActiveState],
    pub actions: &'a mut [ActionRecord],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchStatus {
    pub items_written: u32,
    pub paths_written: u32,
    pub actions_written: u32,
}
