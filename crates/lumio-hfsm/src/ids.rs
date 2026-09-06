//! Graph-local identifiers, instance counters, and configurable limits.
//!
//! Ids are only meaningful within one definition; the crate keeps no global registry.

/// Business state id (graph-local). The synthetic root has no id.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StateId(pub u32);

/// Event kind (graph-local).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventKind(pub u32);

/// Guard id (graph-local); evaluated by the host, never by the kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GuardId(pub u32);

/// Action id (graph-local); emitted in `ActionRecord`, never executed by the kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ActionId(pub u32);

/// Transition id (graph-local).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TransitionId(pub u32);

/// Activation generation of one state on the active path. Starts at 1, never wraps.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ActivationSeq(pub u64);

/// Host-assigned stable machine identity (not a native object address).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MachineKey(pub u64);

/// Host-assigned isolation epoch (rebuild / restore / version change).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MachineEpoch(pub u64);

/// Count of committed legal events.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StepSeq(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionKind {
    Internal,
    Local { target: StateId },
    External { target: StateId },
}

impl TransitionKind {
    pub const fn target(self) -> Option<StateId> {
        match self {
            TransitionKind::Internal => None,
            TransitionKind::Local { target } | TransitionKind::External { target } => Some(target),
        }
    }

    pub(crate) const fn tag(self) -> u32 {
        match self {
            TransitionKind::Internal => 0,
            TransitionKind::Local { .. } => 1,
            TransitionKind::External { .. } => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    NotStarted,
    Running,
    Stopped,
}

/// Resource ceilings. Every field must be non-zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HfsmLimits {
    pub max_depth: u32,
    pub max_states: u32,
    pub max_transitions: u32,
    pub max_actions_per_plan: u32,
    pub max_batch: u32,
}

impl HfsmLimits {
    pub const DEFAULT: Self = Self {
        max_depth: 32,
        max_states: 4096,
        max_transitions: 16384,
        max_actions_per_plan: 128,
        max_batch: 1024,
    };

    pub const fn is_valid(&self) -> bool {
        self.max_depth != 0
            && self.max_states != 0
            && self.max_transitions != 0
            && self.max_actions_per_plan != 0
            && self.max_batch != 0
    }
}

impl Default for HfsmLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
