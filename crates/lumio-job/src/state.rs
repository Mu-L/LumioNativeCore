//! Atomic execution state. Running cancellation is finalized only by the worker.
use std::sync::atomic::{AtomicU8, Ordering};
const ORDER: Ordering = Ordering::SeqCst;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobState {
    Queued = 0,
    Running = 1,
    Succeeded = 2,
    Failed = 3,
    Cancelled = 4,
    TimedOut = 5,
}
impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
pub type JobStateCell = JobStateMachine;
pub struct JobStateMachine {
    state: AtomicU8,
}
impl JobStateMachine {
    pub fn queued() -> Self {
        Self {
            state: AtomicU8::new(JobState::Queued as u8),
        }
    }
    pub fn snapshot(&self) -> JobState {
        decode(self.state.load(ORDER))
    }
    pub fn cas_start(&self) -> Result<(), JobState> {
        self.state
            .compare_exchange(
                JobState::Queued as u8,
                JobState::Running as u8,
                ORDER,
                ORDER,
            )
            .map(|_| ())
            .map_err(decode)
    }
    /// Terminal transition for queued work or a worker's acknowledged cancel point.
    /// A client cancelling running work must set the request token, not call this.
    pub fn cas_cancel(&self) -> Result<JobState, JobState> {
        loop {
            let current = self.snapshot();
            if !matches!(current, JobState::Queued | JobState::Running) {
                return Err(current);
            }
            if self
                .state
                .compare_exchange(current as u8, JobState::Cancelled as u8, ORDER, ORDER)
                .is_ok()
            {
                return Ok(current);
            }
        }
    }
    pub fn cas_complete(&self, to: JobState) -> Result<(), JobState> {
        if !matches!(to, JobState::Succeeded | JobState::Failed) {
            return Err(self.snapshot());
        }
        self.state
            .compare_exchange(JobState::Running as u8, to as u8, ORDER, ORDER)
            .map(|_| ())
            .map_err(decode)
    }
}
fn decode(raw: u8) -> JobState {
    match raw {
        0 => JobState::Queued,
        1 => JobState::Running,
        2 => JobState::Succeeded,
        3 => JobState::Failed,
        4 => JobState::Cancelled,
        5 => JobState::TimedOut,
        _ => unreachable!("invalid job state"),
    }
}
