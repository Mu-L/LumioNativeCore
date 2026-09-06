//! T27: every SnapshotError has a deterministic diagnosis; nothing wraps or truncates.

mod common;

use common::*;
use lumio_hfsm::*;

fn reject(def: &CompiledDefinition, s: &Snapshot, expected: SnapshotError) {
    assert_eq!(s.validate(def), Err(expected));
    let r = run(&[event_item(def, s, 2, &[])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::InvalidSnapshot(expected))
    );
    assert!(r.plan(0).next.is_none());
}

#[test]
fn fingerprint_mismatch() {
    let def = compile_ok(&workflow_spec());
    let mut s = snap(&def, 7, 1, 1, 3, &[(1, 1), (2, 2)]);
    s.fingerprint ^= 1;
    reject(&def, &s, SnapshotError::FingerprintMismatch);
}

#[test]
fn lifecycle_mismatch() {
    let def = compile_ok(&workflow_spec());
    let mut s = snap(&def, 7, 1, 1, 3, &[(1, 1), (2, 2)]);
    s.lifecycle = Lifecycle::Stopped;
    reject(&def, &s, SnapshotError::LifecycleMismatch);
    let mut s = snap(&def, 7, 1, 1, 3, &[]);
    s.lifecycle = Lifecycle::Running;
    reject(&def, &s, SnapshotError::LifecycleMismatch);
}

#[test]
fn path_shape_errors() {
    let def = compile_ok(&workflow_spec());
    // deeper than the definition
    let s = snap(&def, 7, 1, 1, 9, &[(1, 1), (3, 2), (4, 3), (4, 4)]);
    reject(&def, &s, SnapshotError::PathTooDeep);
    // unknown state
    let s = snap(&def, 7, 1, 1, 3, &[(1, 1), (99, 2)]);
    reject(&def, &s, SnapshotError::UnknownState { state: StateId(99) });
    // first element not top-level
    let s = snap(&def, 7, 1, 1, 3, &[(2, 1)]);
    reject(&def, &s, SnapshotError::PathNotChain { index: 0 });
    // broken parent link
    let s = snap(&def, 7, 1, 1, 3, &[(1, 1), (4, 2)]);
    reject(&def, &s, SnapshotError::PathNotChain { index: 1 });
    // stops on a composite
    let s = snap(&def, 7, 1, 1, 3, &[(1, 1), (3, 2)]);
    reject(&def, &s, SnapshotError::PathNotEndingAtLeaf);
}

#[test]
fn activation_seq_errors() {
    let def = compile_ok(&workflow_spec());
    let s = snap(&def, 7, 1, 1, 3, &[(1, 0), (2, 2)]);
    reject(&def, &s, SnapshotError::ActivationSeqInvalid { index: 0 });
    let s = snap(&def, 7, 1, 1, 3, &[(1, 1), (2, 3)]);
    reject(&def, &s, SnapshotError::ActivationSeqInvalid { index: 1 });
}

#[test]
fn counter_overflow_is_rejected_not_wrapped() {
    let def = compile_ok(&workflow_spec());
    let s = snap(&def, 7, 1, u64::MAX, 3, &[(1, 1), (2, 2)]);
    let r = run(&[event_item(&def, &s, 2, &[])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::CounterOverflow)
    );
    // next_activation_seq at MAX: a transition that allocates must fail, Internal/Unhandled may pass.
    let s = snap(&def, 7, 1, 1, u64::MAX, &[(1, 1), (2, 2)]);
    let r = run(&[event_item(&def, &s, 1, &[(GuardId(1), true)])]).unwrap();
    assert_eq!(
        r.plan(0).outcome,
        Outcome::Rejected(ItemError::CounterOverflow)
    );
}

#[test]
fn valid_snapshot_passes_and_leaf_helper_works() {
    let def = compile_ok(&workflow_spec());
    let s = snap(&def, 7, 1, 1, 5, &[(1, 1), (3, 3), (4, 4)]);
    assert_eq!(s.validate(&def), Ok(()));
    assert_eq!(s.leaf(), Some(StateId(4)));
    let fresh = Snapshot::not_started(def.fingerprint(), MachineKey(7));
    assert_eq!(fresh.validate(&def), Ok(()));
    assert_eq!(fresh.leaf(), None);
}
