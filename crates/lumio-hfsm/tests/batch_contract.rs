//! T13 / T15 and batch-level rejections (contract §6).

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn t13_buffer_too_small_writes_nothing_and_retry_is_bitwise_identical() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let guards = [(GuardId(1), true)];
    let items = [event_item(&def, &idle, 1, &guards), start_item(&def, 8, 1)];
    // Needs 2 items, 3 + 2 = 5 paths, 1 + 0 = 1 actions.
    let err = run_with(&items, &HfsmLimits::DEFAULT, 4, 8).unwrap_err();
    assert_eq!(
        err,
        HfsmError::BufferTooSmall {
            required_items: 2,
            required_paths: 5,
            required_actions: 1
        }
    );
    let err = run_with(&items, &HfsmLimits::DEFAULT, 8, 0).unwrap_err();
    assert_eq!(
        err,
        HfsmError::BufferTooSmall {
            required_items: 2,
            required_paths: 5,
            required_actions: 1
        }
    );

    // Too-small item slice is also BufferTooSmall.
    let mut one_plan = vec![ItemPlan::EMPTY; 1];
    let mut paths = vec![ActiveState::EMPTY; 8];
    let mut actions = vec![ActionRecord::EMPTY; 8];
    let mut scratch = Scratch::new(&HfsmLimits::DEFAULT);
    let err = evaluate_batch(
        &items,
        &HfsmLimits::DEFAULT,
        PlanOutput {
            items: &mut one_plan,
            paths: &mut paths,
            actions: &mut actions,
        },
        &mut scratch,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        HfsmError::BufferTooSmall {
            required_items: 2,
            ..
        }
    ));
    assert_eq!(one_plan[0], ItemPlan::EMPTY);
    assert!(paths.iter().all(|p| *p == ActiveState::EMPTY));

    let a = run_with(&items, &HfsmLimits::DEFAULT, 5, 1).unwrap();
    let b = run_with(&items, &HfsmLimits::DEFAULT, 64, 64).unwrap();
    assert_eq!(a.status, b.status);
    assert_eq!(a.items, b.items);
    assert_eq!(a.paths, b.paths);
    assert_eq!(a.actions, b.actions);
    assert_eq!(a.status.paths_written, 5);
    assert_eq!(a.status.actions_written, 1);
    assert_eq!(a.status.items_written, 2);
}

#[test]
fn t15_same_machine_twice_in_one_batch_rejects_whole_batch() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let items = [
        event_item(&def, &idle, 2, &[]),
        event_item(&def, &idle, 7, &[]),
    ];
    assert_eq!(
        run(&items).unwrap_err(),
        HfsmError::DuplicateMachineInBatch { key: MachineKey(7) }
    );
}

#[test]
fn batch_too_large_and_invalid_limits_are_rejected() {
    let def = compile_ok(&workflow_spec());
    let limits = HfsmLimits {
        max_batch: 1,
        ..HfsmLimits::DEFAULT
    };
    let items = [start_item(&def, 1, 1), start_item(&def, 2, 1)];
    assert_eq!(
        run_with(&items, &limits, 64, 64).unwrap_err(),
        HfsmError::BatchTooLarge { len: 2, max: 1 }
    );
    let bad = HfsmLimits {
        max_batch: 0,
        ..HfsmLimits::DEFAULT
    };
    let mut plans = vec![ItemPlan::EMPTY; 1];
    let mut paths = vec![ActiveState::EMPTY; 8];
    let mut actions = vec![ActionRecord::EMPTY; 8];
    let mut scratch = Scratch::new(&HfsmLimits::DEFAULT);
    let err = evaluate_batch(
        &[start_item(&def, 1, 1)],
        &bad,
        PlanOutput {
            items: &mut plans,
            paths: &mut paths,
            actions: &mut actions,
        },
        &mut scratch,
    )
    .unwrap_err();
    assert_eq!(err, HfsmError::LimitsInvalid);
}

#[test]
fn rejected_item_in_the_middle_does_not_affect_neighbours() {
    let def = compile_ok(&workflow_spec());
    let idle_a = started(&def, 1, 1);
    let idle_c = started(&def, 3, 1);
    let guards = [(GuardId(1), true)];
    let mut mismatched = event_item(&def, &idle_a, 1, &guards);
    mismatched.machine_key = MachineKey(2); // snapshot says 1
    let items = [
        event_item(&def, &idle_a, 1, &guards),
        mismatched,
        event_item(&def, &idle_c, 1, &guards),
    ];
    let r = run(&items).unwrap();
    assert!(matches!(r.plan(0).outcome, Outcome::Transitioned { .. }));
    assert_eq!(
        r.plan(1).outcome,
        Outcome::Rejected(ItemError::MachineKeyMismatch)
    );
    assert_eq!(r.plan(1).path_len, 0);
    assert_eq!(r.plan(1).action_len, 0);
    assert!(matches!(r.plan(2).outcome, Outcome::Transitioned { .. }));
    assert_eq!(r.next_snapshot(2).machine_key, MachineKey(3));
    assert_eq!(r.actions_of(2)[0].item_index, 2);
    assert_eq!(r.status.items_written, 3);
}

#[test]
fn empty_batch_is_ok_and_writes_nothing() {
    let r = run(&[]).unwrap();
    assert_eq!(
        r.status,
        BatchStatus {
            items_written: 0,
            paths_written: 0,
            actions_written: 0
        }
    );
}

#[test]
fn evaluate_does_not_mutate_inputs() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let before = idle.clone();
    let guards = [(GuardId(1), true)];
    let _ = run(&[event_item(&def, &idle, 1, &guards)]).unwrap();
    assert_eq!(idle, before);
}
