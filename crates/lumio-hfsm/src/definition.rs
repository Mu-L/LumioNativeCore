//! Flat, host-built definition input. No JSON here: parsing lives in the architecture repo.

use crate::ids::{ActionId, EventKind, GuardId, StateId, TransitionId, TransitionKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSpec {
    pub id: StateId,
    /// `None` = direct child of the synthetic root.
    pub parent: Option<StateId>,
    /// Required for composite states (must be a direct child); must be `None` for leaves.
    pub initial: Option<StateId>,
    pub entry: Vec<ActionId>,
    pub exit: Vec<ActionId>,
    /// Diagnostics only; excluded from the fingerprint.
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionSpec {
    pub id: TransitionId,
    pub source: StateId,
    pub event: EventKind,
    /// Unique within `(source, event)`; lower runs first.
    pub priority: u32,
    pub guard: Option<GuardId>,
    pub kind: TransitionKind,
    pub actions: Vec<ActionId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionSpec {
    /// Must equal [`crate::FORMAT_VERSION`].
    pub format_version: u32,
    /// Initial child of the synthetic root; must be a top-level state.
    pub initial: StateId,
    pub states: Vec<StateSpec>,
    pub transitions: Vec<TransitionSpec>,
}
