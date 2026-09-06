//! T28: `HfsmDefinitionRegistry` handle rules and ContextResource lifecycle (contract §9).

mod common;

use std::sync::Arc;

use common::*;
use lumio_hfsm::{
    CompileErrorKind, DefinitionHandle, DefinitionInfo, HfsmDefinitionRegistry, HfsmError,
    HfsmLimits, compile,
};
use lumio_kernel::capability::ConfiguredLimits;
use lumio_kernel::context::{
    CancelReason, ContextConfig, ContextPhase, ContextResource, Deadline, KernelContext,
};
use lumio_kernel::error::ErrorCategory;
use lumio_kernel::handle::ContextKey;

fn registry(ctx: u64, capacity: u32) -> HfsmDefinitionRegistry {
    HfsmDefinitionRegistry::new(ContextKey::new(ctx), capacity, HfsmLimits::DEFAULT)
}

fn assert_handle_err<T: std::fmt::Debug>(
    result: Result<T, HfsmError>,
    expected: ErrorCategory,
    what: &str,
) {
    match result {
        Err(HfsmError::Handle(category)) => {
            assert_eq!(category, expected, "{what}: wrong handle category");
        }
        other => panic!("{what}: expected Handle({expected:?}), got {other:?}"),
    }
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn create_then_inspect_matches_direct_compile() {
    assert_send_sync::<HfsmDefinitionRegistry>();
    assert_send_sync::<DefinitionHandle>();

    let reg = registry(1, 4);
    assert_eq!(reg.context(), ContextKey::new(1));
    assert_eq!(*reg.limits(), HfsmLimits::DEFAULT);
    assert!(reg.is_empty());

    let spec = workflow_spec();
    let direct = compile(&spec, &HfsmLimits::DEFAULT).expect("workflow compiles");
    let handle = reg.create(&spec).expect("create workflow");
    assert_eq!(handle.key().context, ContextKey::new(1));
    assert_eq!(reg.len(), 1);
    assert!(!reg.is_empty());

    let info = reg.inspect(handle).expect("inspect");
    assert_eq!(
        info,
        DefinitionInfo {
            fingerprint: direct.fingerprint(),
            state_count: direct.state_count(),
            transition_count: direct.transition_count(),
            max_depth: direct.max_depth(),
            max_plan_actions: direct.max_plan_actions(),
        }
    );

    let leased = reg.lease(handle).expect("lease");
    assert_eq!(leased.fingerprint(), direct.fingerprint());
    assert_eq!(leased.state_count(), direct.state_count());

    let second = reg.create(&spec).expect("second create");
    assert_ne!(second, handle, "distinct slots yield distinct handles");
    assert_eq!(reg.len(), 2);
}

#[test]
fn compile_failure_maps_to_compile_error_and_keeps_len() {
    let reg = registry(2, 4);
    let bad = spec(1, Vec::new(), Vec::new());
    match reg.create(&bad) {
        Err(HfsmError::Compile(e)) => assert_eq!(e.kind, CompileErrorKind::EmptyDefinition),
        other => panic!("expected Compile error, got {other:?}"),
    }
    assert_eq!(reg.len(), 0);
}

#[test]
fn foreign_context_handle_is_rejected() {
    let a = registry(10, 2);
    let b = registry(11, 2);
    let handle = a.create(&workflow_spec()).expect("create on A");

    assert_handle_err(b.lease(handle), ErrorCategory::WrongContext, "lease on B");
    assert_handle_err(
        b.inspect(handle),
        ErrorCategory::WrongContext,
        "inspect on B",
    );
    assert_handle_err(
        b.release(handle),
        ErrorCategory::WrongContext,
        "release on B",
    );
    assert_eq!(a.len(), 1, "A keeps its definition");
    assert_eq!(b.len(), 0);
}

#[test]
fn double_release_and_stale_lease_are_already_released() {
    let reg = registry(3, 2);
    let handle = reg.create(&workflow_spec()).expect("create");
    reg.release(handle).expect("first release");
    assert_eq!(reg.len(), 0);

    assert_handle_err(
        reg.release(handle),
        ErrorCategory::AlreadyReleased,
        "second release",
    );
    assert_handle_err(
        reg.lease(handle),
        ErrorCategory::AlreadyReleased,
        "lease after release",
    );
    assert_handle_err(
        reg.inspect(handle),
        ErrorCategory::AlreadyReleased,
        "inspect after release",
    );
}

#[test]
fn lease_survives_release() {
    let reg = registry(4, 2);
    let spec = workflow_spec();
    let expected = compile(&spec, &HfsmLimits::DEFAULT)
        .expect("compiles")
        .fingerprint();
    let handle = reg.create(&spec).expect("create");
    let leased = reg.lease(handle).expect("lease");
    reg.release(handle).expect("release");

    assert_eq!(leased.fingerprint(), expected);
    assert_eq!(Arc::strong_count(&leased), 1, "registry dropped its Arc");
}

#[test]
fn capacity_is_enforced_and_recovered_on_release() {
    let reg = registry(5, 1);
    let spec = workflow_spec();
    let first = reg.create(&spec).expect("first create");
    assert_handle_err(
        reg.create(&spec),
        ErrorCategory::CapacityExceeded,
        "second create at capacity 1",
    );
    assert_eq!(reg.len(), 1);

    reg.release(first).expect("release first");
    let again = reg.create(&spec).expect("create after release");
    assert_ne!(again, first, "generation bump yields a fresh handle");
    assert_handle_err(
        reg.lease(first),
        ErrorCategory::InvalidHandle,
        "old generation after slot reuse",
    );
    assert_eq!(reg.len(), 1);
}

#[test]
fn cancel_blocks_create_but_not_lease_and_destroy_blocks_everything() {
    let reg = registry(6, 4);
    let spec = workflow_spec();
    let handle = reg.create(&spec).expect("create");
    assert_eq!(reg.name(), "hfsm");

    reg.cancel_requested(CancelReason::ContextClosing);
    assert_handle_err(
        reg.create(&spec),
        ErrorCategory::ContextClosing,
        "create while closing",
    );
    reg.lease(handle)
        .expect("lease still allowed while closing");
    reg.inspect(handle)
        .expect("inspect still allowed while closing");
    assert_eq!(reg.len(), 1);

    let report = reg.quiesce(Deadline::NONE).expect("quiesce");
    assert_eq!(report.state, lumio_kernel::context::QuiesceState::Quiesced);

    let leased_before = reg.lease(handle).expect("lease before destroy");
    reg.destroy().expect("destroy");
    assert_eq!(reg.len(), 0);
    assert_handle_err(
        reg.create(&spec),
        ErrorCategory::ContextDestroyed,
        "create after destroy",
    );
    assert_handle_err(
        reg.lease(handle),
        ErrorCategory::ContextDestroyed,
        "lease after destroy",
    );
    assert_handle_err(
        reg.inspect(handle),
        ErrorCategory::ContextDestroyed,
        "inspect after destroy",
    );
    assert_handle_err(
        reg.release(handle),
        ErrorCategory::ContextDestroyed,
        "release after destroy",
    );
    assert_eq!(
        leased_before.fingerprint(),
        compile(&spec, &HfsmLimits::DEFAULT)
            .expect("compiles")
            .fingerprint(),
        "in-flight lease outlives destroy"
    );
    reg.destroy().expect("destroy is idempotent");
    assert_eq!(reg.len(), 0);
}

fn test_config() -> ContextConfig {
    ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 4,
            max_native_bytes: 64,
            max_jobs_queued: 1,
            max_jobs_running: 1,
            max_completion_items: 1,
        },
        quiesce_deadline: Deadline::NONE,
    }
}

#[test]
fn closing_a_real_kernel_context_destroys_the_registry() {
    let ctx = KernelContext::create_for_test(test_config());
    let reg = Arc::new(HfsmDefinitionRegistry::new(
        ctx.key(),
        4,
        HfsmLimits::DEFAULT,
    ));
    ctx.register_resource(Arc::clone(&reg) as Arc<dyn ContextResource>)
        .expect("register hfsm resource");

    let spec = workflow_spec();
    let handle = reg.create(&spec).expect("create before close");
    assert_eq!(handle.key().context, ctx.key());

    let report = ctx
        .close(CancelReason::ContextClosing, Deadline::NONE)
        .expect("close");
    assert_eq!(report.phase, ContextPhase::Closed);

    assert_handle_err(
        reg.create(&spec),
        ErrorCategory::ContextDestroyed,
        "create after context close",
    );
    assert_handle_err(
        reg.lease(handle),
        ErrorCategory::ContextDestroyed,
        "lease after context close",
    );
    assert_eq!(reg.len(), 0);
}
