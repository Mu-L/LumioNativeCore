//! Bounded Rust job execution with cooperative cancellation and explicit reaping.
#![forbid(unsafe_code)]
mod cancel;
mod completion;
mod id;
mod operation;
mod queue;
mod state;
mod worker;
pub use cancel::{CancellationSource, CancellationView};
pub use completion::{CompletionBatch, JobCompletion};
pub use id::{JobId, OperationId};
pub use operation::{OperationRegistry, TypedKernel};
pub use queue::BoundedJobQueue;
pub use state::{JobState, JobStateCell, JobStateMachine};
pub use worker::{
    CancelOutcome, JobExecution, JobHandle, JobRequest, JobResult, JobSnapshot, JobSystem,
    JobSystemConfig,
};
