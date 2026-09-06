//! Real executable kernel re-enters scheduler queries, without a boolean oracle.
use lumio_job::{
    JobExecution, JobRequest, JobState, JobSystem, JobSystemConfig, OperationId, OperationRegistry,
    TypedKernel,
};
use lumio_kernel::capability::ConfiguredLimits;
use lumio_kernel::context::{ContextConfig, KernelContext};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use lumio_platform::{Deadline, StdMonotonicClock};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::Duration;
struct Reentrant {
    system: Mutex<Weak<JobSystem>>,
}
impl TypedKernel for Reentrant {
    fn operation_id(&self) -> OperationId {
        OperationId::from_raw(7)
    }
    fn execute(&self, _: &[u8], _: &mut [u8], _: &JobExecution) -> KernelResult<usize> {
        let system = self.system.lock().unwrap().upgrade().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(system.retained_jobs());
        });
        let count = rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| KernelError::new(ErrorCategory::InternalInvariant, ErrorDetail::None))?;
        assert_eq!(count, 1);
        Ok(0)
    }
}
#[test]
fn worker_never_executes_under_scheduler_lock() {
    let context = KernelContext::create(ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 8,
            max_native_bytes: 64,
            max_jobs_queued: 2,
            max_jobs_running: 1,
            max_completion_items: 3,
        },
        quiesce_deadline: Deadline::NONE,
    })
    .unwrap();
    let kernel = Arc::new(Reentrant {
        system: Mutex::new(Weak::new()),
    });
    let mut registry = OperationRegistry::new();
    registry.register(kernel.clone()).unwrap();
    let system = JobSystem::create(
        context.clone(),
        JobSystemConfig {
            queue_capacity: 2,
            worker_count: 0,
        },
        Arc::new(registry),
        Arc::new(StdMonotonicClock::new()),
    )
    .unwrap();
    *kernel.system.lock().unwrap() = Arc::downgrade(&system);
    let request = JobRequest {
        operation: OperationId::from_raw(7),
        deadline: Deadline::NONE,
    };
    let handle = system.submit(request).unwrap();
    assert!(system.pump_one());
    assert_eq!(
        system.take_result(handle).unwrap().state,
        JobState::Succeeded
    );
    system.submit(request).unwrap();
    system.submit(request).unwrap();
    assert_eq!(
        system.submit(request).unwrap_err().category(),
        ErrorCategory::CapacityExceeded
    );
}
