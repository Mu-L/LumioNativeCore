//! Contract §11 group P: invariants over randomly generated graphs and event sequences.
//!
//! Deterministic LCG, no external crates. A failure prints the seed so it can be replayed.

mod common;

use std::collections::BTreeMap;

use common::*;
use lumio_hfsm::*;

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6364136223846793005).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    fn bool(&mut self) -> bool {
        self.next().is_multiple_of(2)
    }
}

const EVENTS: u32 = 4;
const GUARDS: u32 = 3;

/// Random well-formed graph: a tree of `n` states plus guarded transitions.
fn random_spec(rng: &mut Lcg, n: usize) -> DefinitionSpec {
    let mut parents: Vec<Option<u32>> = vec![None];
    for i in 1..n {
        // Attach to an existing state (or the synthetic root) so the tree stays acyclic.
        parents.push(if rng.bool() {
            Some(rng.below(i) as u32 + 1)
        } else {
            None
        });
    }
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (i, p) in parents.iter().enumerate() {
        if let Some(p) = p {
            children.entry(*p).or_default().push(i as u32 + 1);
        }
    }
    let states: Vec<StateSpec> = (0..n)
        .map(|i| {
            let id = i as u32 + 1;
            let kids = children.get(&id);
            let initial = kids.map(|k| k[rng.below(k.len())]);
            let mut s = st_actions(id, parents[i], initial, &[id * 100 + 1], &[id * 100 + 2]);
            if rng.bool() {
                s.entry.push(ActionId(id * 100 + 3));
            }
            s
        })
        .collect();

    let top: Vec<u32> = (0..n)
        .filter(|i| parents[*i].is_none())
        .map(|i| i as u32 + 1)
        .collect();
    let root_initial = top[rng.below(top.len())];

    let depth_of = |mut id: u32| {
        let mut d = 1;
        while let Some(p) = parents[id as usize - 1] {
            d += 1;
            id = p;
        }
        d
    };
    let is_descendant = |mut d: u32, a: u32| {
        while let Some(p) = parents[d as usize - 1] {
            if p == a {
                return true;
            }
            d = p;
        }
        false
    };

    let mut used: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut transitions = Vec::new();
    for _ in 0..n * 2 {
        let source = rng.below(n) as u32 + 1;
        let event = rng.below(EVENTS as usize) as u32 + 1;
        let slot = used.entry((source, event)).or_insert(0);
        let priority = *slot;
        *slot += 1;
        let id = TransitionId(transitions.len() as u32 + 1);
        // Always guarded: a guardless transition would shadow later priorities.
        let guard = Some(GuardId(rng.below(GUARDS as usize) as u32 + 1));
        let descendants: Vec<u32> = (1..=n as u32)
            .filter(|d| is_descendant(*d, source))
            .collect();
        let kind = match rng.below(3) {
            0 => TransitionKind::Internal,
            1 if !descendants.is_empty() => TransitionKind::Local {
                target: StateId(descendants[rng.below(descendants.len())]),
            },
            _ => TransitionKind::External {
                target: StateId(rng.below(n) as u32 + 1),
            },
        };
        transitions.push(TransitionSpec {
            id,
            source: StateId(source),
            event: EventKind(event),
            priority,
            guard,
            kind,
            actions: vec![ActionId(id.0 * 10)],
        });
    }
    // Keep generated graphs shallow enough for the default depth limit.
    assert!((1..=n as u32).all(|id| depth_of(id) <= HfsmLimits::DEFAULT.max_depth));
    spec(root_initial, states, transitions)
}

fn random_frame(
    def: &CompiledDefinition,
    leaf: StateId,
    event: EventKind,
    rng: &mut Lcg,
) -> Vec<(GuardId, bool)> {
    def.required_guards(leaf, event)
        .iter()
        .map(|g| (*g, rng.bool()))
        .collect()
}

/// Exits unwind leaf-first (non-increasing depth); entries nest outer-first (non-decreasing).
fn assert_depth_order(def: &CompiledDefinition, records: &[ActionRecord], seed: u64) {
    let depth = |r: &ActionRecord| def.depth_of(r.state).expect("recorded state exists");
    let exits: Vec<u32> = records
        .iter()
        .filter(|r| r.phase == ActionPhase::Exit)
        .map(depth)
        .collect();
    assert!(
        exits.windows(2).all(|w| w[0] >= w[1]),
        "seed {seed}: exits must run leaf-first, got depths {exits:?}"
    );
    let enters: Vec<u32> = records
        .iter()
        .filter(|r| r.phase == ActionPhase::Enter)
        .map(depth)
        .collect();
    assert!(
        enters.windows(2).all(|w| w[0] <= w[1]),
        "seed {seed}: entries must run outer-first, got depths {enters:?}"
    );
}

/// Every exit precedes every enter; transition actions sit between them.
fn assert_phase_order(records: &[ActionRecord], seed: u64) {
    let mut stage = 0u8;
    for r in records {
        let s = match r.phase {
            ActionPhase::Exit => 0,
            ActionPhase::Transition => 1,
            ActionPhase::Enter => 2,
        };
        assert!(s >= stage, "seed {seed}: phases out of order: {records:?}");
        stage = s;
    }
    assert!(
        records
            .iter()
            .map(|r| r.ordinal)
            .eq(0..records.len() as u32),
        "seed {seed}: ordinals must be dense and 0-based"
    );
}

/// Independent restatement of contract §4: LCA, stepped up when it equals either endpoint.
fn strict_common_ancestor(def: &CompiledDefinition, a: StateId, b: StateId) -> Option<StateId> {
    let chain = |mut s: StateId| {
        let mut out = vec![s];
        while let Some(p) = def.parent_of(s) {
            out.push(p);
            s = p;
        }
        out
    };
    let ca = chain(a);
    let lca = chain(b).into_iter().find(|s| ca.contains(s));
    match lca {
        Some(l) if l == a || l == b => def.parent_of(l),
        other => other,
    }
}

#[test]
fn random_graphs_preserve_every_documented_invariant() {
    for seed in 0..400u64 {
        let mut rng = Lcg::new(seed);
        let n = 2 + rng.below(7);
        let spec = random_spec(&mut rng, n);
        let def = match compile(&spec, &HfsmLimits::DEFAULT) {
            Ok(d) => d,
            // Generation only produces well-formed graphs; anything else is a generator bug.
            Err(e) => panic!("seed {seed}: generated spec rejected: {e:?}"),
        };

        let r = run(&[start_item(&def, 1, 1)]).expect("start batch");
        assert_eq!(r.plan(0).outcome, Outcome::Started, "seed {seed}");
        assert_phase_order(r.actions_of(0), seed);
        assert_depth_order(&def, r.actions_of(0), seed);
        let mut snapshot = r.next_snapshot(0);
        assert_eq!(
            snapshot.validate(&def),
            Ok(()),
            "seed {seed}: start path invalid"
        );
        assert!(
            r.actions_of(0)
                .iter()
                .all(|a| a.phase == ActionPhase::Enter),
            "seed {seed}: start emits only Enter"
        );

        for _ in 0..24 {
            let leaf = snapshot.leaf().expect("running snapshot has a leaf");
            let event = EventKind(rng.below(EVENTS as usize) as u32 + 1);
            let frame = random_frame(&def, leaf, event, &mut rng);
            let before = snapshot.clone();

            let r = run(&[event_item(&def, &snapshot, event.0, &frame)]).expect("event batch");
            // P: pure computation does not touch its inputs.
            assert_eq!(snapshot, before, "seed {seed}: input snapshot mutated");

            let plan = *r.plan(0);
            let records = r.actions_of(0).to_vec();
            assert_phase_order(&records, seed);
            assert_depth_order(&def, &records, seed);

            match plan.outcome {
                Outcome::Rejected(ItemError::UnknownEvent) => {
                    assert!(plan.next.is_none(), "seed {seed}");
                    continue;
                }
                Outcome::Rejected(e) => panic!("seed {seed}: unexpected rejection {e:?}"),
                _ => {}
            }

            let next = r.next_snapshot(0);
            // P: the configuration stays one top-level→leaf chain.
            assert_eq!(next.validate(&def), Ok(()), "seed {seed}: {next:?}");
            // P: every legal event advances step_seq exactly once.
            assert_eq!(next.step_seq.0, before.step_seq.0 + 1, "seed {seed}");
            assert_eq!(next.epoch, before.epoch, "seed {seed}");
            assert_eq!(next.machine_key, before.machine_key, "seed {seed}");
            assert!(
                next.next_activation_seq >= before.next_activation_seq,
                "seed {seed}"
            );

            let old: Vec<(StateId, ActivationSeq)> = before
                .active_path
                .iter()
                .map(|a| (a.state, a.activation_seq))
                .collect();
            let new: Vec<(StateId, ActivationSeq)> = next
                .active_path
                .iter()
                .map(|a| (a.state, a.activation_seq))
                .collect();
            let exited: Vec<(StateId, ActivationSeq)> = records
                .iter()
                .filter(|r| r.phase == ActionPhase::Exit)
                .map(|r| (r.state, r.activation_seq))
                .collect();
            let entered: Vec<(StateId, ActivationSeq)> = records
                .iter()
                .filter(|r| r.phase == ActionPhase::Enter)
                .map(|r| (r.state, r.activation_seq))
                .collect();

            match plan.outcome {
                Outcome::Unhandled => {
                    // P: Unhandled changes nothing but step_seq.
                    assert_eq!(new, old, "seed {seed}: Unhandled moved the path");
                    assert!(records.is_empty(), "seed {seed}");
                    assert_eq!(
                        next.next_activation_seq, before.next_activation_seq,
                        "seed {seed}"
                    );
                }
                Outcome::Transitioned { transition } => {
                    let t = def
                        .transition(transition)
                        .expect("selected transition exists");
                    if matches!(t.kind, TransitionKind::Internal) {
                        // P: Internal keeps the whole path and every generation.
                        assert_eq!(new, old, "seed {seed}: Internal moved the path");
                        assert!(exited.is_empty() && entered.is_empty(), "seed {seed}");
                        assert_eq!(
                            next.next_activation_seq, before.next_activation_seq,
                            "seed {seed}"
                        );
                    } else {
                        // P: the retained prefix is exactly what the contract's transition
                        // domain says it is — derived from the definition, not the evaluator.
                        let domain = match t.kind {
                            TransitionKind::Local { .. } => Some(t.source),
                            TransitionKind::External { target } => {
                                strict_common_ancestor(&def, t.source, target)
                            }
                            TransitionKind::Internal => unreachable!(),
                        };
                        let keep = match domain {
                            None => 0,
                            Some(d) => old
                                .iter()
                                .position(|(s, _)| *s == d)
                                .map(|i| i + 1)
                                .unwrap_or_else(|| panic!("seed {seed}: domain {d:?} not on path")),
                        };
                        assert_eq!(
                            &new[..keep],
                            &old[..keep],
                            "seed {seed}: retained prefix must survive untouched ({:?})",
                            t.kind
                        );
                        assert!(
                            new.len() > keep,
                            "seed {seed}: something must be entered below the domain"
                        );
                        assert!(
                            new[keep..]
                                .iter()
                                .all(|(_, seq)| *seq >= before.next_activation_seq),
                            "seed {seed}: states below the domain must be freshly entered"
                        );

                        // P: retained states keep their generation; re-entered states get a new one.
                        for kept in new.iter().filter(|s| old.contains(s)) {
                            assert!(
                                !exited.contains(kept),
                                "seed {seed}: retained {kept:?} was exited"
                            );
                            assert!(
                                !entered.contains(kept),
                                "seed {seed}: retained {kept:?} was re-entered"
                            );
                        }
                        for fresh in new.iter().filter(|s| !old.contains(s)) {
                            assert!(
                                fresh.1 >= before.next_activation_seq,
                                "seed {seed}: {fresh:?} reused an old generation"
                            );
                            assert!(
                                entered.contains(fresh),
                                "seed {seed}: {fresh:?} never entered"
                            );
                        }
                        // Everything dropped from the path was exited exactly once.
                        for gone in old.iter().filter(|s| !new.contains(s)) {
                            assert_eq!(
                                exited.iter().filter(|e| *e == gone).count(),
                                usize::from(!def.exit_actions(gone.0).is_empty()),
                                "seed {seed}: {gone:?} exit records"
                            );
                        }
                    }
                }
                other => panic!("seed {seed}: unexpected outcome {other:?}"),
            }

            snapshot = next;
        }

        // P: Stop unwinds the whole path, leaf first, and leaves no active state.
        let r = run(&[stop_item(&def, &snapshot)]).expect("stop batch");
        assert_eq!(r.plan(0).outcome, Outcome::Stopped, "seed {seed}");
        assert!(
            r.actions_of(0).iter().all(|a| a.phase == ActionPhase::Exit),
            "seed {seed}"
        );
        assert_depth_order(&def, r.actions_of(0), seed);
        let stopped = r.next_snapshot(0);
        assert!(stopped.active_path.is_empty(), "seed {seed}");
        assert_eq!(stopped.lifecycle, Lifecycle::Stopped, "seed {seed}");
        assert_eq!(stopped.validate(&def), Ok(()), "seed {seed}");
    }
}

#[test]
fn undersized_buffers_never_write_and_retries_match_exactly() {
    // P: BufferTooSmall leaves caller buffers untouched; the retry is bitwise identical.
    for seed in 0..120u64 {
        let mut rng = Lcg::new(seed ^ 0xa5a5);
        let n = 3 + rng.below(5);
        let spec = random_spec(&mut rng, n);
        let def = compile(&spec, &HfsmLimits::DEFAULT).expect("generated spec compiles");
        let a = started(&def, 1, 1);
        let b = started(&def, 2, 1);
        let event = EventKind(rng.below(EVENTS as usize) as u32 + 1);
        let fa = random_frame(&def, a.leaf().unwrap(), event, &mut rng);
        let fb = random_frame(&def, b.leaf().unwrap(), event, &mut rng);
        let items = [
            event_item(&def, &a, event.0, &fa),
            event_item(&def, &b, event.0, &fb),
        ];

        let full = run(&items).expect("generous buffers");
        let need_paths = full.status.paths_written as usize;
        let need_actions = full.status.actions_written as usize;

        if need_paths > 0 {
            let mut plans = vec![ItemPlan::EMPTY; items.len()];
            let mut paths = vec![ActiveState::EMPTY; need_paths - 1];
            let mut actions = vec![ActionRecord::EMPTY; need_actions];
            let mut scratch = Scratch::new(&HfsmLimits::DEFAULT);
            let err = evaluate_batch(
                &items,
                &HfsmLimits::DEFAULT,
                PlanOutput {
                    items: &mut plans,
                    paths: &mut paths,
                    actions: &mut actions,
                },
                &mut scratch,
            )
            .expect_err("one path short");
            assert_eq!(
                err,
                HfsmError::BufferTooSmall {
                    required_items: items.len() as u32,
                    required_paths: need_paths as u32,
                    required_actions: need_actions as u32,
                },
                "seed {seed}"
            );
            assert!(plans.iter().all(|p| *p == ItemPlan::EMPTY), "seed {seed}");
            assert!(
                paths.iter().all(|p| *p == ActiveState::EMPTY),
                "seed {seed}"
            );
            assert!(
                actions.iter().all(|a| *a == ActionRecord::EMPTY),
                "seed {seed}"
            );
        }

        let exact =
            run_with(&items, &HfsmLimits::DEFAULT, need_paths, need_actions).expect("exact fit");
        assert_eq!(exact.status, full.status, "seed {seed}");
        assert_eq!(exact.items, full.items, "seed {seed}");
        assert_eq!(exact.paths, full.paths, "seed {seed}");
        assert_eq!(exact.actions, full.actions, "seed {seed}");
    }
}
