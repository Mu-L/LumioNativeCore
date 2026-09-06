//! Definition validation and the immutable compiled graph (contract §5.2, §7.1).
//!
//! Deterministic by construction: only `Vec` / `BTreeMap`, never `HashMap` (ADR 0010 D5).

use std::collections::BTreeMap;

use crate::definition::{DefinitionSpec, StateSpec, TransitionSpec};
use crate::error::{CompileError, CompileErrorKind as K};
use crate::fingerprint::Fnv1a;
use crate::ids::{ActionId, EventKind, GuardId, HfsmLimits, StateId, TransitionId, TransitionKind};
use crate::{FORMAT_VERSION, SEMANTICS_VERSION};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledTransition {
    pub id: TransitionId,
    pub source: StateId,
    pub event: EventKind,
    pub priority: u32,
    pub guard: Option<GuardId>,
    pub kind: TransitionKind,
    pub actions: Vec<ActionId>,
}

#[derive(Clone, Debug)]
struct CompiledState {
    id: StateId,
    parent: Option<StateId>,
    initial: Option<StateId>,
    depth: u32,
    entry: Vec<ActionId>,
    exit: Vec<ActionId>,
    name: Option<String>,
}

/// Immutable, shareable compiled graph.
#[derive(Clone, Debug)]
pub struct CompiledDefinition {
    limits: HfsmLimits,
    fingerprint: u64,
    root_initial: StateId,
    states: Vec<CompiledState>,
    index: BTreeMap<StateId, u32>,
    transitions: Vec<CompiledTransition>,
    transition_index: BTreeMap<TransitionId, u32>,
    candidates: BTreeMap<(StateId, EventKind), Vec<TransitionId>>,
    required_guards: BTreeMap<(StateId, EventKind), Vec<GuardId>>,
    events: Vec<EventKind>,
    max_depth: u32,
    max_plan_actions: u32,
}

pub fn compile(
    spec: &DefinitionSpec,
    limits: &HfsmLimits,
) -> Result<CompiledDefinition, CompileError> {
    if !limits.is_valid() {
        return Err(CompileError::new(K::LimitsInvalid));
    }
    if spec.format_version != FORMAT_VERSION {
        return Err(CompileError::new(K::FormatVersionUnsupported {
            found: spec.format_version,
        }));
    }
    if spec.states.is_empty() {
        return Err(CompileError::new(K::EmptyDefinition));
    }

    let index = index_states(&spec.states)?;
    let count = spec.states.len() as u32;
    if count > limits.max_states {
        return Err(CompileError::new(K::StateCountExceeded {
            count,
            max: limits.max_states,
        }));
    }
    let tcount = spec.transitions.len() as u32;
    if tcount > limits.max_transitions {
        return Err(CompileError::new(K::TransitionCountExceeded {
            count: tcount,
            max: limits.max_transitions,
        }));
    }

    check_references(spec, &index)?;
    let depths = compute_depths(&spec.states, &index, limits)?;
    check_initials(&spec.states, &index)?;

    let states: Vec<CompiledState> = spec
        .states
        .iter()
        .zip(depths.iter())
        .map(|(s, depth)| CompiledState {
            id: s.id,
            parent: s.parent,
            initial: s.initial,
            depth: *depth,
            entry: s.entry.clone(),
            exit: s.exit.clone(),
            name: s.name.clone(),
        })
        .collect();
    let max_depth = depths.iter().copied().max().unwrap_or(0);

    let (transitions, transition_index) = index_transitions(&spec.transitions)?;
    let mut def = CompiledDefinition {
        limits: *limits,
        fingerprint: 0,
        root_initial: spec.initial,
        states,
        index,
        transitions,
        transition_index,
        candidates: BTreeMap::new(),
        required_guards: BTreeMap::new(),
        events: Vec::new(),
        max_depth,
        max_plan_actions: 0,
    };

    def.check_local_targets()?;
    def.build_candidates()?;
    def.build_required_guards();
    def.max_plan_actions = def.compute_max_plan_actions()?;
    def.fingerprint = def.compute_fingerprint(spec);
    Ok(def)
}

fn index_states(states: &[StateSpec]) -> Result<BTreeMap<StateId, u32>, CompileError> {
    let mut index = BTreeMap::new();
    for (i, s) in states.iter().enumerate() {
        if index.insert(s.id, i as u32).is_some() {
            return Err(CompileError::at_state(K::DuplicateStateId, s.id));
        }
    }
    Ok(index)
}

fn check_references(
    spec: &DefinitionSpec,
    index: &BTreeMap<StateId, u32>,
) -> Result<(), CompileError> {
    for s in &spec.states {
        for referenced in [s.parent, s.initial].into_iter().flatten() {
            if !index.contains_key(&referenced) {
                return Err(CompileError::at_state(K::UnknownState { referenced }, s.id));
            }
        }
    }
    match index.get(&spec.initial) {
        None => {
            return Err(CompileError::new(K::UnknownState {
                referenced: spec.initial,
            }));
        }
        Some(i) => {
            if spec.states[*i as usize].parent.is_some() {
                return Err(CompileError::at_state(
                    K::RootInitialNotTopLevel,
                    spec.initial,
                ));
            }
        }
    }
    for t in &spec.transitions {
        for referenced in [Some(t.source), t.kind.target()].into_iter().flatten() {
            if !index.contains_key(&referenced) {
                return Err(CompileError::at_transition(
                    K::UnknownState { referenced },
                    t.id,
                ));
            }
        }
    }
    Ok(())
}

/// Depth = ancestors + 1. A parent chain longer than the state count is a cycle.
fn compute_depths(
    states: &[StateSpec],
    index: &BTreeMap<StateId, u32>,
    limits: &HfsmLimits,
) -> Result<Vec<u32>, CompileError> {
    let n = states.len();
    let mut depths = Vec::with_capacity(n);
    for s in states {
        let mut depth = 1u32;
        let mut cursor = s.parent;
        while let Some(p) = cursor {
            depth += 1;
            if depth as usize > n {
                return Err(CompileError::at_state(K::StructuralCycle, s.id));
            }
            cursor = states[index[&p] as usize].parent;
        }
        if depth > limits.max_depth {
            return Err(CompileError::at_state(
                K::DepthExceeded {
                    depth,
                    max: limits.max_depth,
                },
                s.id,
            ));
        }
        depths.push(depth);
    }
    Ok(depths)
}

fn check_initials(
    states: &[StateSpec],
    index: &BTreeMap<StateId, u32>,
) -> Result<(), CompileError> {
    let mut has_children = vec![false; states.len()];
    for s in states {
        if let Some(p) = s.parent {
            has_children[index[&p] as usize] = true;
        }
    }
    for (s, composite) in states.iter().zip(has_children) {
        match (composite, s.initial) {
            (true, None) => return Err(CompileError::at_state(K::CompositeWithoutInitial, s.id)),
            (false, Some(_)) => return Err(CompileError::at_state(K::LeafWithInitial, s.id)),
            (true, Some(initial)) => {
                if states[index[&initial] as usize].parent != Some(s.id) {
                    return Err(CompileError::at_state(K::InitialNotDirectChild, s.id));
                }
            }
            (false, None) => {}
        }
    }
    Ok(())
}

fn index_transitions(
    specs: &[TransitionSpec],
) -> Result<(Vec<CompiledTransition>, BTreeMap<TransitionId, u32>), CompileError> {
    let mut index = BTreeMap::new();
    let mut out = Vec::with_capacity(specs.len());
    for (i, t) in specs.iter().enumerate() {
        if index.insert(t.id, i as u32).is_some() {
            return Err(CompileError::at_transition(K::DuplicateTransitionId, t.id));
        }
        out.push(CompiledTransition {
            id: t.id,
            source: t.source,
            event: t.event,
            priority: t.priority,
            guard: t.guard,
            kind: t.kind,
            actions: t.actions.clone(),
        });
    }
    Ok((out, index))
}

impl CompiledDefinition {
    fn state(&self, id: StateId) -> Option<&CompiledState> {
        self.index.get(&id).map(|i| &self.states[*i as usize])
    }

    fn check_local_targets(&self) -> Result<(), CompileError> {
        for t in &self.transitions {
            if let TransitionKind::Local { target } = t.kind
                && !self.is_strict_descendant(target, t.source)
            {
                return Err(CompileError::at_transition(
                    K::LocalTargetNotStrictDescendant,
                    t.id,
                ));
            }
        }
        Ok(())
    }

    /// `descendant` is strictly below `ancestor`.
    pub(crate) fn is_strict_descendant(&self, descendant: StateId, ancestor: StateId) -> bool {
        let mut cursor = self.parent_of(descendant);
        while let Some(p) = cursor {
            if p == ancestor {
                return true;
            }
            cursor = self.parent_of(p);
        }
        false
    }

    fn build_candidates(&mut self) -> Result<(), CompileError> {
        let mut groups: BTreeMap<(StateId, EventKind), Vec<u32>> = BTreeMap::new();
        for (i, t) in self.transitions.iter().enumerate() {
            groups
                .entry((t.source, t.event))
                .or_default()
                .push(i as u32);
        }
        let mut events = Vec::new();
        for ((source, event), mut idxs) in groups {
            idxs.sort_by_key(|i| self.transitions[*i as usize].priority);
            let mut shadowed_by: Option<TransitionId> = None;
            for pair in idxs.windows(2) {
                let a = &self.transitions[pair[0] as usize];
                let b = &self.transitions[pair[1] as usize];
                if a.priority == b.priority {
                    return Err(CompileError::at_transition(
                        K::DuplicatePriority { other: a.id },
                        b.id,
                    ));
                }
            }
            for i in &idxs {
                let t = &self.transitions[*i as usize];
                if let Some(by) = shadowed_by {
                    return Err(CompileError::at_transition(
                        K::UnreachableTransition { shadowed_by: by },
                        t.id,
                    ));
                }
                if t.guard.is_none() {
                    shadowed_by = Some(t.id);
                }
            }
            events.push(event);
            self.candidates.insert(
                (source, event),
                idxs.iter()
                    .map(|i| self.transitions[*i as usize].id)
                    .collect(),
            );
        }
        events.sort();
        events.dedup();
        self.events = events;
        Ok(())
    }

    fn build_required_guards(&mut self) {
        let leaves: Vec<StateId> = self
            .states
            .iter()
            .filter(|s| s.initial.is_none())
            .map(|s| s.id)
            .collect();
        let events = self.events.clone();
        let mut table = BTreeMap::new();
        for leaf in leaves {
            for event in &events {
                let mut guards: Vec<GuardId> = Vec::new();
                let mut cursor = Some(leaf);
                while let Some(state) = cursor {
                    for id in self.candidates(state, *event) {
                        if let Some(g) = self.transition(*id).and_then(|t| t.guard)
                            && !guards.contains(&g)
                        {
                            guards.push(g);
                        }
                    }
                    cursor = self.parent_of(state);
                }
                if !guards.is_empty() {
                    table.insert((leaf, *event), guards);
                }
            }
        }
        self.required_guards = table;
    }

    /// Worst-case action count over Start, Stop, and every transition (contract §7.1).
    fn compute_max_plan_actions(&self) -> Result<u32, CompileError> {
        let max = self.limits.max_actions_per_plan;
        let mut worst = 0u32;

        // Start: entry actions along the root initial chain.
        let start = self.entry_actions(self.root_initial).len() as u32
            + self.initial_chain_actions(self.root_initial);
        worst = worst.max(start);
        if start > max {
            return Err(CompileError::new(K::ActionsPerPlanExceeded {
                required: start,
                max,
            }));
        }

        // Stop: deepest exit chain from any leaf to the root.
        let mut stop = 0u32;
        for s in &self.states {
            if s.parent.is_none() {
                stop = stop.max(s.exit.len() as u32 + self.max_exit_below(s.id));
            }
        }
        worst = worst.max(stop);
        if stop > max {
            return Err(CompileError::new(K::ActionsPerPlanExceeded {
                required: stop,
                max,
            }));
        }

        for t in &self.transitions {
            let required = self.worst_case_actions(t);
            worst = worst.max(required);
            if required > max {
                return Err(CompileError::at_transition(
                    K::ActionsPerPlanExceeded { required, max },
                    t.id,
                ));
            }
        }
        Ok(worst)
    }

    /// Max total exit actions of any active sub-path strictly below `state`.
    fn max_exit_below(&self, state: StateId) -> u32 {
        let mut best = 0u32;
        for s in &self.states {
            if s.parent == Some(state) {
                best = best.max(s.exit.len() as u32 + self.max_exit_below(s.id));
            }
        }
        best
    }

    /// Entry actions of the initial chain strictly below `state`.
    fn initial_chain_actions(&self, state: StateId) -> u32 {
        self.initial_chain(state)
            .map(|s| self.entry_actions(s).len() as u32)
            .sum()
    }

    /// Entry actions from the child of `domain` down to and including `target`.
    fn entry_actions_between(&self, domain: Option<StateId>, target: StateId) -> u32 {
        let mut total = 0u32;
        let mut cursor = Some(target);
        while let Some(s) = cursor
            && Some(s) != domain
        {
            total += self.entry_actions(s).len() as u32;
            cursor = self.parent_of(s);
        }
        total
    }

    /// Exit actions from `from` (inclusive) up to but excluding `domain`.
    fn exit_actions_between(&self, from: StateId, domain: Option<StateId>) -> u32 {
        let mut total = 0u32;
        let mut cursor = Some(from);
        while let Some(s) = cursor
            && Some(s) != domain
        {
            total += self.exit_actions(s).len() as u32;
            cursor = self.parent_of(s);
        }
        total
    }

    fn worst_case_actions(&self, t: &CompiledTransition) -> u32 {
        let own = t.actions.len() as u32;
        match t.kind {
            TransitionKind::Internal => own,
            TransitionKind::Local { target } => {
                self.max_exit_below(t.source)
                    + own
                    + self.entry_actions_between(Some(t.source), target)
                    + self.initial_chain_actions(target)
            }
            TransitionKind::External { target } => {
                let domain = self.strict_common_ancestor(t.source, target);
                self.max_exit_below(t.source)
                    + self.exit_actions_between(t.source, domain)
                    + own
                    + self.entry_actions_between(domain, target)
                    + self.initial_chain_actions(target)
            }
        }
    }

    /// Strict common ancestor (contract §4): LCA, then its parent if LCA equals either input.
    /// `None` = synthetic root.
    pub(crate) fn strict_common_ancestor(&self, a: StateId, b: StateId) -> Option<StateId> {
        let mut ancestors_a = Vec::new();
        let mut cursor = Some(a);
        while let Some(s) = cursor {
            ancestors_a.push(s);
            cursor = self.parent_of(s);
        }
        let mut cursor = Some(b);
        let mut lca = None;
        while let Some(s) = cursor {
            if ancestors_a.contains(&s) {
                lca = Some(s);
                break;
            }
            cursor = self.parent_of(s);
        }
        match lca {
            Some(l) if l == a || l == b => self.parent_of(l),
            other => other,
        }
    }

    fn compute_fingerprint(&self, spec: &DefinitionSpec) -> u64 {
        let mut h = Fnv1a::new();
        h.bytes(b"LUMIO-HFSM\0");
        h.u32(FORMAT_VERSION);
        h.u32(SEMANTICS_VERSION);
        h.u32(spec.initial.0);
        h.u32(spec.states.len() as u32);
        let mut states: Vec<&StateSpec> = spec.states.iter().collect();
        states.sort_by_key(|s| s.id);
        for s in states {
            h.u32(s.id.0);
            h.opt(s.parent.map(|p| p.0));
            h.opt(s.initial.map(|i| i.0));
            h.list(s.entry.iter().map(|a| a.0));
            h.list(s.exit.iter().map(|a| a.0));
        }
        h.u32(spec.transitions.len() as u32);
        let mut transitions: Vec<&TransitionSpec> = spec.transitions.iter().collect();
        transitions.sort_by_key(|t| (t.source, t.event, t.priority));
        for t in transitions {
            h.u32(t.id.0);
            h.u32(t.source.0);
            h.u32(t.event.0);
            h.u32(t.priority);
            h.opt(t.guard.map(|g| g.0));
            h.u32(t.kind.tag());
            h.opt(t.kind.target().map(|s| s.0));
            h.list(t.actions.iter().map(|a| a.0));
        }
        h.finish()
    }

    // ---- public queries ----

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn limits(&self) -> &HfsmLimits {
        &self.limits
    }

    pub fn state_count(&self) -> u32 {
        self.states.len() as u32
    }

    pub fn transition_count(&self) -> u32 {
        self.transitions.len() as u32
    }

    pub fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Worst-case `ActionRecord` count of any single plan from this definition.
    pub fn max_plan_actions(&self) -> u32 {
        self.max_plan_actions
    }

    pub fn root_initial(&self) -> StateId {
        self.root_initial
    }

    pub fn contains(&self, state: StateId) -> bool {
        self.index.contains_key(&state)
    }

    /// `None` for top-level states and for unknown ids (check `contains` first).
    pub fn parent_of(&self, state: StateId) -> Option<StateId> {
        self.state(state).and_then(|s| s.parent)
    }

    /// 1 for top-level states.
    pub fn depth_of(&self, state: StateId) -> Option<u32> {
        self.state(state).map(|s| s.depth)
    }

    pub fn is_leaf(&self, state: StateId) -> Option<bool> {
        self.state(state).map(|s| s.initial.is_none())
    }

    /// States entered when expanding `state` down to its leaf (excluding `state`).
    pub fn initial_chain(&self, state: StateId) -> impl Iterator<Item = StateId> + '_ {
        let mut cursor = self.state(state).and_then(|s| s.initial);
        std::iter::from_fn(move || {
            let next = cursor?;
            cursor = self.state(next).and_then(|s| s.initial);
            Some(next)
        })
    }

    pub fn entry_actions(&self, state: StateId) -> &[ActionId] {
        self.state(state).map(|s| s.entry.as_slice()).unwrap_or(&[])
    }

    pub fn exit_actions(&self, state: StateId) -> &[ActionId] {
        self.state(state).map(|s| s.exit.as_slice()).unwrap_or(&[])
    }

    pub fn name_of(&self, state: StateId) -> Option<&str> {
        self.state(state).and_then(|s| s.name.as_deref())
    }

    pub fn has_event(&self, event: EventKind) -> bool {
        self.events.binary_search(&event).is_ok()
    }

    /// Transitions declared on `state` for `event`, ascending priority.
    pub fn candidates(&self, state: StateId, event: EventKind) -> &[TransitionId] {
        self.candidates
            .get(&(state, event))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Guards the host must supply for `event` when `leaf` is active (bubbling order, deduped).
    pub fn required_guards(&self, leaf: StateId, event: EventKind) -> &[GuardId] {
        self.required_guards
            .get(&(leaf, event))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn transition(&self, id: TransitionId) -> Option<&CompiledTransition> {
        self.transition_index
            .get(&id)
            .map(|i| &self.transitions[*i as usize])
    }
}
