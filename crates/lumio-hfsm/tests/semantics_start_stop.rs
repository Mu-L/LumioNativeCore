//! T01 / T12: Start expansion, Stop exit chain, lifecycle violations, restart.

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn start_enters_initial_chain_outer_to_inner_with_fresh_activation_seqs() {
    // T01: Workflow(initial Idle); Running(initial Loading) — start goes Workflow → Idle.
    let mut s = workflow_spec();
    s.states[0].entry = vec![ActionId(1)];
    s.states[1].entry = vec![ActionId(2), ActionId(3)];
    let def = compile_ok(&s);

    let r = run(&[start_item(&def, 7, 1)]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Started);
    assert_eq!(r.path_of(0), path(&[(1, 1), (2, 2)]));
    assert_eq!(
        r.records(0),
        vec![
            (ActionPhase::Enter, 1, 1, 1),
            (ActionPhase::Enter, 2, 2, 2),
            (ActionPhase::Enter, 3, 2, 2),
        ]
    );
    let next = r.next_snapshot(0);
    assert_eq!(next.fingerprint, def.fingerprint());
    assert_eq!(next.machine_key, MachineKey(7));
    assert_eq!(next.epoch, MachineEpoch(1));
    assert_eq!(next.lifecycle, Lifecycle::Running);
    assert_eq!(next.step_seq, StepSeq(1));
    assert_eq!(next.next_activation_seq, ActivationSeq(3));
    assert_eq!(next.leaf(), Some(StateId(2)));

    // Ordinals are 0..n within the item.
    let ordinals: Vec<u32> = r.actions_of(0).iter().map(|a| a.ordinal).collect();
    assert_eq!(ordinals, vec![0, 1, 2]);
    assert!(r.actions_of(0).iter().all(|a| a.item_index == 0));
}

#[test]
fn start_on_running_snapshot_is_a_lifecycle_violation() {
    // T12
    let def = compile_ok(&workflow_spec());
    let running = started(&def, 7, 1);
    let mut item = start_item(&def, 7, 2);
    item.snapshot = Some(&running);
    let r = run(&[item]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::LifecycleViolation)
    );
    assert!(r.plan(0).next.is_none());
    assert_eq!(r.plan(0).action_len, 0);
    assert_eq!(r.plan(0).path_len, 0);
}

#[test]
fn stop_exits_leaf_to_root_and_rejects_later_events() {
    // T12: Running/Loading → Stop exits Loading then Running(exit 103) then Workflow.
    let mut s = workflow_spec();
    s.states[0].exit = vec![ActionId(9)];
    let def = compile_ok(&s);
    let idle = started(&def, 7, 1);
    let guards = [(GuardId(1), true)];
    let r = run(&[event_item(&def, &idle, 1, &guards)]).unwrap();
    let loading = r.next_snapshot(0);
    assert_eq!(loading.active_path, path(&[(1, 1), (3, 3), (4, 4)]));

    let r = run(&[stop_item(&def, &loading)]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Stopped);
    assert_eq!(
        r.records(0),
        vec![(ActionPhase::Exit, 103, 3, 3), (ActionPhase::Exit, 9, 1, 1)]
    );
    let stopped = r.next_snapshot(0);
    assert_eq!(stopped.lifecycle, Lifecycle::Stopped);
    assert!(stopped.active_path.is_empty());
    assert_eq!(stopped.step_seq, StepSeq(loading.step_seq.0 + 1));
    assert_eq!(stopped.next_activation_seq, loading.next_activation_seq);

    let r = run(&[event_item(&def, &stopped, 2, &[])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::LifecycleViolation)
    );
    let r = run(&[stop_item(&def, &stopped)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::LifecycleViolation)
    );
}

#[test]
fn restart_after_stop_continues_counters_and_takes_new_epoch() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[stop_item(&def, &idle)]).unwrap();
    let stopped = r.next_snapshot(0);

    let mut restart = start_item(&def, 7, 2);
    restart.snapshot = Some(&stopped);
    let r = run(&[restart]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Started);
    let again = r.next_snapshot(0);
    assert_eq!(again.epoch, MachineEpoch(2));
    assert_eq!(again.active_path, path(&[(1, 3), (2, 4)]));
    assert_eq!(again.step_seq, StepSeq(3));
    assert_eq!(again.next_activation_seq, ActivationSeq(5));
}

#[test]
fn start_with_not_started_snapshot_uses_its_counters() {
    let def = compile_ok(&workflow_spec());
    let fresh = snap(&def, 7, 0, 0, 1, &[]);
    let mut item = start_item(&def, 7, 5);
    item.snapshot = Some(&fresh);
    let r = run(&[item]).unwrap();
    assert_eq!(r.plan(0).outcome, Outcome::Started);
    assert_eq!(r.next_snapshot(0).epoch, MachineEpoch(5));
    assert_eq!(r.next_snapshot(0).active_path, path(&[(1, 1), (2, 2)]));
}
