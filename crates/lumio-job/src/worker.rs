//! Bounded executable jobs. Locks protect scheduling, never kernel execution.
//! Results retain their byte reservation until the caller drops JobResult.
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::{self, JoinHandle};

use crate::completion::JobCompletion;
use crate::id::{JobId, OperationId};
use crate::operation::OperationRegistry;
use crate::state::JobState;
use lumio_kernel::context::{
    CancelReason, ContextResource, KernelContext, QuiesceReport, QuiesceState,
};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use lumio_kernel::handle::ContextKey;
use lumio_kernel::memory::MemoryBudget;
use lumio_platform::{Deadline, MonotonicClock};

static NEXT_SYSTEM: AtomicU64 = AtomicU64::new(1);
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
fn error(category: ErrorCategory) -> KernelError {
    KernelError::new(category, ErrorDetail::None)
}
fn next_id(counter: &AtomicU64) -> KernelResult<u64> {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
        .map_err(|_| error(ErrorCategory::CapacityExceeded))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSystemConfig {
    pub queue_capacity: usize,
    /// Zero selects deterministic caller-driven pump mode.
    pub worker_count: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobRequest {
    pub operation: OperationId,
    pub deadline: Deadline,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct JobHandle {
    id: JobId,
    system: u64,
    context: ContextKey,
}
impl JobHandle {
    pub fn id(self) -> JobId {
        self.id
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSnapshot {
    pub id: JobId,
    pub state: JobState,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    Cancelled { previous: JobState },
    Requested,
    AlreadyTerminal(JobState),
}

/// Cooperative cancellation and deadline port passed only to Rust kernels.
#[derive(Clone)]
pub struct JobExecution {
    cancelled: Arc<AtomicBool>,
    deadline: Deadline,
    clock: Arc<dyn MonotonicClock>,
}
impl JobExecution {
    pub fn is_cancel_requested(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.deadline_exceeded()
    }
    pub fn deadline_exceeded(&self) -> bool {
        self.deadline.is_expired(self.clock.now())
    }
    pub fn check_cancelled(&self) -> KernelResult<()> {
        if self.deadline_exceeded() {
            return Err(error(ErrorCategory::TimedOut));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(error(ErrorCategory::Cancelled));
        }
        Ok(())
    }
}

struct ByteReservation {
    budget: Arc<MemoryBudget>,
    bytes: u64,
}
impl Drop for ByteReservation {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

/// Taking a result reaps the job handle. Bytes are owned by this value, not by
/// a callback, a temporary scratch buffer, or a Context that can disappear.
pub struct JobResult {
    pub id: JobId,
    pub state: JobState,
    pub deadline_exceeded: bool,
    pub error: Option<KernelError>,
    output: Vec<u8>,
    _reservation: ByteReservation,
}
impl JobResult {
    pub fn bytes(&self) -> &[u8] {
        &self.output
    }
}
impl fmt::Debug for JobResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobResult")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("bytes", &self.output.len())
            .field("error", &self.error)
            .finish()
    }
}

struct JobRecord {
    request: JobRequest,
    state: JobState,
    control: JobExecution,
    input: Vec<u8>,
    output: Vec<u8>,
    output_len: usize,
    error: Option<KernelError>,
    deadline_exceeded: bool,
    reservation: ByteReservation,
}
struct SchedulerState {
    accepting: bool,
    jobs: HashMap<JobId, JobRecord>,
    queue: VecDeque<JobId>,
    ready: VecDeque<JobId>,
    running: usize,
}
struct Shared {
    state: Mutex<SchedulerState>,
    wake: Condvar,
    registry: Arc<OperationRegistry>,
    max_running: usize,
}
struct Work {
    id: JobId,
    operation: OperationId,
    control: JobExecution,
    input: Vec<u8>,
    output: Vec<u8>,
}

impl Shared {
    fn take_work(&self, state: &mut SchedulerState) -> Option<Work> {
        if !state.accepting || state.running >= self.max_running {
            return None;
        }
        while let Some(id) = state.queue.pop_front() {
            let Some(record) = state.jobs.get_mut(&id) else {
                continue;
            };
            if record.state != JobState::Queued {
                continue;
            }
            record.state = JobState::Running;
            state.running += 1;
            return Some(Work {
                id,
                operation: record.request.operation,
                control: record.control.clone(),
                input: std::mem::take(&mut record.input),
                output: std::mem::take(&mut record.output),
            });
        }
        None
    }
    fn execute(&self, mut work: Work) {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            work.control.check_cancelled()?;
            let kernel = self
                .registry
                .get(work.operation)
                .ok_or_else(|| error(ErrorCategory::CapabilityUnavailable))?;
            let written = kernel.execute(&work.input, &mut work.output, &work.control)?;
            if written > work.output.len() {
                return Err(error(ErrorCategory::InternalInvariant));
            }
            Ok(written)
        }))
        .unwrap_or(Err(error(ErrorCategory::PanicBoundary)));
        let exceeded = work.control.deadline_exceeded();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(record) = state.jobs.get_mut(&work.id) {
            record.output = std::mem::take(&mut work.output);
            record.deadline_exceeded = exceeded;
            match outcome {
                Ok(written) => {
                    record.output_len = written;
                    record.state = JobState::Succeeded;
                }
                Err(failure) => {
                    record.state = if matches!(
                        failure.category(),
                        ErrorCategory::Cancelled | ErrorCategory::TimedOut
                    ) {
                        JobState::Cancelled
                    } else {
                        JobState::Failed
                    };
                    record.error = Some(failure);
                }
            }
            state.ready.push_back(work.id);
        }
        state.running -= 1;
        drop(state);
        self.wake.notify_all();
    }
    fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.accepting = false;
        for record in state.jobs.values() {
            record.control.cancelled.store(true, Ordering::Release);
        }
        while let Some(id) = state.queue.pop_front() {
            if let Some(record) = state.jobs.get_mut(&id) {
                record.state = JobState::Cancelled;
                record.error = Some(error(ErrorCategory::Cancelled));
                state.ready.push_back(id);
            }
        }
        drop(state);
        self.wake.notify_all();
    }
}
fn worker_loop(shared: Arc<Shared>) {
    loop {
        let work = {
            let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
            loop {
                if !state.accepting {
                    return;
                }
                if let Some(work) = shared.take_work(&mut state) {
                    break work;
                }
                state = shared.wake.wait(state).unwrap_or_else(|p| p.into_inner());
            }
        };
        shared.execute(work);
    }
}
fn zeroed(bytes: usize) -> KernelResult<Vec<u8>> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(bytes)
        .map_err(|_| error(ErrorCategory::CapacityExceeded))?;
    buffer.resize(bytes, 0);
    Ok(buffer)
}

pub struct JobSystem {
    context: Weak<KernelContext>,
    context_key: ContextKey,
    system_id: u64,
    clock: Arc<dyn MonotonicClock>,
    budget: Arc<MemoryBudget>,
    queue_capacity: usize,
    max_live: usize,
    shared: Arc<Shared>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}
impl JobSystem {
    pub fn create(
        context: Arc<KernelContext>,
        config: JobSystemConfig,
        registry: Arc<OperationRegistry>,
        clock: Arc<dyn MonotonicClock>,
    ) -> KernelResult<Arc<Self>> {
        let limits = context.config().limits;
        if config.queue_capacity == 0
            || config.queue_capacity > limits.max_jobs_queued as usize
            || config.worker_count > limits.max_jobs_running as usize
        {
            return Err(error(ErrorCategory::InvalidArgument));
        }
        context.ensure_accepting_work()?;
        let shared = Arc::new(Shared {
            state: Mutex::new(SchedulerState {
                accepting: true,
                jobs: HashMap::new(),
                queue: VecDeque::new(),
                ready: VecDeque::new(),
                running: 0,
            }),
            wake: Condvar::new(),
            registry,
            max_running: config.worker_count.max(1),
        });
        let system = Arc::new(Self {
            context: Arc::downgrade(&context),
            context_key: context.key(),
            system_id: next_id(&NEXT_SYSTEM)?,
            clock,
            budget: context.memory_budget(),
            queue_capacity: config.queue_capacity,
            max_live: limits.max_handles.min(limits.max_completion_items) as usize,
            shared,
            workers: Mutex::new(Vec::new()),
        });
        for index in 0..config.worker_count {
            let worker_shared = Arc::clone(&system.shared);
            match thread::Builder::new()
                .name(format!("lumio-job-{index}"))
                .spawn(move || worker_loop(worker_shared))
            {
                Ok(handle) => system
                    .workers
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push(handle),
                Err(_) => {
                    system.shared.stop();
                    return Err(error(ErrorCategory::CapacityExceeded));
                }
            }
        }
        // The resource owns only a Weak context, so this registration has no Arc cycle.
        if let Err(failure) =
            context.register_resource(Arc::clone(&system) as Arc<dyn ContextResource>)
        {
            system.shared.stop();
            return Err(failure);
        }
        Ok(system)
    }
    pub fn submit(&self, request: JobRequest) -> KernelResult<JobHandle> {
        self.submit_input(request, &[], 0)
    }
    /// Input is copied; output capacity is reserved before publication. Completed
    /// but unreaped jobs still consume capacity and therefore apply backpressure.
    pub fn submit_input(
        &self,
        request: JobRequest,
        input: &[u8],
        output_capacity: usize,
    ) -> KernelResult<JobHandle> {
        let context = self
            .context
            .upgrade()
            .ok_or_else(|| error(ErrorCategory::ContextDestroyed))?;
        let _admission = context.admit_work()?;
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state.accepting {
            return Err(error(ErrorCategory::ContextClosing));
        }
        if state.queue.len() >= self.queue_capacity || state.jobs.len() >= self.max_live {
            return Err(error(ErrorCategory::CapacityExceeded));
        }
        if self.shared.registry.get(request.operation).is_none() {
            return Err(error(ErrorCategory::CapabilityUnavailable));
        }
        let bytes = input
            .len()
            .checked_add(output_capacity)
            .ok_or_else(|| error(ErrorCategory::CapacityExceeded))? as u64;
        self.budget.try_reserve(bytes)?;
        let reservation = ByteReservation {
            budget: Arc::clone(&self.budget),
            bytes,
        };
        let mut owned_input = zeroed(input.len())?;
        owned_input.copy_from_slice(input);
        let output = zeroed(output_capacity)?;
        let id = JobId::from_raw(next_id(&NEXT_JOB)?);
        let control = JobExecution {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: request.deadline,
            clock: Arc::clone(&self.clock),
        };
        state.jobs.insert(
            id,
            JobRecord {
                request,
                state: JobState::Queued,
                control,
                input: owned_input,
                output,
                output_len: 0,
                error: None,
                deadline_exceeded: false,
                reservation,
            },
        );
        state.queue.push_back(id);
        drop(state);
        self.shared.wake.notify_one();
        Ok(JobHandle {
            id,
            system: self.system_id,
            context: self.context_key,
        })
    }
    fn validate_handle(&self, handle: JobHandle) -> KernelResult<()> {
        if handle.context != self.context_key {
            return Err(error(ErrorCategory::WrongContext));
        }
        if handle.system != self.system_id {
            return Err(error(ErrorCategory::InvalidHandle));
        }
        Ok(())
    }
    pub fn poll(&self, handle: JobHandle) -> KernelResult<JobSnapshot> {
        self.validate_handle(handle)?;
        let state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        let record = state
            .jobs
            .get(&handle.id)
            .ok_or_else(|| error(ErrorCategory::InvalidHandle))?;
        Ok(JobSnapshot {
            id: handle.id,
            state: record.state,
        })
    }
    pub fn cancel(&self, handle: JobHandle) -> KernelResult<CancelOutcome> {
        self.validate_handle(handle)?;
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        let record = state
            .jobs
            .get_mut(&handle.id)
            .ok_or_else(|| error(ErrorCategory::InvalidHandle))?;
        let previous = record.state;
        if previous.is_terminal() {
            return Ok(CancelOutcome::AlreadyTerminal(previous));
        }
        record.control.cancelled.store(true, Ordering::Release);
        if previous == JobState::Running {
            return Ok(CancelOutcome::Requested);
        }
        record.state = JobState::Cancelled;
        record.error = Some(error(ErrorCategory::Cancelled));
        state.queue.retain(|id| *id != handle.id);
        state.ready.push_back(handle.id);
        Ok(CancelOutcome::Cancelled { previous })
    }
    pub fn pump_one(&self) -> bool {
        let work = {
            let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
            self.shared.take_work(&mut state)
        };
        match work {
            Some(work) => {
                self.shared.execute(work);
                true
            }
            None => false,
        }
    }
    pub fn drain_completions(&self, out: &mut [JobCompletion]) -> usize {
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        state.ready.make_contiguous().sort_by_key(|id| id.raw());
        let count = out.len().min(state.ready.len());
        for slot in &mut out[..count] {
            let id = state.ready.pop_front().expect("count checked");
            *slot = JobCompletion {
                id,
                state: state.jobs[&id].state,
            };
        }
        count
    }
    /// Reap once. Invalidates handle and removes all scheduler metadata. A running
    /// cancellation request is not terminal and cannot release its input lease.
    pub fn take_result(&self, handle: JobHandle) -> KernelResult<JobResult> {
        self.validate_handle(handle)?;
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        let record = state
            .jobs
            .get(&handle.id)
            .ok_or_else(|| error(ErrorCategory::InvalidHandle))?;
        if !record.state.is_terminal() {
            return Err(error(ErrorCategory::InvalidArgument));
        }
        let mut record = state.jobs.remove(&handle.id).expect("record checked");
        state.ready.retain(|id| *id != handle.id);
        drop(state);
        record.output.truncate(record.output_len);
        Ok(JobResult {
            id: handle.id,
            state: record.state,
            deadline_exceeded: record.deadline_exceeded,
            error: record.error,
            output: record.output,
            _reservation: record.reservation,
        })
    }
    pub fn retained_jobs(&self) -> usize {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .jobs
            .len()
    }
}
impl ContextResource for JobSystem {
    fn name(&self) -> &'static str {
        "job-system"
    }
    fn cancel_requested(&self, _reason: CancelReason) {
        self.shared.stop();
    }
    fn quiesce(&self, _deadline: Deadline) -> KernelResult<QuiesceReport> {
        self.shared.stop();
        let running = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .running;
        let workers = self.workers.lock().unwrap_or_else(|p| p.into_inner());
        let remaining = running
            + workers
                .iter()
                .filter(|worker| !worker.is_finished())
                .count();
        Ok(QuiesceReport {
            state: if remaining == 0 {
                QuiesceState::Quiesced
            } else {
                QuiesceState::Pending {
                    remaining: u32::try_from(remaining).unwrap_or(u32::MAX),
                }
            },
        })
    }
    fn destroy(&self) -> KernelResult<()> {
        if self.quiesce(Deadline::NONE)?.state != QuiesceState::Quiesced {
            return Err(error(ErrorCategory::ContextClosing));
        }
        let handles = std::mem::take(&mut *self.workers.lock().unwrap_or_else(|p| p.into_inner()));
        for handle in handles {
            handle
                .join()
                .map_err(|_| error(ErrorCategory::PanicBoundary))?;
        }
        let old = {
            let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
            state.queue.clear();
            state.ready.clear();
            std::mem::take(&mut state.jobs)
        };
        drop(old);
        Ok(())
    }
}
impl Drop for JobSystem {
    fn drop(&mut self) {
        self.shared.stop();
        // Never block Drop on an uncooperative kernel. Worker-owned Shared keeps
        // inputs alive until real termination; explicit close provides the fence.
        let workers = self.workers.get_mut().unwrap_or_else(|p| p.into_inner());
        for handle in workers.drain(..) {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}
