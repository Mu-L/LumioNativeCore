//! T-job-08 / R-00171: timeout vs complete / close vs submit spec winners.
//!
//! `TimedOut` is a completion observation (ADR 0004), not a CAS target.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use lumio_job::{
    CompletionBatch, JobCompletion, JobId, JobRequest, JobState, JobStateMachine, JobSystem,
    JobSystemConfig, OperationId, OperationRegistry, TypedKernel,
};
use lumio_kernel::capability::ConfiguredLimits;
use lumio_kernel::context::{CancelReason, ContextConfig, KernelContext};
use lumio_kernel::error::ErrorCategory;
use lumio_platform::{Deadline, MonotonicClock, Ticks};
use lumio_test_support::{FakeClock, Interleaving};

const DEADLINE_NANOS: u64 = 100;

struct DummyKernel {
    id: OperationId,
}

impl TypedKernel for DummyKernel {
    fn operation_id(&self) -> OperationId {
        self.id
    }
    fn execute(
        &self,
        input: &[u8],
        output: &mut [u8],
        control: &lumio_job::JobExecution,
    ) -> lumio_kernel::error::KernelResult<usize> {
        control.check_cancelled()?;
        assert!(input.is_empty() && output.is_empty());
        Ok(0)
    }
}

fn test_config() -> ContextConfig {
    ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 4,
            max_native_bytes: 64,
            max_jobs_queued: 4,
            max_jobs_running: 1,
            max_completion_items: 1,
        },
        quiesce_deadline: Deadline::NONE,
    }
}

fn late_clock() -> Arc<FakeClock> {
    let clock = FakeClock::new(Ticks::ZERO);
    clock.advance(Duration::from_nanos(DEADLINE_NANOS + 1));
    Arc::new(clock)
}

fn deadline() -> Deadline {
    Deadline::at(Ticks::from_nanos(DEADLINE_NANOS))
}

fn job_system(clock: Arc<FakeClock>) -> (Arc<KernelContext>, Arc<JobSystem>, OperationId) {
    let context = KernelContext::create_for_test(test_config());
    let mut registry = OperationRegistry::new();
    let op = OperationId::test_only(1);
    registry
        .register(Arc::new(DummyKernel { id: op }))
        .expect("register dummy kernel");
    let system = JobSystem::create(
        Arc::clone(&context),
        JobSystemConfig {
            queue_capacity: 2,
            worker_count: 0,
        },
        Arc::new(registry),
        clock,
    )
    .expect("create job system");
    (context, system, op)
}

#[test]
fn timeout_vs_complete_matches_spec_winner() {
    complete_wins_over_late_clock();
    concurrent_complete_rejects_timed_out_cas();
    expired_before_start_is_cancelled();
    unexpired_deadline_completes_succeeded();
    completed_result_is_published_not_timed_out();
    close_then_submit_is_rejected();
}

fn complete_wins_over_late_clock() {
    let clock = late_clock();
    assert!(deadline().is_expired(clock.now()), "clock is past deadline");

    let machine = JobStateMachine::queued();
    assert_eq!(machine.snapshot(), JobState::Queued);
    assert_eq!(machine.cas_start(), Ok(()));
    assert_eq!(machine.cas_complete(JobState::Succeeded), Ok(()));
    assert_eq!(machine.snapshot(), JobState::Succeeded);
    assert_ne!(machine.snapshot(), JobState::TimedOut);

    let machine = JobStateMachine::queued();
    assert_eq!(machine.cas_start(), Ok(()));
    assert_eq!(
        machine.cas_complete(JobState::TimedOut),
        Err(JobState::Running)
    );
    assert_eq!(machine.snapshot(), JobState::Running);
    assert_eq!(machine.cas_complete(JobState::Succeeded), Ok(()));
    assert_eq!(machine.snapshot(), JobState::Succeeded);
}

fn concurrent_complete_rejects_timed_out_cas() {
    let clock = late_clock();
    assert!(deadline().is_expired(clock.now()), "clock is past deadline");

    let machine = JobStateMachine::queued();
    assert_eq!(machine.cas_start(), Ok(()));
    let interleaving = Interleaving::new(&["go"]);
    let (complete, timeout) = thread::scope(|scope| {
        let complete = scope.spawn(|| {
            interleaving.arrive_and_wait("go");
            machine.cas_complete(JobState::Succeeded)
        });
        let timeout = scope.spawn(|| {
            interleaving.arrive_and_wait("go");
            machine.cas_complete(JobState::TimedOut)
        });
        (
            complete.join().expect("complete thread"),
            timeout.join().expect("timeout thread"),
        )
    });
    assert_eq!(complete, Ok(()));
    assert!(
        timeout == Err(JobState::Running) || timeout == Err(JobState::Succeeded),
        "TimedOut is not a CAS target: {timeout:?}"
    );
    assert_eq!(machine.snapshot(), JobState::Succeeded);
}

fn expired_before_start_is_cancelled() {
    let clock = late_clock();
    let (_context, system, op) = job_system(Arc::clone(&clock));
    let handle = system
        .submit(JobRequest {
            operation: op,
            deadline: deadline(),
        })
        .expect("submit");
    assert!(deadline().is_expired(clock.now()), "clock is past deadline");
    assert!(system.pump_one(), "queued job must be pumped");
    let snap = system.poll(handle).expect("poll");
    assert_eq!(snap.id, handle.id());
    assert_eq!(snap.state, JobState::Cancelled);
    assert_ne!(snap.state, JobState::Succeeded);
    assert_ne!(snap.state, JobState::TimedOut);
}

fn unexpired_deadline_completes_succeeded() {
    let clock = Arc::new(FakeClock::new(Ticks::ZERO));
    let (_context, system, op) = job_system(Arc::clone(&clock));
    let handle = system
        .submit(JobRequest {
            operation: op,
            deadline: deadline(),
        })
        .expect("submit");
    assert!(
        !deadline().is_expired(clock.now()),
        "clock is before deadline"
    );
    assert!(system.pump_one(), "queued job must be pumped");
    let snap = system.poll(handle).expect("poll");
    assert_eq!(snap.id, handle.id());
    assert_eq!(snap.state, JobState::Succeeded);
    assert_ne!(snap.state, JobState::TimedOut);
}

fn completed_result_is_published_not_timed_out() {
    let clock = late_clock();
    assert!(deadline().is_expired(clock.now()), "clock is past deadline");

    let machine = JobStateMachine::queued();
    assert_eq!(machine.cas_start(), Ok(()));
    assert_eq!(machine.cas_complete(JobState::Succeeded), Ok(()));

    let batch = CompletionBatch::with_capacity(1);
    let id = JobId::from_raw(1);
    batch
        .publish(JobCompletion {
            id,
            state: machine.snapshot(),
        })
        .expect("publish actual terminal");

    let mut out = [JobCompletion {
        id: JobId::from_raw(0),
        state: JobState::Queued,
    }; 1];
    let n = batch.drain(&mut out).expect("drain");
    assert_eq!(n, 1);
    assert_eq!(out[0].id, id);
    assert_eq!(out[0].state, JobState::Succeeded);
    assert_ne!(out[0].state, JobState::TimedOut);
    batch.release(id).expect("release once");
}

fn close_then_submit_is_rejected() {
    let context = KernelContext::create_for_test(test_config());
    let mut registry = OperationRegistry::new();
    let op = OperationId::test_only(1);
    registry
        .register(Arc::new(DummyKernel { id: op }))
        .expect("register dummy kernel");
    let system = JobSystem::create(
        Arc::clone(&context),
        JobSystemConfig {
            queue_capacity: 2,
            worker_count: 0,
        },
        Arc::new(registry),
        Arc::new(FakeClock::new(Ticks::ZERO)),
    )
    .expect("create job system");

    let interleaving = Interleaving::new(&["close", "submit"]);
    let submit = thread::scope(|scope| {
        scope.spawn(|| {
            interleaving.arrive_and_wait("close");
            context
                .close(CancelReason::ContextClosing, Deadline::NONE)
                .expect("close");
            interleaving.arrive_and_wait("submit");
        });
        let submit = scope.spawn(|| {
            interleaving.arrive_and_wait("close");
            interleaving.arrive_and_wait("submit");
            system.submit(JobRequest {
                operation: op,
                deadline: Deadline::NONE,
            })
        });
        submit.join().expect("submit thread")
    });

    let err = submit.expect_err("submit after close must fail");
    assert!(
        err.category() == ErrorCategory::ContextClosing
            || err.category() == ErrorCategory::ContextDestroyed,
        "expected ContextClosing or ContextDestroyed, got {:?}",
        err.category()
    );
}
