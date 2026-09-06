//! T35: required_guards(leaf, event) is ordered by bubbling level then priority, deduplicated.

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn required_guards_are_ordered_by_level_then_priority_and_deduped() {
    // Root └─ A(initial A1) └─ A1 ; event 1 declared on A1 (p0 g5, p2 g1) and A (p0 g5, p1 g7).
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 2, Some(1), 3),
            external(2, 2, 1, 0, Some(5), 3),
            external(3, 1, 1, 1, Some(7), 3),
            external(4, 1, 1, 0, Some(5), 3),
        ],
    ));
    assert_eq!(
        def.required_guards(StateId(2), EventKind(1)),
        &[GuardId(5), GuardId(1), GuardId(7)]
    );
    // Leaf A2 has no own candidates; only A's apply.
    assert_eq!(
        def.required_guards(StateId(3), EventKind(1)),
        &[GuardId(5), GuardId(7)]
    );
    // Unknown event / unknown state → empty.
    assert!(def.required_guards(StateId(2), EventKind(9)).is_empty());
    assert!(def.required_guards(StateId(99), EventKind(1)).is_empty());
}

#[test]
fn candidates_are_sorted_by_priority() {
    let def = compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 7, Some(1), 3),
            external(2, 2, 1, 0, Some(2), 3),
            external(3, 2, 1, 3, Some(3), 3),
        ],
    ));
    assert_eq!(
        def.candidates(StateId(2), EventKind(1)),
        &[TransitionId(2), TransitionId(3), TransitionId(1)]
    );
    assert!(def.candidates(StateId(3), EventKind(1)).is_empty());
}

#[test]
fn guardless_transitions_contribute_no_guard_requirement() {
    let def = compile_ok(&workflow_spec());
    assert_eq!(def.required_guards(StateId(2), EventKind(1)), &[GuardId(1)]);
    assert!(def.required_guards(StateId(4), EventKind(2)).is_empty());
    assert!(def.required_guards(StateId(4), EventKind(7)).is_empty());
}
