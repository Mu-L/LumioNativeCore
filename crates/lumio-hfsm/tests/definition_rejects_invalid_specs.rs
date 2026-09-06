//! H02 / T09 / T32 / T33: compile-time validation of DefinitionSpec.

mod common;

use common::*;
use lumio_hfsm::*;

#[test]
fn limits_with_zero_are_rejected() {
    let limits = HfsmLimits {
        max_depth: 0,
        ..HfsmLimits::DEFAULT
    };
    let err = compile(&workflow_spec(), &limits).expect_err("zero limit");
    assert_eq!(err.kind, CompileErrorKind::LimitsInvalid);
}

#[test]
fn unsupported_format_version_is_rejected() {
    let mut s = workflow_spec();
    s.format_version = FORMAT_VERSION + 1;
    let err = compile_err(&s);
    assert_eq!(
        err.kind,
        CompileErrorKind::FormatVersionUnsupported {
            found: FORMAT_VERSION + 1
        }
    );
}

#[test]
fn empty_definition_is_rejected() {
    let err = compile_err(&spec(1, vec![], vec![]));
    assert_eq!(err.kind, CompileErrorKind::EmptyDefinition);
}

#[test]
fn duplicate_state_id_is_rejected() {
    let err = compile_err(&spec(1, vec![st(1, None, None), st(1, None, None)], vec![]));
    assert_eq!(err.kind, CompileErrorKind::DuplicateStateId);
    assert_eq!(err.state, Some(StateId(1)));
}

#[test]
fn duplicate_transition_id_is_rejected() {
    let err = compile_err(&spec(
        1,
        a_tree(),
        vec![external(1, 2, 1, 0, None, 3), external(1, 3, 1, 0, None, 2)],
    ));
    assert_eq!(err.kind, CompileErrorKind::DuplicateTransitionId);
    assert_eq!(err.transition, Some(TransitionId(1)));
}

#[test]
fn unknown_state_references_are_rejected() {
    let unknown_parent = compile_err(&spec(
        1,
        vec![st(1, None, None), st(2, Some(9), None)],
        vec![],
    ));
    assert_eq!(
        unknown_parent.kind,
        CompileErrorKind::UnknownState {
            referenced: StateId(9)
        }
    );
    assert_eq!(unknown_parent.state, Some(StateId(2)));

    let unknown_initial = compile_err(&spec(
        1,
        vec![st(1, None, Some(9)), st(2, Some(1), None)],
        vec![],
    ));
    assert_eq!(
        unknown_initial.kind,
        CompileErrorKind::UnknownState {
            referenced: StateId(9)
        }
    );

    let unknown_root_initial = compile_err(&spec(9, vec![st(1, None, None)], vec![]));
    assert_eq!(
        unknown_root_initial.kind,
        CompileErrorKind::UnknownState {
            referenced: StateId(9)
        }
    );

    let unknown_source = compile_err(&spec(1, a_tree(), vec![external(1, 9, 1, 0, None, 3)]));
    assert_eq!(
        unknown_source.kind,
        CompileErrorKind::UnknownState {
            referenced: StateId(9)
        }
    );
    assert_eq!(unknown_source.transition, Some(TransitionId(1)));

    let unknown_target = compile_err(&spec(1, a_tree(), vec![external(1, 2, 1, 0, None, 9)]));
    assert_eq!(
        unknown_target.kind,
        CompileErrorKind::UnknownState {
            referenced: StateId(9)
        }
    );
}

#[test]
fn root_initial_must_be_top_level() {
    let err = compile_err(&spec(2, a_tree(), vec![]));
    assert_eq!(err.kind, CompileErrorKind::RootInitialNotTopLevel);
}

#[test]
fn initial_must_be_direct_child() {
    // A(initial A1a) ├─ A1(initial A1a) └─ A1a : A's initial skips a level
    let err = compile_err(&spec(
        1,
        vec![
            st(1, None, Some(3)),
            st(2, Some(1), Some(3)),
            st(3, Some(2), None),
        ],
        vec![],
    ));
    assert_eq!(err.kind, CompileErrorKind::InitialNotDirectChild);
    assert_eq!(err.state, Some(StateId(1)));
}

#[test]
fn composite_without_initial_and_leaf_with_initial_are_rejected() {
    let composite = compile_err(&spec(
        1,
        vec![st(1, None, None), st(2, Some(1), None)],
        vec![],
    ));
    assert_eq!(composite.kind, CompileErrorKind::CompositeWithoutInitial);
    assert_eq!(composite.state, Some(StateId(1)));

    let leaf = compile_err(&spec(
        1,
        vec![st(1, None, Some(2)), st(2, Some(1), Some(2))],
        vec![],
    ));
    assert_eq!(leaf.kind, CompileErrorKind::LeafWithInitial);
    assert_eq!(leaf.state, Some(StateId(2)));
}

#[test]
fn structural_parent_cycle_is_rejected_but_business_loop_compiles() {
    // T32: parent cycle 2 -> 3 -> 2
    let err = compile_err(&spec(
        1,
        vec![
            st(1, None, None),
            st(2, Some(3), Some(3)),
            st(3, Some(2), Some(2)),
        ],
        vec![],
    ));
    assert_eq!(err.kind, CompileErrorKind::StructuralCycle);

    // Idle --Start--> Running --Reset--> Idle is a legal event loop.
    let def = compile_ok(&workflow_spec());
    assert_eq!(def.state_count(), 8);
    assert_eq!(def.transition_count(), 7);
}

#[test]
fn depth_and_count_limits_are_enforced() {
    let limits = HfsmLimits {
        max_depth: 2,
        ..HfsmLimits::DEFAULT
    };
    let three_deep = spec(
        1,
        vec![
            st(1, None, Some(2)),
            st(2, Some(1), Some(3)),
            st(3, Some(2), None),
        ],
        vec![],
    );
    let err = compile(&three_deep, &limits).expect_err("depth 3 > 2");
    assert_eq!(
        err.kind,
        CompileErrorKind::DepthExceeded { depth: 3, max: 2 }
    );
    assert_eq!(err.state, Some(StateId(3)));

    let limits = HfsmLimits {
        max_states: 2,
        ..HfsmLimits::DEFAULT
    };
    let err = compile(&spec(1, a_tree(), vec![]), &limits).expect_err("3 states > 2");
    assert_eq!(
        err.kind,
        CompileErrorKind::StateCountExceeded { count: 3, max: 2 }
    );

    let limits = HfsmLimits {
        max_transitions: 1,
        ..HfsmLimits::DEFAULT
    };
    let err = compile(
        &spec(
            1,
            a_tree(),
            vec![external(1, 2, 1, 0, None, 3), external(2, 3, 1, 0, None, 2)],
        ),
        &limits,
    )
    .expect_err("2 transitions > 1");
    assert_eq!(
        err.kind,
        CompileErrorKind::TransitionCountExceeded { count: 2, max: 1 }
    );
}

#[test]
fn duplicate_priority_names_both_transitions() {
    // T09
    let err = compile_err(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 0, Some(1), 3),
            external(2, 2, 1, 0, Some(2), 3),
        ],
    ));
    assert_eq!(
        err.kind,
        CompileErrorKind::DuplicatePriority {
            other: TransitionId(1)
        }
    );
    assert_eq!(err.transition, Some(TransitionId(2)));
}

#[test]
fn transition_after_guardless_one_is_unreachable() {
    // T33: priority 0 has no guard, so priority 5 can never be selected.
    let err = compile_err(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 0, None, 3),
            external(2, 2, 1, 5, Some(1), 3),
        ],
    ));
    assert_eq!(
        err.kind,
        CompileErrorKind::UnreachableTransition {
            shadowed_by: TransitionId(1)
        }
    );
    assert_eq!(err.transition, Some(TransitionId(2)));

    // Guarded first, guardless last is fine.
    compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 0, Some(1), 3),
            external(2, 2, 1, 5, None, 3),
        ],
    ));
}

#[test]
fn local_target_must_be_strict_descendant() {
    let self_target = compile_err(&spec(1, a_tree(), vec![local(1, 1, 1, 0, None, 1)]));
    assert_eq!(
        self_target.kind,
        CompileErrorKind::LocalTargetNotStrictDescendant
    );

    let sibling = compile_err(&spec(1, a_tree(), vec![local(1, 2, 1, 0, None, 3)]));
    assert_eq!(
        sibling.kind,
        CompileErrorKind::LocalTargetNotStrictDescendant
    );

    let ancestor = compile_err(&spec(1, a_tree(), vec![local(1, 2, 1, 0, None, 1)]));
    assert_eq!(
        ancestor.kind,
        CompileErrorKind::LocalTargetNotStrictDescendant
    );

    compile_ok(&spec(1, a_tree(), vec![local(1, 1, 1, 0, None, 3)]));
}

#[test]
fn external_transition_to_synthetic_root_is_impossible_but_any_state_is_allowed() {
    // External may target an ancestor, a sibling, or itself.
    compile_ok(&spec(
        1,
        a_tree(),
        vec![
            external(1, 2, 1, 0, None, 1),
            external(2, 2, 2, 0, None, 3),
            external(3, 2, 3, 0, None, 2),
        ],
    ));
}

#[test]
fn worst_case_plan_actions_are_bounded_at_compile_time() {
    // A(exit 1) initial A1(entry 2, exit 3); A1 --e--> A (External):
    // exit A1 (1) + exit A (1) + transition (1) + enter A (0) + enter A1 (1) = 4
    let s = spec(
        1,
        vec![
            st_actions(1, None, Some(2), &[], &[1]),
            st_actions(2, Some(1), None, &[2], &[3]),
        ],
        vec![TransitionSpec {
            actions: vec![ActionId(9)],
            ..external(1, 2, 1, 0, None, 1)
        }],
    );
    let def = compile_ok(&s);
    assert_eq!(def.max_plan_actions(), 4);

    let limits = HfsmLimits {
        max_actions_per_plan: 3,
        ..HfsmLimits::DEFAULT
    };
    let err = compile(&s, &limits).expect_err("4 > 3");
    assert_eq!(
        err.kind,
        CompileErrorKind::ActionsPerPlanExceeded {
            required: 4,
            max: 3
        }
    );
    assert_eq!(err.transition, Some(TransitionId(1)));
}

#[test]
fn start_expansion_counts_toward_plan_actions() {
    // Start enters A(entry 1,2) then A1(entry 3) = 3 actions, no transitions at all.
    let s = spec(
        1,
        vec![
            st_actions(1, None, Some(2), &[1, 2], &[]),
            st_actions(2, Some(1), None, &[3], &[]),
        ],
        vec![],
    );
    assert_eq!(compile_ok(&s).max_plan_actions(), 3);
    let limits = HfsmLimits {
        max_actions_per_plan: 2,
        ..HfsmLimits::DEFAULT
    };
    let err = compile(&s, &limits).expect_err("start needs 3");
    assert_eq!(
        err.kind,
        CompileErrorKind::ActionsPerPlanExceeded {
            required: 3,
            max: 2
        }
    );
    assert_eq!(err.transition, None);
}

#[test]
fn compiled_definition_exposes_structure() {
    let def = compile_ok(&workflow_spec());
    assert_eq!(def.root_initial(), StateId(1));
    assert_eq!(def.max_depth(), 3);
    assert_eq!(def.depth_of(StateId(1)), Some(1));
    assert_eq!(def.depth_of(StateId(4)), Some(3));
    assert_eq!(def.depth_of(StateId(99)), None);
    assert_eq!(def.parent_of(StateId(4)), Some(StateId(3)));
    assert_eq!(def.parent_of(StateId(1)), None);
    assert_eq!(def.is_leaf(StateId(4)), Some(true));
    assert_eq!(def.is_leaf(StateId(3)), Some(false));
    assert_eq!(
        def.initial_chain(StateId(1)).collect::<Vec<_>>(),
        vec![StateId(2)]
    );
    assert_eq!(
        def.initial_chain(StateId(3)).collect::<Vec<_>>(),
        vec![StateId(4)]
    );
    assert!(def.initial_chain(StateId(4)).next().is_none());
    assert!(def.has_event(EventKind(7)));
    assert!(!def.has_event(EventKind(42)));
    assert_eq!(def.exit_actions(StateId(3)), &[ActionId(103)]);
    assert_eq!(def.entry_actions(StateId(4)), &[ActionId(101)]);
    let t = def.transition(TransitionId(7)).expect("Reset");
    assert_eq!(t.kind, TransitionKind::Local { target: StateId(2) });
    assert!(def.transition(TransitionId(70)).is_none());
}
