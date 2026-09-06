//! Bounded standalone completion buffer. IDs must be published monotonically.
//! Active leases plus a bounded recent-release window use O(capacity) space.
//! Older releases return InvalidHandle; the high-water mark prevents republish.
use crate::id::JobId;
use crate::state::JobState;
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCompletion {
    pub id: JobId,
    pub state: JobState,
}
#[derive(Default)]
struct BatchState {
    queue: VecDeque<JobCompletion>,
    leases: HashMap<JobId, bool>,
    released: VecDeque<JobId>,
    high_water: Option<u64>,
}
pub struct CompletionBatch {
    capacity: usize,
    state: Mutex<BatchState>,
}
fn err(category: ErrorCategory) -> KernelError {
    KernelError::new(category, ErrorDetail::None)
}
impl CompletionBatch {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::new(BatchState::default()),
        }
    }
    pub fn publish(&self, completion: JobCompletion) -> KernelResult<()> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !completion.state.is_terminal()
            || state
                .high_water
                .is_some_and(|high| completion.id.raw() <= high)
        {
            return Err(err(ErrorCategory::InvalidArgument));
        }
        if state.leases.len() >= self.capacity {
            return Err(err(ErrorCategory::CapacityExceeded));
        }
        state.queue.push_back(completion);
        state.leases.insert(completion.id, false);
        state.high_water = Some(completion.id.raw());
        Ok(())
    }
    pub fn drain(&self, out: &mut [JobCompletion]) -> KernelResult<usize> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let count = out.len().min(state.queue.len());
        for slot in &mut out[..count] {
            *slot = state.queue.pop_front().expect("count checked");
            if let Some(drained) = state.leases.get_mut(&slot.id) {
                *drained = true;
            }
        }
        Ok(count)
    }
    pub fn release(&self, id: JobId) -> KernelResult<()> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match state.leases.get(&id) {
            Some(true) => {}
            Some(false) => return Err(err(ErrorCategory::InvalidArgument)),
            None if state.released.contains(&id) => {
                return Err(err(ErrorCategory::AlreadyReleased));
            }
            None => return Err(err(ErrorCategory::InvalidHandle)),
        }
        state.leases.remove(&id);
        if state.released.len() == self.capacity {
            state.released.pop_front();
        }
        state.released.push_back(id);
        Ok(())
    }
    pub fn retained_count(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.leases.len() + state.released.len()
    }
}
