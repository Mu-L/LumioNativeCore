//! Real execution, result ownership, cancellation and bounded-history regressions.
use lumio_job::*;
use lumio_kernel::capability::ConfiguredLimits;
use lumio_kernel::context::{CancelReason, ContextConfig, ContextPhase, KernelContext};
use lumio_kernel::error::{ErrorCategory, KernelResult};
use lumio_platform::{Deadline, StdMonotonicClock};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
const OP: OperationId = OperationId::from_raw(17);
struct Echo;
impl TypedKernel for Echo {
    fn operation_id(&self) -> OperationId {
        OP
    }
    fn execute(
        &self,
        input: &[u8],
        output: &mut [u8],
        control: &JobExecution,
    ) -> KernelResult<usize> {
        control.check_cancelled()?;
        if output.len() < input.len() {
            return Err(lumio_kernel::KernelError::buffer_too_small(
                input.len() as u64,
                output.len() as u64,
            ));
        }
        for (out, value) in output.iter_mut().zip(input) {
            *out = value.wrapping_add(1);
        }
        Ok(input.len())
    }
}
fn context() -> Arc<KernelContext> {
    KernelContext::create(ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 8,
            max_native_bytes: 256,
            max_jobs_queued: 2,
            max_jobs_running: 2,
            max_completion_items: 2,
        },
        quiesce_deadline: Deadline::NONE,
    })
    .unwrap()
}
fn system(
    context: &Arc<KernelContext>,
    kernel: Arc<dyn TypedKernel>,
    workers: usize,
) -> Arc<JobSystem> {
    let mut registry = OperationRegistry::new();
    registry.register(kernel).unwrap();
    JobSystem::create(
        context.clone(),
        JobSystemConfig {
            queue_capacity: 2,
            worker_count: workers,
        },
        Arc::new(registry),
        Arc::new(StdMonotonicClock::new()),
    )
    .unwrap()
}
fn request() -> JobRequest {
    JobRequest {
        operation: OP,
        deadline: Deadline::NONE,
    }
}
#[test]
fn actual_bytes_reap_and_result_reservation() {
    let context = context();
    let jobs = system(&context, Arc::new(Echo), 0);
    for _ in 0..1000 {
        let handle = jobs.submit_input(request(), &[1, 2, 3], 3).unwrap();
        assert!(jobs.pump_one());
        let result = jobs.take_result(handle).unwrap();
        assert_eq!(result.state, JobState::Succeeded);
        assert_eq!(result.bytes(), &[2, 3, 4]);
        assert_eq!(jobs.retained_jobs(), 0);
        assert!(jobs.poll(handle).is_err());
        assert_eq!(context.memory_budget().charged(), 6);
        drop(result);
        assert_eq!(context.memory_budget().charged(), 0);
    }
}
#[test]
fn completed_unreaped_jobs_apply_backpressure() {
    let context = context();
    let jobs = system(&context, Arc::new(Echo), 0);
    let first = jobs.submit(request()).unwrap();
    jobs.pump_one();
    let second = jobs.submit(request()).unwrap();
    jobs.pump_one();
    assert_eq!(
        jobs.submit(request()).unwrap_err().category(),
        ErrorCategory::CapacityExceeded
    );
    drop(jobs.take_result(first).unwrap());
    drop(jobs.take_result(second).unwrap());
    assert!(jobs.submit(request()).is_ok());
}
#[test]
fn foreign_system_handle_cannot_poll_or_cancel() {
    let context = context();
    let a = system(&context, Arc::new(Echo), 0);
    let b = system(&context, Arc::new(Echo), 0);
    let ah = a.submit(request()).unwrap();
    let bh = b.submit(request()).unwrap();
    assert!(b.poll(ah).is_err());
    assert!(b.cancel(ah).is_err());
    assert_eq!(b.poll(bh).unwrap().state, JobState::Queued);
}
struct Blocking {
    started: Arc<Barrier>,
    resume: Arc<Barrier>,
}
impl TypedKernel for Blocking {
    fn operation_id(&self) -> OperationId {
        OP
    }
    fn execute(&self, _: &[u8], _: &mut [u8], control: &JobExecution) -> KernelResult<usize> {
        self.started.wait();
        self.resume.wait();
        control.check_cancelled()?;
        Ok(0)
    }
}
#[test]
fn running_cancel_keeps_lease_until_real_quiescence() {
    let context = context();
    let started = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let jobs = system(
        &context,
        Arc::new(Blocking {
            started: started.clone(),
            resume: resume.clone(),
        }),
        1,
    );
    let handle = jobs.submit_input(request(), &[1, 2], 2).unwrap();
    started.wait();
    let cancel = jobs.cancel(handle).unwrap();
    let state = jobs.poll(handle).unwrap().state;
    let early_reap = jobs.take_result(handle).is_err();
    let charged = context.memory_budget().charged();
    let close = context
        .close(CancelReason::ContextClosing, Deadline::NONE)
        .unwrap();
    resume.wait();
    assert_eq!(cancel, CancelOutcome::Requested);
    assert_eq!(state, JobState::Running);
    assert!(early_reap);
    assert_eq!(charged, 4);
    assert_eq!(close.phase, ContextPhase::Quiescing);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if context
            .close(CancelReason::ContextClosing, Deadline::NONE)
            .unwrap()
            .phase
            == ContextPhase::Closed
        {
            break;
        }
        assert!(Instant::now() < deadline, "worker failed to quiesce");
        std::thread::yield_now();
    }
    assert_eq!(jobs.retained_jobs(), 0);
    assert_eq!(context.memory_budget().charged(), 0);
}
struct MetadataOnly;
impl TypedKernel for MetadataOnly {
    fn operation_id(&self) -> OperationId {
        OP
    }
}
struct Panics;
impl TypedKernel for Panics {
    fn operation_id(&self) -> OperationId {
        OP
    }
    fn execute(&self, _: &[u8], _: &mut [u8], _: &JobExecution) -> KernelResult<usize> {
        panic!("fixture kernel failure")
    }
}
#[test]
fn metadata_is_not_success_and_panic_is_failed_result() {
    for (kernel, category) in [
        (
            Arc::new(MetadataOnly) as Arc<dyn TypedKernel>,
            ErrorCategory::CapabilityUnavailable,
        ),
        (
            Arc::new(Panics) as Arc<dyn TypedKernel>,
            ErrorCategory::PanicBoundary,
        ),
    ] {
        let context = context();
        let jobs = system(&context, kernel, 0);
        let handle = jobs.submit(request()).unwrap();
        jobs.pump_one();
        let result = jobs.take_result(handle).unwrap();
        assert_eq!(result.state, JobState::Failed);
        assert_eq!(result.error.as_ref().unwrap().category(), category);
    }
}
#[test]
fn completion_history_remains_bounded() {
    let batch = CompletionBatch::with_capacity(2);
    for id in 1..1001 {
        let id = JobId::from_raw(id);
        let completion = JobCompletion {
            id,
            state: JobState::Succeeded,
        };
        batch.publish(completion).unwrap();
        let mut out = [completion];
        assert_eq!(batch.drain(&mut out).unwrap(), 1);
        batch.release(id).unwrap();
        assert!(batch.retained_count() <= 2);
    }
    assert!(
        batch
            .publish(JobCompletion {
                id: JobId::from_raw(1),
                state: JobState::Succeeded
            })
            .is_err()
    );
}
