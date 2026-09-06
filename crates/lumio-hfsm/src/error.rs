//! Crate error vocabulary (ADR 0010 D6). No public numeric codes here.

use lumio_kernel::error::ErrorCategory;

use crate::ids::{MachineKey, StateId, TransitionId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileErrorKind {
    LimitsInvalid,
    FormatVersionUnsupported { found: u32 },
    EmptyDefinition,
    DuplicateStateId,
    DuplicateTransitionId,
    UnknownState { referenced: StateId },
    RootInitialNotTopLevel,
    InitialNotDirectChild,
    CompositeWithoutInitial,
    LeafWithInitial,
    StructuralCycle,
    DepthExceeded { depth: u32, max: u32 },
    StateCountExceeded { count: u32, max: u32 },
    TransitionCountExceeded { count: u32, max: u32 },
    DuplicatePriority { other: TransitionId },
    UnreachableTransition { shadowed_by: TransitionId },
    LocalTargetNotStrictDescendant,
    ActionsPerPlanExceeded { required: u32, max: u32 },
}

/// Compile failure located to a state and/or transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileError {
    pub kind: CompileErrorKind,
    pub state: Option<StateId>,
    pub transition: Option<TransitionId>,
}

impl CompileError {
    pub(crate) const fn new(kind: CompileErrorKind) -> Self {
        Self {
            kind,
            state: None,
            transition: None,
        }
    }

    pub(crate) const fn at_state(kind: CompileErrorKind, state: StateId) -> Self {
        Self {
            kind,
            state: Some(state),
            transition: None,
        }
    }

    pub(crate) const fn at_transition(kind: CompileErrorKind, transition: TransitionId) -> Self {
        Self {
            kind,
            state: None,
            transition: Some(transition),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    FingerprintMismatch,
    LifecycleMismatch,
    PathTooDeep,
    UnknownState { state: StateId },
    PathNotChain { index: u32 },
    PathNotEndingAtLeaf,
    ActivationSeqInvalid { index: u32 },
    CounterOverflow,
}

/// Batch-level failure: nothing is written when one of these is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HfsmError {
    Compile(CompileError),
    /// Kernel handle / context failure category (WrongContext, InvalidHandle, ContextDestroyed …).
    Handle(ErrorCategory),
    LimitsInvalid,
    BatchTooLarge {
        len: u32,
        max: u32,
    },
    DuplicateMachineInBatch {
        key: MachineKey,
    },
    BufferTooSmall {
        required_items: u32,
        required_paths: u32,
        required_actions: u32,
    },
}

impl From<CompileError> for HfsmError {
    fn from(e: CompileError) -> Self {
        HfsmError::Compile(e)
    }
}

pub type HfsmResult<T> = Result<T, HfsmError>;
