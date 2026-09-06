//! Pending and registration races through the real Context API.
use lumio_kernel::capability::ConfiguredLimits;
use lumio_kernel::context::*;
use lumio_kernel::error::{ErrorCategory, KernelResult};
use lumio_platform::{Deadline, Ticks};
use lumio_test_support::FakeClock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
fn config() -> ContextConfig {
    ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 8,
            max_native_bytes: 64,
            max_jobs_queued: 2,
            max_jobs_running: 1,
            max_completion_items: 2,
        },
        quiesce_deadline: Deadline::NONE,
    }
}
struct Pending {
    ready: AtomicBool,
    destroyed: AtomicUsize,
}
impl ContextResource for Pending {
    fn name(&self) -> &'static str {
        "pending"
    }
    fn cancel_requested(&self, _: CancelReason) {}
    fn quiesce(&self, _: Deadline) -> KernelResult<QuiesceReport> {
        Ok(QuiesceReport {
            state: if self.ready.load(Ordering::SeqCst) {
                QuiesceState::Quiesced
            } else {
                QuiesceState::Pending { remaining: 1 }
            },
        })
    }
    fn destroy(&self) -> KernelResult<()> {
        assert!(self.ready.load(Ordering::SeqCst));
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn pending_close_does_not_destroy_and_can_resume() {
    let context = KernelContext::create(config()).unwrap();
    let resource = Arc::new(Pending {
        ready: AtomicBool::new(false),
        destroyed: AtomicUsize::new(0),
    });
    context.register_resource(resource.clone()).unwrap();
    assert_eq!(
        context
            .close(CancelReason::ContextClosing, Deadline::NONE)
            .unwrap()
            .phase,
        ContextPhase::Quiescing
    );
    assert_eq!(resource.destroyed.load(Ordering::SeqCst), 0);
    resource.ready.store(true, Ordering::SeqCst);
    assert_eq!(
        context
            .close(CancelReason::ContextClosing, Deadline::NONE)
            .unwrap()
            .phase,
        ContextPhase::Closed
    );
    context
        .close(CancelReason::ContextClosing, Deadline::NONE)
        .unwrap();
    assert_eq!(resource.destroyed.load(Ordering::SeqCst), 1);
}
#[test]
fn deadline_retains_pending_resources_for_retry() {
    let clock = Arc::new(FakeClock::new(Ticks::from_nanos(10)));
    let context = KernelContext::create_with_clock(config(), clock).unwrap();
    let resource = Arc::new(Pending {
        ready: AtomicBool::new(false),
        destroyed: AtomicUsize::new(0),
    });
    context.register_resource(resource.clone()).unwrap();
    assert_eq!(
        context
            .close(
                CancelReason::ContextClosing,
                Deadline::at(Ticks::from_nanos(9))
            )
            .unwrap_err()
            .category(),
        ErrorCategory::TimedOut
    );
    assert_eq!(resource.destroyed.load(Ordering::SeqCst), 0);
    resource.ready.store(true, Ordering::SeqCst);
    assert_eq!(
        context
            .close(CancelReason::ContextClosing, Deadline::NONE)
            .unwrap()
            .phase,
        ContextPhase::Closed
    );
}
struct DelayedName {
    entered: Arc<Barrier>,
    resume: Arc<Barrier>,
    destroyed: AtomicUsize,
}
impl ContextResource for DelayedName {
    fn name(&self) -> &'static str {
        self.entered.wait();
        self.resume.wait();
        "late"
    }
    fn cancel_requested(&self, _: CancelReason) {}
    fn quiesce(&self, _: Deadline) -> KernelResult<QuiesceReport> {
        Ok(QuiesceReport {
            state: QuiesceState::Quiesced,
        })
    }
    fn destroy(&self) -> KernelResult<()> {
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn resource_losing_close_cannot_register_after_snapshot() {
    let context = KernelContext::create(config()).unwrap();
    let entered = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let resource = Arc::new(DelayedName {
        entered: entered.clone(),
        resume: resume.clone(),
        destroyed: AtomicUsize::new(0),
    });
    let result = std::thread::scope(|scope| {
        let owner = context.clone();
        let resource = resource.clone();
        let registration = scope.spawn(move || owner.register_resource(resource));
        entered.wait();
        let close = context.close(CancelReason::ContextClosing, Deadline::NONE);
        resume.wait();
        assert_eq!(close.unwrap().phase, ContextPhase::Closed);
        registration.join().unwrap()
    });
    assert!(result.is_err());
    assert_eq!(resource.destroyed.load(Ordering::SeqCst), 0);
}
