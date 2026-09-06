//! T10 / T16: guard tri-state errors and DeliveryScope stale rejection.

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn t10_missing_guard_is_an_error_not_false() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[event_item(&def, &idle, 1, &[])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::GuardMissing { guard: GuardId(1) })
    );
    assert!(r.plan(0).next.is_none());
}

#[test]
fn t10_duplicate_guard_entries_invalidate_the_frame() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let guards = [(GuardId(1), true), (GuardId(1), false)];
    let r = run(&[event_item(&def, &idle, 1, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::GuardFrameInvalid)
    );
}

#[test]
fn extra_guards_are_ignored_and_missing_guard_on_unreached_level_is_still_required() {
    // A1 --7[g1]--> A2 ; A --7[g2]--> A2. Frame has g1=true and a stray g9; g2 absent.
    // required_guards includes g2, so the frame is incomplete even though g1 would win.
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 7, 0, Some(1), 3),
            external(2, 1, 7, 0, Some(2), 3),
        ],
    ));
    let s = started(&def, 7, 1);
    let guards = [(GuardId(1), true), (GuardId(9), true)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::GuardMissing { guard: GuardId(2) })
    );
    let guards = [(GuardId(1), true), (GuardId(2), false), (GuardId(9), true)];
    let r = run(&[event_item(&def, &s, 7, &guards)]).unwrap();
    assert!(matches!(r.plan(0).outcome, Outcome::Transitioned { .. }));
}

#[test]
fn t16_stale_scope_is_rejected_without_touching_snapshot() {
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[event_item(&def, &idle, 1, &[(GuardId(1), true)])]).unwrap();
    let loading17 = r.next_snapshot(0); // Loading#4 in this run
    let loading_seq = loading17.active_path[2].activation_seq;

    // Cancel + Reset + Start again → Loading with a newer seq.
    let r = run(&[event_item(&def, &loading17, 6, &[])]).unwrap();
    let cancelled = r.next_snapshot(0);
    let r = run(&[event_item(&def, &cancelled, 7, &[])]).unwrap();
    let idle2 = r.next_snapshot(0);
    let r = run(&[event_item(&def, &idle2, 1, &[(GuardId(1), true)])]).unwrap();
    let loading18 = r.next_snapshot(0);
    assert!(loading18.active_path[2].activation_seq > loading_seq);

    // Old completion carries the old scope.
    let mut stale = event_item(&def, &loading18, 2, &[]);
    stale.scope = Some(DeliveryScope {
        state: StateId(4),
        activation_seq: loading_seq,
        epoch: loading18.epoch,
    });
    let r = run(&[stale]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::StaleDelivery)
    );
    assert!(r.plan(0).next.is_none());

    // Fresh scope passes.
    let mut fresh = event_item(&def, &loading18, 2, &[]);
    fresh.scope = Some(DeliveryScope {
        state: StateId(4),
        activation_seq: loading18.active_path[2].activation_seq,
        epoch: loading18.epoch,
    });
    let r = run(&[fresh]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Transitioned {
            transition: TransitionId(2)
        }
    );

    // Epoch mismatch alone is stale.
    let mut wrong_epoch = event_item(&def, &loading18, 2, &[]);
    wrong_epoch.scope = Some(DeliveryScope {
        state: StateId(4),
        activation_seq: loading18.active_path[2].activation_seq,
        epoch: MachineEpoch(loading18.epoch.0 + 1),
    });
    let r = run(&[wrong_epoch]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::StaleDelivery)
    );
}

#[test]
fn scope_on_ancestor_of_leaf_is_valid() {
    // Parent scope (Running) stays valid while child switches Loading → Executing (T19 kernel half).
    let def = compile_ok(&workflow_spec());
    let idle = started(&def, 7, 1);
    let r = run(&[event_item(&def, &idle, 1, &[(GuardId(1), true)])]).unwrap();
    let loading = r.next_snapshot(0);
    let r = run(&[event_item(&def, &loading, 2, &[])]).unwrap();
    let executing = r.next_snapshot(0);
    let mut item = event_item(&def, &executing, 4, &[]);
    item.scope = Some(DeliveryScope {
        state: StateId(3),
        activation_seq: executing.active_path[1].activation_seq,
        epoch: executing.epoch,
    });
    let r = run(&[item]).unwrap();
    assert!(matches!(r.plan(0).outcome, Outcome::Transitioned { .. }));
}
