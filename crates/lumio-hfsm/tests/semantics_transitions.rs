//! T02–T08, T11: the six §4.1 vectors, bubbling, child priority, Unhandled, UnknownEvent.

mod common;

use common::*;
use lumio_hfsm::*;

/// A-tree with actions and six transitions, one per event 1..=6 (§4.1 rows).
fn six_vectors() -> CompiledDefinition {
    compile_ok(&spec(
        1,
        a_tree_actions(),
        vec![
            with_action(external(1, 2, 1, 0, None, 3), 99), // A1 → A2 External
            with_action(external(2, 1, 2, 0, None, 3), 99), // A  → A2 External
            with_action(local(3, 1, 3, 0, None, 3), 99),    // A  → A2 Local
            with_action(external(4, 2, 4, 0, None, 2), 99), // A1 → A1 External
            with_action(internal(5, 2, 5, 0, None), 99),    // A1 Internal
            with_action(external(6, 2, 6, 0, None, 1), 99), // A1 → A  External
        ],
    ))
}

fn a_a1() -> (CompiledDefinition, Snapshot) {
    let def = six_vectors();
    let s = started(&def, 7, 1);
    assert_eq!(s.active_path, path(&[(1, 1), (2, 2)]));
    assert_eq!(s.next_activation_seq, ActivationSeq(3));
    (def, s)
}

fn fire(def: &CompiledDefinition, s: &Snapshot, event: u32) -> Run {
    run(&[event_item(def, s, event, &[])]).unwrap()
}

#[test]
fn t02_sibling_external_keeps_parent() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 1);
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(1)
        }
    );
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Exit, 21, 2, 2),
            (ActionPhase::Transition, 99, 2, 2),
            (ActionPhase::Enter, 30, 3, 3),
        ]
    );
    let n = r.next_snapshot(0);
    assert_eq!(n.active_path, path(&[(1, 1), (3, 3)]));
    assert_eq!(n.next_activation_seq, ActivationSeq(4));
    assert_eq!(n.step_seq, StepSeq(2));
}

#[test]
fn t03_parent_declared_external_to_descendant_reenters_parent() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 2);
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Exit, 21, 2, 2),
            (ActionPhase::Exit, 11, 1, 1),
            (ActionPhase::Transition, 99, 1, 1),
            (ActionPhase::Enter, 10, 1, 3),
            (ActionPhase::Enter, 30, 3, 4),
        ]
    );
    let n = r.next_snapshot(0);
    assert_eq!(n.active_path, path(&[(1, 3), (3, 4)]));
    assert_eq!(n.next_activation_seq, ActivationSeq(5));
}

#[test]
fn t04_parent_declared_local_keeps_source_generation() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 3);
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Exit, 21, 2, 2),
            (ActionPhase::Transition, 99, 1, 1),
            (ActionPhase::Enter, 30, 3, 3),
        ]
    );
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 1), (3, 3)]));
}

#[test]
fn t05_external_self_transition_exits_and_reenters_with_new_seq() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 4);
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Exit, 21, 2, 2),
            (ActionPhase::Transition, 99, 2, 2),
            (ActionPhase::Enter, 20, 2, 3),
        ]
    );
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 1), (2, 3)]));
}

#[test]
fn t06_internal_only_emits_transition_actions() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 5);
    assert_eq!(r.records(0), vec![(ActionPhase::Transition, 99, 2, 2)]);
    let n = r.next_snapshot(0);
    assert_eq!(n.active_path, s.active_path);
    assert_eq!(n.next_activation_seq, s.next_activation_seq);
    assert_eq!(n.step_seq, StepSeq(s.step_seq.0 + 1));
}

#[test]
fn t06b_external_to_ancestor_reenters_ancestor_and_its_initial() {
    let (def, s) = a_a1();
    let r = fire(&def, &s, 6);
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Exit, 21, 2, 2),
            (ActionPhase::Exit, 11, 1, 1),
            (ActionPhase::Transition, 99, 2, 2),
            (ActionPhase::Enter, 10, 1, 3),
            (ActionPhase::Enter, 20, 2, 4),
        ]
    );
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 3), (2, 4)]));
}

#[test]
fn t07_child_guard_false_bubbles_to_parent() {
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 7, 0, Some(1), 3), // A1 --7[g1]--> A2
            external(2, 1, 7, 0, Some(2), 3), // A  --7[g2]--> A2
        ],
    ));
    let s = started(&def, 7, 1);
    let guards = [(GuardId(1), false), (GuardId(2), true)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(2)
        }
    );
    // Parent-declared External: A is re-entered.
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 3), (3, 4)]));
}

#[test]
fn t08_child_wins_even_with_larger_priority_number() {
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 7, 10, Some(1), 3), // A1 p10
            external(2, 1, 7, 0, Some(2), 3),  // A  p0
        ],
    ));
    let s = started(&def, 7, 1);
    let guards = [(GuardId(1), true), (GuardId(2), true)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(1)
        }
    );
}

#[test]
fn priority_order_within_one_level_picks_first_true_guard() {
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 7, 5, Some(1), 3),
            external(2, 2, 7, 1, Some(2), 3),
            external(3, 2, 7, 9, None, 3),
        ],
    ));
    let s = started(&def, 7, 1);
    let guards = [(GuardId(1), true), (GuardId(2), false)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(1)
        }
    );
    let guards = [(GuardId(1), false), (GuardId(2), false)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(3)
        }
    );
}

#[test]
fn t11_unhandled_advances_step_seq_only() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    // Event 2 (LoadSucceeded) is declared on Loading only; Idle → Unhandled.
    let r = run(&[event_item(&def, &idle, 2, &[])]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Unhandled);
    assert_eq!(r.plan(0).action_len, 0);
    let n = r.next_snapshot(0);
    assert_eq!(n.active_path, idle.active_path);
    assert_eq!(n.next_activation_seq, idle.next_activation_seq);
    assert_eq!(n.step_seq, StepSeq(idle.step_seq.0 + 1));

    // All guards false is also Unhandled.
    let r = run(&[event_item(&def, &idle, 1, &[(GuardId(1), false)])]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Unhandled);
}

#[test]
fn unknown_event_is_an_input_error_not_unhandled() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[event_item(&def, &idle, 42, &[])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::UnknownEvent)
    );
    assert!(r.plan(0).next.is_none());
}

#[test]
fn workflow_full_path_loading_to_executing_keeps_running_scope() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[event_item(&def, &idle, 1, &[(GuardId(1), true)])]).unwrap();
    let loading = r.next_snapshot(0);
    assert_eq!(loading.active_path, path(&[(1, 1), (3, 3), (4, 4)]));
    assert_eq!(r.records(0), vec![(ActionPhase::Enter, 101, 4, 4)]);

    let r = run(&[event_item(&def, &loading, 2, &[])]).unwrap();
    let executing = r.next_snapshot(0);
    assert_eq!(executing.active_path, path(&[(1, 1), (3, 3), (5, 5)]));
    assert_eq!(r.records(0), vec![(ActionPhase::Enter, 102, 5, 5)]);

    // Cancel declared on Running: exits Executing, Running(103); enters Cancelled.
    let r = run(&[event_item(&def, &executing, 6, &[])]).unwrap();
    assert_eq!(r.records(0), vec![(ActionPhase::Exit, 103, 3, 3)]);
    let cancelled = r.next_snapshot(0);
    assert_eq!(cancelled.active_path, path(&[(1, 1), (8, 6)]));

    // Reset is Local on Workflow: Workflow keeps #1, enters Idle.
    let r = run(&[event_item(&def, &cancelled, 7, &[])]).unwrap();
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 1), (2, 7)]));
}
