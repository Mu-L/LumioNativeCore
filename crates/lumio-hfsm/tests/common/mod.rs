//! Shared spec builders for lumio-hfsm integration tests.
#![allow(dead_code)]

use lumio_hfsm::*;

pub fn st(id: u32, parent: Option<u32>, initial: Option<u32>) -> StateSpec {
    StateSpec {
        id: StateId(id),
        parent: parent.map(StateId),
        initial: initial.map(StateId),
        entry: Vec::new(),
        exit: Vec::new(),
        name: None,
    }
}

pub fn st_actions(
    id: u32,
    parent: Option<u32>,
    initial: Option<u32>,
    entry: &[u32],
    exit: &[u32],
) -> StateSpec {
    let mut s = st(id, parent, initial);
    s.entry = entry.iter().map(|a| ActionId(*a)).collect();
    s.exit = exit.iter().map(|a| ActionId(*a)).collect();
    s
}

pub fn external(
    id: u32,
    source: u32,
    event: u32,
    priority: u32,
    guard: Option<u32>,
    target: u32,
) -> TransitionSpec {
    TransitionSpec {
        id: TransitionId(id),
        source: StateId(source),
        event: EventKind(event),
        priority,
        guard: guard.map(GuardId),
        kind: TransitionKind::External {
            target: StateId(target),
        },
        actions: Vec::new(),
    }
}

pub fn local(
    id: u32,
    source: u32,
    event: u32,
    priority: u32,
    guard: Option<u32>,
    target: u32,
) -> TransitionSpec {
    TransitionSpec {
        kind: TransitionKind::Local {
            target: StateId(target),
        },
        ..external(id, source, event, priority, guard, target)
    }
}

pub fn internal(
    id: u32,
    source: u32,
    event: u32,
    priority: u32,
    guard: Option<u32>,
) -> TransitionSpec {
    TransitionSpec {
        kind: TransitionKind::Internal,
        ..external(id, source, event, priority, guard, source)
    }
}

pub fn spec(
    initial: u32,
    states: Vec<StateSpec>,
    transitions: Vec<TransitionSpec>,
) -> DefinitionSpec {
    DefinitionSpec {
        format_version: FORMAT_VERSION,
        initial: StateId(initial),
        states,
        transitions,
    }
}

/// Root └─ A(initial A1) ├─ A1 └─ A2   (contract §4.1)
pub fn a_tree() -> Vec<StateSpec> {
    vec![
        st(1, None, Some(2)),
        st(2, Some(1), None),
        st(3, Some(1), None),
    ]
}

/// v0.1 §15.1 content-loading workflow.
pub fn workflow_spec() -> DefinitionSpec {
    spec(
        1,
        vec![
            st(1, None, Some(2)),
            st(2, Some(1), None),
            st_actions(3, Some(1), Some(4), &[], &[103]),
            st_actions(4, Some(3), None, &[101], &[]),
            st_actions(5, Some(3), None, &[102], &[]),
            st(6, Some(1), None),
            st(7, Some(1), None),
            st(8, Some(1), None),
        ],
        vec![
            external(1, 2, 1, 0, Some(1), 3),
            external(2, 4, 2, 0, None, 5),
            external(3, 4, 3, 0, None, 7),
            external(4, 5, 4, 0, None, 6),
            external(5, 5, 5, 0, None, 7),
            external(6, 3, 6, 0, None, 8),
            local(7, 1, 7, 0, None, 2),
        ],
    )
}

pub fn compile_ok(spec: &DefinitionSpec) -> CompiledDefinition {
    compile(spec, &HfsmLimits::DEFAULT).expect("spec should compile")
}

pub fn compile_err(spec: &DefinitionSpec) -> CompileError {
    compile(spec, &HfsmLimits::DEFAULT).expect_err("spec should be rejected")
}

// ---- evaluator helpers (H03) ----

/// §4.1 A-tree with distinguishable actions: A entry 10 / exit 11, A1 20/21, A2 30/31.
pub fn a_tree_actions() -> Vec<StateSpec> {
    vec![
        st_actions(1, None, Some(2), &[10], &[11]),
        st_actions(2, Some(1), None, &[20], &[21]),
        st_actions(3, Some(1), None, &[30], &[31]),
    ]
}

pub fn with_action(mut t: TransitionSpec, action: u32) -> TransitionSpec {
    t.actions.push(ActionId(action));
    t
}

pub fn snap(
    def: &CompiledDefinition,
    key: u64,
    epoch: u64,
    step: u64,
    next: u64,
    path: &[(u32, u64)],
) -> Snapshot {
    Snapshot {
        fingerprint: def.fingerprint(),
        machine_key: MachineKey(key),
        epoch: MachineEpoch(epoch),
        step_seq: StepSeq(step),
        next_activation_seq: ActivationSeq(next),
        lifecycle: if path.is_empty() {
            Lifecycle::NotStarted
        } else {
            Lifecycle::Running
        },
        active_path: path
            .iter()
            .map(|(s, seq)| ActiveState {
                state: StateId(*s),
                activation_seq: ActivationSeq(*seq),
            })
            .collect(),
    }
}

pub fn path(path: &[(u32, u64)]) -> Vec<ActiveState> {
    path.iter()
        .map(|(s, seq)| ActiveState {
            state: StateId(*s),
            activation_seq: ActivationSeq(*seq),
        })
        .collect()
}

pub fn start_item<'a>(def: &'a CompiledDefinition, key: u64, epoch: u64) -> BatchItem<'a> {
    BatchItem {
        machine_key: MachineKey(key),
        definition: def,
        kind: ItemKind::Start {
            epoch: MachineEpoch(epoch),
        },
        snapshot: None,
        guards: GuardFrame::new(&[]),
        scope: None,
    }
}

pub fn event_item<'a>(
    def: &'a CompiledDefinition,
    snapshot: &'a Snapshot,
    event: u32,
    guards: &'a [(GuardId, bool)],
) -> BatchItem<'a> {
    BatchItem {
        machine_key: snapshot.machine_key,
        definition: def,
        kind: ItemKind::Event {
            event: EventKind(event),
        },
        snapshot: Some(snapshot),
        guards: GuardFrame::new(guards),
        scope: None,
    }
}

pub fn stop_item<'a>(def: &'a CompiledDefinition, snapshot: &'a Snapshot) -> BatchItem<'a> {
    BatchItem {
        machine_key: snapshot.machine_key,
        definition: def,
        kind: ItemKind::Stop,
        snapshot: Some(snapshot),
        guards: GuardFrame::new(&[]),
        scope: None,
    }
}

#[derive(Debug)]
pub struct Run {
    pub status: BatchStatus,
    pub items: Vec<ItemPlan>,
    pub paths: Vec<ActiveState>,
    pub actions: Vec<ActionRecord>,
}

impl Run {
    pub fn plan(&self, i: usize) -> &ItemPlan {
        &self.items[i]
    }

    pub fn path_of(&self, i: usize) -> &[ActiveState] {
        let p = &self.items[i];
        &self.paths[p.path_start as usize..(p.path_start + p.path_len) as usize]
    }

    pub fn actions_of(&self, i: usize) -> &[ActionRecord] {
        let p = &self.items[i];
        &self.actions[p.action_start as usize..(p.action_start + p.action_len) as usize]
    }

    pub fn next_snapshot(&self, i: usize) -> Snapshot {
        Snapshot::from_plan(&self.items[i], &self.paths).expect("item has a next snapshot")
    }

    /// (phase, action, state, activation_seq) tuples for compact assertions.
    pub fn records(&self, i: usize) -> Vec<(ActionPhase, u32, u32, u64)> {
        self.actions_of(i)
            .iter()
            .map(|r| (r.phase, r.action.0, r.state.0, r.activation_seq.0))
            .collect()
    }
}

/// Evaluate with generously sized caller buffers.
pub fn run(items: &[BatchItem<'_>]) -> Result<Run, HfsmError> {
    run_with(items, &HfsmLimits::DEFAULT, 4096, 4096)
}

pub fn run_with(
    items: &[BatchItem<'_>],
    limits: &HfsmLimits,
    path_cap: usize,
    action_cap: usize,
) -> Result<Run, HfsmError> {
    let mut plans = vec![ItemPlan::EMPTY; items.len()];
    let mut paths = vec![ActiveState::EMPTY; path_cap];
    let mut actions = vec![ActionRecord::EMPTY; action_cap];
    let mut scratch = Scratch::new(limits);
    let status = evaluate_batch(
        items,
        limits,
        PlanOutput {
            items: &mut plans,
            paths: &mut paths,
            actions: &mut actions,
        },
        &mut scratch,
    )?;
    paths.truncate(status.paths_written as usize);
    actions.truncate(status.actions_written as usize);
    Ok(Run {
        status,
        items: plans,
        paths,
        actions,
    })
}

pub fn started(def: &CompiledDefinition, key: u64, epoch: u64) -> Snapshot {
    let r = run(&[start_item(def, key, epoch)]).expect("start batch");
    assert_eq!(r.plan(0).outcome, Outcome::Started);
    r.next_snapshot(0)
}
