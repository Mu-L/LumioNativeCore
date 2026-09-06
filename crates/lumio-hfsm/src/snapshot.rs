//! Host-owned instance state (contract §5.3, §7.2). The kernel only reads and validates it.

use crate::compile::CompiledDefinition;
use crate::error::SnapshotError;
use crate::ids::{ActivationSeq, Lifecycle, MachineEpoch, MachineKey, StateId, StepSeq};
use crate::plan::ItemPlan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveState {
    pub state: StateId,
    pub activation_seq: ActivationSeq,
}

impl ActiveState {
    /// Filler for caller-owned output buffers; never a valid path element.
    pub const EMPTY: Self = Self {
        state: StateId(0),
        activation_seq: ActivationSeq(0),
    };
}

/// Everything in a [`Snapshot`] except the path; written per item into `ItemPlan::next`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotHeader {
    pub fingerprint: u64,
    pub machine_key: MachineKey,
    pub epoch: MachineEpoch,
    pub step_seq: StepSeq,
    pub next_activation_seq: ActivationSeq,
    pub lifecycle: Lifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub fingerprint: u64,
    pub machine_key: MachineKey,
    pub epoch: MachineEpoch,
    pub step_seq: StepSeq,
    pub next_activation_seq: ActivationSeq,
    pub lifecycle: Lifecycle,
    /// Top-level → leaf, excluding the synthetic root. Empty unless `Running`.
    pub active_path: Vec<ActiveState>,
}

impl Snapshot {
    pub fn not_started(fingerprint: u64, machine_key: MachineKey) -> Self {
        Self {
            fingerprint,
            machine_key,
            epoch: MachineEpoch(0),
            step_seq: StepSeq(0),
            next_activation_seq: ActivationSeq(1),
            lifecycle: Lifecycle::NotStarted,
            active_path: Vec::new(),
        }
    }

    /// Rebuild the next snapshot from a plan and the batch path buffer. `None` for rejected items.
    pub fn from_plan(plan: &ItemPlan, paths: &[ActiveState]) -> Option<Self> {
        let h = plan.next?;
        let start = plan.path_start as usize;
        let end = start + plan.path_len as usize;
        Some(Self {
            fingerprint: h.fingerprint,
            machine_key: h.machine_key,
            epoch: h.epoch,
            step_seq: h.step_seq,
            next_activation_seq: h.next_activation_seq,
            lifecycle: h.lifecycle,
            active_path: paths.get(start..end)?.to_vec(),
        })
    }

    pub fn header(&self) -> SnapshotHeader {
        SnapshotHeader {
            fingerprint: self.fingerprint,
            machine_key: self.machine_key,
            epoch: self.epoch,
            step_seq: self.step_seq,
            next_activation_seq: self.next_activation_seq,
            lifecycle: self.lifecycle,
        }
    }

    pub fn leaf(&self) -> Option<StateId> {
        self.active_path.last().map(|a| a.state)
    }

    /// Contract §7.2. Counter overflow is detected at the increment site, not here.
    pub fn validate(&self, def: &CompiledDefinition) -> Result<(), SnapshotError> {
        if self.fingerprint != def.fingerprint() {
            return Err(SnapshotError::FingerprintMismatch);
        }
        // Running <=> a non-empty path; any other combination is malformed.
        let running = self.lifecycle == Lifecycle::Running;
        let has_path = !self.active_path.is_empty();
        if running != has_path {
            return Err(SnapshotError::LifecycleMismatch);
        }
        if self.active_path.len() as u32 > def.max_depth() {
            return Err(SnapshotError::PathTooDeep);
        }
        let mut prev: Option<StateId> = None;
        for (i, a) in self.active_path.iter().enumerate() {
            let index = i as u32;
            if !def.contains(a.state) {
                return Err(SnapshotError::UnknownState { state: a.state });
            }
            if def.parent_of(a.state) != prev {
                return Err(SnapshotError::PathNotChain { index });
            }
            if a.activation_seq.0 == 0 || a.activation_seq >= self.next_activation_seq {
                return Err(SnapshotError::ActivationSeqInvalid { index });
            }
            prev = Some(a.state);
        }
        if let Some(leaf) = prev
            && def.is_leaf(leaf) != Some(true)
        {
            return Err(SnapshotError::PathNotEndingAtLeaf);
        }
        Ok(())
    }
}
