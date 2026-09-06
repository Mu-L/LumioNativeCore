//! Context admission and resumable, non-blocking close.
//!
//! An admission guard covers the complete publication of new work. Closing takes
//! the same gate before freezing the resource set. Resource callbacks never run
//! under that gate or the registry lock. A Pending resource is never destroyed.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use lumio_platform::{Deadline, MonotonicClock, StdMonotonicClock};

use super::config::ContextConfig;
use super::registry::{ResourceRegistration, ResourceRegistry};
use super::resource::{CancelReason, ContextResource, QuiesceState};
use super::state::{ContextPhase, ContextStateGate};
use crate::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use crate::handle::ContextKey;
use crate::memory::MemoryBudget;

const CLOSE_STEPS: &[&str] = &[
    "reject_new_work",
    "cancel_requested",
    "quiesce",
    "wait_quiesce",
    "drain",
    "destroy",
    "mark_closed",
];
static NEXT_CONTEXT_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextCloseReport {
    pub steps: &'static [&'static str],
    pub phase: ContextPhase,
}

#[derive(Default)]
struct CloseProgress {
    started: bool,
    cancelled: usize,
    resources: Vec<Arc<dyn ContextResource>>,
    quiesced: Vec<bool>,
    destroy_remaining: usize,
    report: Option<ContextCloseReport>,
    failure: Option<KernelError>,
}

pub struct KernelContext {
    key: ContextKey,
    config: ContextConfig,
    clock: Arc<dyn MonotonicClock>,
    budget: Arc<MemoryBudget>,
    gate: ContextStateGate,
    admission: Mutex<()>,
    resources: ResourceRegistry,
    close_progress: Mutex<CloseProgress>,
}

fn error(category: ErrorCategory) -> KernelError {
    KernelError::new(category, ErrorDetail::None)
}

impl KernelContext {
    pub fn create(config: ContextConfig) -> KernelResult<Arc<Self>> {
        Self::create_with_clock(config, Arc::new(StdMonotonicClock::new()))
    }

    /// Deadlines passed to close must use this clock's epoch.
    pub fn create_with_clock(
        config: ContextConfig,
        clock: Arc<dyn MonotonicClock>,
    ) -> KernelResult<Arc<Self>> {
        config.validate()?;
        let raw = NEXT_CONTEXT_KEY
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
            .map_err(|_| error(ErrorCategory::CapacityExceeded))?;
        Ok(Arc::new(Self {
            key: ContextKey::new(raw),
            config,
            clock,
            budget: Arc::new(MemoryBudget::new(config.limits.max_native_bytes)),
            gate: ContextStateGate::new_running(),
            admission: Mutex::new(()),
            resources: ResourceRegistry::new(),
            close_progress: Mutex::new(CloseProgress::default()),
        }))
    }

    /// Compatibility convenience for fixtures; production callers use create.
    pub fn create_for_test(config: ContextConfig) -> Arc<Self> {
        Self::create(config).expect("valid test ContextConfig")
    }

    pub fn key(&self) -> ContextKey {
        self.key
    }
    pub fn config(&self) -> ContextConfig {
        self.config
    }
    pub fn phase(&self) -> ContextPhase {
        self.gate.snapshot().phase
    }
    pub fn memory_budget(&self) -> Arc<MemoryBudget> {
        Arc::clone(&self.budget)
    }

    /// Hold until new work is fully published. Do not run user callbacks, block,
    /// re-enter the context, or retain this guard across an FFI boundary.
    pub fn admit_work(&self) -> KernelResult<MutexGuard<'_, ()>> {
        let guard = self
            .admission
            .lock()
            .map_err(|_| error(ErrorCategory::InternalInvariant))?;
        self.gate.try_admit()?;
        Ok(guard)
    }

    pub fn register_resource(
        &self,
        resource: Arc<dyn ContextResource>,
    ) -> KernelResult<ResourceRegistration> {
        // name() is a user callback and therefore precedes admission.
        let name = resource.name();
        let _admission = self.admit_work()?;
        self.resources
            .register_named(name, resource, self.config.limits.max_handles)
    }

    /// A snapshot only. Mutating operations must use admit_work instead.
    pub fn ensure_accepting_work(&self) -> KernelResult<()> {
        self.gate.try_admit()
    }

    /// Drive one close pass. Quiescing means the caller must poll close again;
    /// TimedOut retains resources for a later retry with a new deadline. Neither
    /// outcome grants permission to destroy active resources. No background
    /// reaper or unbounded wait is hidden in this API.
    pub fn close(
        &self,
        reason: CancelReason,
        deadline: Deadline,
    ) -> KernelResult<ContextCloseReport> {
        let mut progress = match self.close_progress.try_lock() {
            Ok(p) => p,
            Err(TryLockError::WouldBlock) => return Err(error(ErrorCategory::ContextClosing)),
            Err(TryLockError::Poisoned(_)) => return Err(error(ErrorCategory::InternalInvariant)),
        };
        if let Some(report) = progress.report {
            return Ok(report);
        }
        if let Some(failure) = &progress.failure {
            return Err(failure.clone());
        }
        if !progress.started {
            let _admission = self
                .admission
                .lock()
                .map_err(|_| error(ErrorCategory::InternalInvariant))?;
            self.gate.begin_close();
            progress.resources = self.resources.snapshot();
            progress.quiesced = vec![false; progress.resources.len()];
            progress.destroy_remaining = progress.resources.len();
            progress.started = true;
        }
        while progress.cancelled < progress.resources.len() {
            let index = progress.cancelled;
            if catch_unwind(AssertUnwindSafe(|| {
                progress.resources[index].cancel_requested(reason)
            }))
            .is_err()
            {
                let failure = error(ErrorCategory::PanicBoundary);
                progress.failure = Some(failure.clone());
                return Err(failure);
            }
            progress.cancelled += 1;
        }
        let effective = if deadline == Deadline::NONE {
            self.config.quiesce_deadline
        } else {
            deadline
        };
        for index in 0..progress.resources.len() {
            if progress.quiesced[index] {
                continue;
            }
            match catch_unwind(AssertUnwindSafe(|| {
                progress.resources[index].quiesce(effective)
            })) {
                Ok(Ok(report)) => progress.quiesced[index] = report.state == QuiesceState::Quiesced,
                Ok(Err(failure)) if failure.category() == ErrorCategory::TimedOut => {
                    return Err(failure);
                }
                result => {
                    let failure = match result {
                        Ok(Err(failure)) => failure,
                        _ => error(ErrorCategory::PanicBoundary),
                    };
                    progress.failure = Some(failure.clone());
                    return Err(failure);
                }
            }
        }
        if progress.quiesced.iter().any(|ready| !ready) {
            if effective.is_expired(self.clock.now()) {
                return Err(error(ErrorCategory::TimedOut));
            }
            return Ok(ContextCloseReport {
                steps: &CLOSE_STEPS[..3],
                phase: ContextPhase::Quiescing,
            });
        }
        while progress.destroy_remaining != 0 {
            let index = progress.destroy_remaining - 1;
            match catch_unwind(AssertUnwindSafe(|| progress.resources[index].destroy())) {
                Ok(Ok(())) => progress.destroy_remaining -= 1,
                result => {
                    let failure = match result {
                        Ok(Err(failure)) => failure,
                        _ => error(ErrorCategory::PanicBoundary),
                    };
                    // Do not invoke an ambiguously failed destructor a second time.
                    progress.failure = Some(failure.clone());
                    return Err(failure);
                }
            }
        }
        self.resources.clear();
        progress.resources.clear();
        self.gate.mark_closed();
        let report = ContextCloseReport {
            steps: CLOSE_STEPS,
            phase: ContextPhase::Closed,
        };
        progress.report = Some(report);
        Ok(report)
    }
}

impl Drop for KernelContext {
    fn drop(&mut self) {
        // Best effort only; callers needing a completion guarantee must poll close.
        let _ = self.close(CancelReason::ContextClosing, Deadline::NONE);
    }
}
