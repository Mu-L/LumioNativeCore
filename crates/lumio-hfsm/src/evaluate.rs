//! The stateless batch evaluator (contract §3, §4, §6).
//!
//! Every item is computed into `Scratch` first; caller buffers are written only after
//! the whole batch fits, so `BufferTooSmall` leaves them untouched.

use crate::compile::{CompiledDefinition, CompiledTransition};
use crate::error::{HfsmError, HfsmResult};
use crate::guard::{GuardFrame, GuardValue};
use crate::ids::{
    ActivationSeq, EventKind, HfsmLimits, Lifecycle, MachineEpoch, MachineKey, StateId, StepSeq,
    TransitionKind,
};
use crate::plan::{
    ActionPhase, ActionRecord, BatchStatus, ItemError, ItemPlan, Outcome, PlanOutput,
};
use crate::snapshot::{ActiveState, Snapshot, SnapshotHeader};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemKind {
    Start { epoch: MachineEpoch },
    Event { event: EventKind },
    Stop,
}

/// Token carried by an async completion; checked at consumption time (contract §6.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryScope {
    pub state: StateId,
    pub activation_seq: ActivationSeq,
    pub epoch: MachineEpoch,
}

pub struct BatchItem<'a> {
    pub machine_key: MachineKey,
    pub definition: &'a CompiledDefinition,
    pub kind: ItemKind,
    /// May be `None` only for `Start`.
    pub snapshot: Option<&'a Snapshot>,
    pub guards: GuardFrame<'a>,
    /// Only consulted for `Event`.
    pub scope: Option<DeliveryScope>,
}

/// Reusable working memory. Sized from limits on first use; grows only when a batch needs more.
#[derive(Default)]
pub struct Scratch {
    items: Vec<ItemPlan>,
    paths: Vec<ActiveState>,
    actions: Vec<ActionRecord>,
    keys: Vec<MachineKey>,
    entered: Vec<StateId>,
}

impl Scratch {
    pub fn new(limits: &HfsmLimits) -> Self {
        let batch = limits.max_batch as usize;
        Self {
            items: Vec::with_capacity(batch),
            paths: Vec::with_capacity(batch * limits.max_depth as usize),
            actions: Vec::with_capacity(batch * limits.max_actions_per_plan as usize),
            keys: Vec::with_capacity(batch),
            entered: Vec::with_capacity(limits.max_depth as usize),
        }
    }
}

pub fn evaluate_batch(
    items: &[BatchItem<'_>],
    limits: &HfsmLimits,
    out: PlanOutput<'_>,
    scratch: &mut Scratch,
) -> HfsmResult<BatchStatus> {
    if !limits.is_valid() {
        return Err(HfsmError::LimitsInvalid);
    }
    let len = items.len() as u32;
    if len > limits.max_batch {
        return Err(HfsmError::BatchTooLarge {
            len,
            max: limits.max_batch,
        });
    }

    scratch.keys.clear();
    scratch.keys.extend(items.iter().map(|i| i.machine_key));
    scratch.keys.sort();
    if let Some(pair) = scratch.keys.windows(2).find(|w| w[0] == w[1]) {
        return Err(HfsmError::DuplicateMachineInBatch { key: pair[0] });
    }

    scratch.items.clear();
    scratch.paths.clear();
    scratch.actions.clear();
    for (index, item) in items.iter().enumerate() {
        let plan = evaluate_item(index as u32, item, scratch);
        scratch.items.push(plan);
    }

    let required_items = scratch.items.len() as u32;
    let required_paths = scratch.paths.len() as u32;
    let required_actions = scratch.actions.len() as u32;
    if out.items.len() < scratch.items.len()
        || out.paths.len() < scratch.paths.len()
        || out.actions.len() < scratch.actions.len()
    {
        return Err(HfsmError::BufferTooSmall {
            required_items,
            required_paths,
            required_actions,
        });
    }
    out.items[..scratch.items.len()].copy_from_slice(&scratch.items);
    out.paths[..scratch.paths.len()].copy_from_slice(&scratch.paths);
    out.actions[..scratch.actions.len()].copy_from_slice(&scratch.actions);
    Ok(BatchStatus {
        items_written: required_items,
        paths_written: required_paths,
        actions_written: required_actions,
    })
}

/// Per-item writer over the scratch buffers; rolls back on rejection.
struct Emitter<'s> {
    scratch: &'s mut Scratch,
    item_index: u32,
    path_start: usize,
    action_start: usize,
    next_seq: ActivationSeq,
}

impl<'s> Emitter<'s> {
    fn new(scratch: &'s mut Scratch, item_index: u32, next_seq: ActivationSeq) -> Self {
        let path_start = scratch.paths.len();
        let action_start = scratch.actions.len();
        Self {
            scratch,
            item_index,
            path_start,
            action_start,
            next_seq,
        }
    }

    fn record(
        &mut self,
        phase: ActionPhase,
        state: StateId,
        seq: ActivationSeq,
        actions: &[crate::ids::ActionId],
    ) {
        for action in actions {
            let ordinal = (self.scratch.actions.len() - self.action_start) as u32;
            self.scratch.actions.push(ActionRecord {
                item_index: self.item_index,
                ordinal,
                phase,
                action: *action,
                state,
                activation_seq: seq,
            });
        }
    }

    fn keep(&mut self, a: ActiveState) {
        self.scratch.paths.push(a);
    }

    /// Allocate a fresh activation seq, push the state, and emit its entry actions.
    fn enter(&mut self, def: &CompiledDefinition, state: StateId) -> Result<(), ItemError> {
        let seq = self.next_seq;
        self.next_seq = ActivationSeq(seq.0.checked_add(1).ok_or(ItemError::CounterOverflow)?);
        self.scratch.paths.push(ActiveState {
            state,
            activation_seq: seq,
        });
        self.record(ActionPhase::Enter, state, seq, def.entry_actions(state));
        Ok(())
    }

    fn exit(&mut self, def: &CompiledDefinition, a: ActiveState) {
        self.record(
            ActionPhase::Exit,
            a.state,
            a.activation_seq,
            def.exit_actions(a.state),
        );
    }

    fn finish(self, outcome: Outcome, mut header: SnapshotHeader) -> ItemPlan {
        header.next_activation_seq = self.next_seq;
        ItemPlan {
            outcome,
            next: Some(header),
            path_start: self.path_start as u32,
            path_len: (self.scratch.paths.len() - self.path_start) as u32,
            action_start: self.action_start as u32,
            action_len: (self.scratch.actions.len() - self.action_start) as u32,
        }
    }

    fn reject(self, error: ItemError) -> ItemPlan {
        self.scratch.paths.truncate(self.path_start);
        self.scratch.actions.truncate(self.action_start);
        ItemPlan {
            outcome: Outcome::Rejected(error),
            next: None,
            path_start: self.path_start as u32,
            path_len: 0,
            action_start: self.action_start as u32,
            action_len: 0,
        }
    }
}

fn evaluate_item(index: u32, item: &BatchItem<'_>, scratch: &mut Scratch) -> ItemPlan {
    let rejected = |scratch: &mut Scratch, e: ItemError| ItemPlan {
        outcome: Outcome::Rejected(e),
        next: None,
        path_start: scratch.paths.len() as u32,
        path_len: 0,
        action_start: scratch.actions.len() as u32,
        action_len: 0,
    };
    match item.kind {
        ItemKind::Start { epoch } => match start(index, item, epoch, scratch) {
            Ok(plan) => plan,
            Err(e) => rejected(scratch, e),
        },
        ItemKind::Event { event } => match event_item(index, item, event, scratch) {
            Ok(plan) => plan,
            Err(e) => rejected(scratch, e),
        },
        ItemKind::Stop => match stop(index, item, scratch) {
            Ok(plan) => plan,
            Err(e) => rejected(scratch, e),
        },
    }
}

/// Shared admission for items that carry a snapshot.
fn admit<'a>(item: &BatchItem<'a>) -> Result<&'a Snapshot, ItemError> {
    let s = item.snapshot.ok_or(ItemError::LifecycleViolation)?;
    if s.machine_key != item.machine_key {
        return Err(ItemError::MachineKeyMismatch);
    }
    s.validate(item.definition)
        .map_err(ItemError::InvalidSnapshot)?;
    Ok(s)
}

fn bump(step: StepSeq) -> Result<StepSeq, ItemError> {
    step.0
        .checked_add(1)
        .map(StepSeq)
        .ok_or(ItemError::CounterOverflow)
}

fn start(
    index: u32,
    item: &BatchItem<'_>,
    epoch: MachineEpoch,
    scratch: &mut Scratch,
) -> Result<ItemPlan, ItemError> {
    let def = item.definition;
    let (step, next_seq) = match item.snapshot {
        None => (StepSeq(0), ActivationSeq(1)),
        Some(_) => {
            let s = admit(item)?;
            if s.lifecycle == Lifecycle::Running {
                return Err(ItemError::LifecycleViolation);
            }
            (s.step_seq, s.next_activation_seq)
        }
    };
    let step = bump(step)?;

    scratch.entered.clear();
    scratch.entered.push(def.root_initial());
    scratch
        .entered
        .extend(def.initial_chain(def.root_initial()));
    let chain = std::mem::take(&mut scratch.entered);
    let mut em = Emitter::new(scratch, index, next_seq);
    let result = chain.iter().try_for_each(|s| em.enter(def, *s));
    em.scratch.entered = chain;
    match result {
        Ok(()) => Ok(em.finish(
            Outcome::Started,
            SnapshotHeader {
                fingerprint: def.fingerprint(),
                machine_key: item.machine_key,
                epoch,
                step_seq: step,
                next_activation_seq: next_seq,
                lifecycle: Lifecycle::Running,
            },
        )),
        Err(e) => Ok(em.reject(e)),
    }
}

fn stop(index: u32, item: &BatchItem<'_>, scratch: &mut Scratch) -> Result<ItemPlan, ItemError> {
    let def = item.definition;
    let s = admit(item)?;
    if s.lifecycle != Lifecycle::Running {
        return Err(ItemError::LifecycleViolation);
    }
    let step = bump(s.step_seq)?;
    let mut em = Emitter::new(scratch, index, s.next_activation_seq);
    for a in s.active_path.iter().rev() {
        em.exit(def, *a);
    }
    let mut header = s.header();
    header.step_seq = step;
    header.lifecycle = Lifecycle::Stopped;
    Ok(em.finish(Outcome::Stopped, header))
}

fn event_item(
    index: u32,
    item: &BatchItem<'_>,
    event: EventKind,
    scratch: &mut Scratch,
) -> Result<ItemPlan, ItemError> {
    let def = item.definition;
    let s = admit(item)?;
    if s.lifecycle != Lifecycle::Running {
        return Err(ItemError::LifecycleViolation);
    }
    if let Some(scope) = item.scope {
        let on_path = s
            .active_path
            .iter()
            .any(|a| a.state == scope.state && a.activation_seq == scope.activation_seq);
        if scope.epoch != s.epoch || !on_path {
            return Err(ItemError::StaleDelivery);
        }
    }
    if !def.has_event(event) {
        return Err(ItemError::UnknownEvent);
    }
    if !item.guards.is_well_formed() {
        return Err(ItemError::GuardFrameInvalid);
    }
    let leaf = s.leaf().ok_or(ItemError::LifecycleViolation)?;
    if let Some(missing) = def
        .required_guards(leaf, event)
        .iter()
        .find(|g| item.guards.lookup(**g) == GuardValue::Missing)
    {
        return Err(ItemError::GuardMissing { guard: *missing });
    }
    let step = bump(s.step_seq)?;

    let selected = select(def, s, event, &item.guards)?;
    let mut header = s.header();
    header.step_seq = step;
    let mut em = Emitter::new(scratch, index, s.next_activation_seq);
    let Some((level, t)) = selected else {
        for a in &s.active_path {
            em.keep(*a);
        }
        return Ok(em.finish(Outcome::Unhandled, header));
    };
    let outcome = Outcome::Transitioned { transition: t.id };
    let source = s.active_path[level];
    let result = match t.kind {
        TransitionKind::Internal => {
            for a in &s.active_path {
                em.keep(*a);
            }
            em.record(
                ActionPhase::Transition,
                source.state,
                source.activation_seq,
                &t.actions,
            );
            Ok(())
        }
        TransitionKind::Local { target } => apply(&mut em, def, s, level, source, t, target),
        TransitionKind::External { target } => {
            let domain = def.strict_common_ancestor(t.source, target);
            let keep = match domain {
                None => 0,
                Some(d) => s
                    .active_path
                    .iter()
                    .position(|a| a.state == d)
                    .map_or(0, |i| i + 1),
            };
            apply(&mut em, def, s, keep, source, t, target)
        }
    };
    match result {
        Ok(()) => Ok(em.finish(outcome, header)),
        Err(e) => Ok(em.reject(e)),
    }
}

/// Exit everything above `keep` (from the leaf inward), record the transition, then enter
/// the path from `keep` down to `target` and its initial chain.
#[allow(clippy::too_many_arguments)]
fn apply(
    em: &mut Emitter<'_>,
    def: &CompiledDefinition,
    s: &Snapshot,
    keep: usize,
    source: ActiveState,
    t: &CompiledTransition,
    target: StateId,
) -> Result<(), ItemError> {
    // Local keeps the source itself: `keep` for Local is `level + 1`.
    let keep = if matches!(t.kind, TransitionKind::Local { .. }) {
        keep + 1
    } else {
        keep
    };
    for a in s.active_path[keep..].iter().rev() {
        em.exit(def, *a);
    }
    em.record(
        ActionPhase::Transition,
        source.state,
        source.activation_seq,
        &t.actions,
    );
    for a in &s.active_path[..keep] {
        em.keep(*a);
    }
    // Ancestors of target strictly below the kept prefix, outermost first.
    let boundary = s.active_path[..keep].last().map(|a| a.state);
    let mut entered = std::mem::take(&mut em.scratch.entered);
    entered.clear();
    let mut cursor = Some(target);
    while let Some(st) = cursor
        && Some(st) != boundary
    {
        entered.push(st);
        cursor = def.parent_of(st);
    }
    entered.reverse();
    entered.extend(def.initial_chain(target));
    let result = entered.iter().try_for_each(|st| em.enter(def, *st));
    em.scratch.entered = entered;
    result
}

/// Bubble from the leaf; returns the path level and transition of the first selected candidate.
fn select<'d>(
    def: &'d CompiledDefinition,
    s: &Snapshot,
    event: EventKind,
    guards: &GuardFrame<'_>,
) -> Result<Option<(usize, &'d CompiledTransition)>, ItemError> {
    for level in (0..s.active_path.len()).rev() {
        let state = s.active_path[level].state;
        for id in def.candidates(state, event) {
            let t = def
                .transition(*id)
                .expect("candidate ids come from the same definition");
            match t.guard {
                None => return Ok(Some((level, t))),
                Some(g) => match guards.lookup(g) {
                    GuardValue::True => return Ok(Some((level, t))),
                    GuardValue::False => {}
                    GuardValue::Missing => return Err(ItemError::GuardMissing { guard: g }),
                },
            }
        }
    }
    Ok(None)
}
