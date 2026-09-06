//! Domain-neutral single timer kernel; ABI adapters live in the SDK repository.
#![forbid(unsafe_code)]
mod error;
mod ids;
mod manager;
pub use error::{TimerError, TimerResult};
pub use ids::{
    AdvanceReport, CallbackSlot, DELIVERY_QUEUE_DEPTH_PER_SLOT, Delivery, DispatchId,
    DispatchTarget, DrainOutcome, DrainReport, FiringRecord, FiringRejection,
    MAX_ACTIVE_TIMERS_PER_SCOPE, MAX_SCHEDULES_PER_PUMP, MAX_SCHEDULES_PER_TICK, MIN_INTERVAL_MS,
    MIN_INTERVAL_TICKS, ScopeKind, SliceTrace, SliceTraceEvent, SlotDispatchId, SlotLifecycle,
    TimerBudget, TimerDiagnostic, TimerHandle, TimerKind, TimerLimits, TimerMode, TimerScope,
};
pub use manager::TimerManager;
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;
#[cfg(feature = "test-support")]
pub use test_support::*;
