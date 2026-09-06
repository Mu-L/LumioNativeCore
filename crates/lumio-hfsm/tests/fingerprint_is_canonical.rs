//! T34: fingerprint depends on normalized semantics only (contract §8).

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn names_and_array_order_do_not_change_fingerprint() {
    let base = compile_ok(&workflow_spec()).fingerprint();

    let mut renamed = workflow_spec();
    renamed.states[0].name = Some("Workflow".to_string());
    renamed.states[3].name = Some("Loading".to_string());
    assert_eq!(compile_ok(&renamed).fingerprint(), base);

    let mut reordered = workflow_spec();
    reordered.states.reverse();
    reordered.transitions.reverse();
    assert_eq!(compile_ok(&reordered).fingerprint(), base);
}

#[test]
fn semantic_changes_change_fingerprint() {
    let base = compile_ok(&workflow_spec()).fingerprint();

    let mut guard = workflow_spec();
    guard.transitions[0].guard = None;
    assert_ne!(compile_ok(&guard).fingerprint(), base);

    let mut kind = workflow_spec();
    kind.transitions[6].kind = TransitionKind::External { target: StateId(2) };
    assert_ne!(compile_ok(&kind).fingerprint(), base);

    let mut actions = workflow_spec();
    actions.states[3].entry.push(ActionId(999));
    assert_ne!(compile_ok(&actions).fingerprint(), base);

    let mut initial = workflow_spec();
    initial.states[2].initial = Some(StateId(5));
    assert_ne!(compile_ok(&initial).fingerprint(), base);

    let mut priority = workflow_spec();
    priority.transitions[0].priority = 3;
    assert_ne!(compile_ok(&priority).fingerprint(), base);

    let mut transition_id = workflow_spec();
    transition_id.transitions[0].id = TransitionId(77);
    assert_ne!(compile_ok(&transition_id).fingerprint(), base);
}

#[test]
fn fingerprint_is_fnv1a_over_documented_encoding() {
    // Single leaf state, no transitions: encoding is fully spelled out in contract §8.
    let def = compile_ok(&spec(1, vec![st(1, None, None)], vec![]));
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"LUMIO-HFSM\0");
    for v in [FORMAT_VERSION, SEMANTICS_VERSION, 1u32, 1u32] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // state 1: id, parent=NONE, initial=NONE, entry_len=0, exit_len=0
    for v in [1u32, u32::MAX, u32::MAX, 0, 0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // transition_count
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    assert_eq!(def.fingerprint(), h);
}
