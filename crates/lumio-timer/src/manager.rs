//! Single-thread-owned timer kernel. Advance preflights work before allocation.
use crate::error::{TimerError, TimerResult};
use crate::ids::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotState {
    Unbound,
    Armed,
    Closed,
}
struct ScopeRecord {
    kind: ScopeKind,
    generation: u32,
    active: u32,
    alive: bool,
}
struct QueueItem {
    record: FiringRecord,
    scope: TimerScope,
    valid: bool,
}
struct SlotRecord {
    generation: u32,
    state: SlotState,
    dispatch_id: Option<DispatchId>,
    queue: Vec<QueueItem>,
}
struct TimerRecord {
    scope: TimerScope,
    due: u64,
    interval: Option<u64>,
    sequence: u64,
    slot: CallbackSlot,
    dispatch: DispatchId,
    queued: bool,
}
struct TimerSlot {
    generation: u32,
    record: Option<TimerRecord>,
}

pub struct TimerManager {
    context: u64,
    mode: TimerMode,
    limits: TimerLimits,
    budget: TimerBudget,
    config_error: Option<TimerError>,
    running: bool,
    committed_tick: u64,
    schedules_this_tick: u32,
    next_sequence: u64,
    live: u32,
    pending: usize,
    scopes: BTreeMap<u64, ScopeRecord>,
    timers: Vec<TimerSlot>,
    timer_free: Vec<u32>,
    slots: Vec<SlotRecord>,
    dispatch_table: BTreeSet<DispatchId>,
    trace: SliceTrace,
    diagnostics: Vec<TimerDiagnostic>,
    diagnostics_dropped: u64,
}
impl TimerManager {
    pub fn new(context: u64) -> Self {
        Self::with_mode(context, TimerMode::TickFrame)
    }
    pub fn with_mode(context: u64, mode: TimerMode) -> Self {
        Self::with_mode_and_limits(context, mode, TimerLimits::CONTRACT)
    }
    pub fn with_limits(context: u64, limits: TimerLimits) -> Self {
        Self::with_mode_and_limits(context, TimerMode::TickFrame, limits)
    }
    pub fn with_mode_and_limits(context: u64, mode: TimerMode, limits: TimerLimits) -> Self {
        Self::with_budget(context, mode, limits, TimerBudget::default())
    }
    /// Invalid limits fail all mutating operations rather than enabling a zero
    /// interval loop. try_with_budget provides a fallible constructor.
    pub fn with_budget(
        context: u64,
        mode: TimerMode,
        limits: TimerLimits,
        budget: TimerBudget,
    ) -> Self {
        Self {
            context,
            mode,
            limits,
            budget,
            config_error: limits.validate().and_then(|()| budget.validate()).err(),
            running: true,
            committed_tick: 0,
            schedules_this_tick: 0,
            next_sequence: 1,
            live: 0,
            pending: 0,
            scopes: BTreeMap::new(),
            timers: Vec::new(),
            timer_free: Vec::new(),
            slots: Vec::new(),
            dispatch_table: BTreeSet::new(),
            trace: SliceTrace::new(),
            diagnostics: Vec::new(),
            diagnostics_dropped: 0,
        }
    }
    pub fn try_with_budget(
        context: u64,
        mode: TimerMode,
        limits: TimerLimits,
        budget: TimerBudget,
    ) -> TimerResult<Self> {
        limits.validate()?;
        budget.validate()?;
        Ok(Self::with_budget(context, mode, limits, budget))
    }
    fn ensure_running(&self) -> TimerResult<()> {
        if !self.running {
            return Err(TimerError::ManagerShutdown);
        }
        if let Some(error) = self.config_error {
            return Err(error);
        }
        Ok(())
    }
    pub fn committed_tick(&self) -> u64 {
        self.committed_tick
    }
    pub fn mode(&self) -> TimerMode {
        self.mode
    }
    pub fn is_running(&self) -> bool {
        self.running && self.config_error.is_none()
    }
    pub fn live_timer_count(&self) -> u32 {
        self.live
    }
    pub fn diagnostics(&self) -> &[TimerDiagnostic] {
        &self.diagnostics
    }
    pub fn diagnostics_dropped(&self) -> u64 {
        self.diagnostics_dropped
    }
    pub fn take_diagnostics(&mut self) -> Vec<TimerDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
    pub fn trace(&self) -> &SliceTrace {
        &self.trace
    }
    pub fn clear_trace(&mut self) {
        self.trace.clear();
    }
    pub fn is_scope_alive(&self, id: u64) -> bool {
        self.scopes.get(&id).is_some_and(|s| s.alive)
    }
    pub fn is_dispatch_registered(&self, id: DispatchId) -> bool {
        self.dispatch_table.contains(&id)
    }
    fn diagnose(&mut self, code: TimerError, record: FiringRecord) {
        if self.diagnostics.len() < self.budget.max_diagnostics {
            self.diagnostics.push(TimerDiagnostic {
                code,
                due_tick: Some(record.due_tick),
                schedule_sequence: Some(record.schedule_sequence),
            });
        } else {
            self.diagnostics_dropped = self.diagnostics_dropped.saturating_add(1);
        }
    }
    pub fn shutdown(&mut self) {
        self.running = false;
        for slot in &mut self.slots {
            for item in &mut slot.queue {
                item.valid = false;
            }
        }
        for timer in &mut self.timers {
            timer.record = None;
        }
        for scope in self.scopes.values_mut() {
            scope.active = 0;
        }
        self.live = 0;
    }
    pub fn create_slot(&mut self) -> TimerResult<CallbackSlot> {
        self.ensure_running()?;
        if self.slots.len() >= self.budget.max_slots {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        let index = self.slots.len() as u32;
        self.slots.push(SlotRecord {
            generation: 1,
            state: SlotState::Unbound,
            dispatch_id: None,
            queue: Vec::new(),
        });
        Ok(CallbackSlot::new(index, 1))
    }
    #[cfg(feature = "test-support")]
    pub fn allocate_slot(&mut self) -> CallbackSlot {
        self.create_slot().expect("fixture slot allocation")
    }
    pub fn try_register_dispatch(&mut self, id: DispatchId) -> TimerResult<()> {
        self.ensure_running()?;
        if !self.dispatch_table.contains(&id) && self.dispatch_table.len() >= self.budget.max_slots
        {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        self.dispatch_table.insert(id);
        Ok(())
    }
    #[cfg(feature = "test-support")]
    pub fn register_dispatch(&mut self, id: DispatchId, _target: DispatchTarget) {
        self.try_register_dispatch(id).expect("fixture dispatch");
    }
    pub fn remove_dispatch_binding(&mut self, id: DispatchId) {
        self.dispatch_table.remove(&id);
    }
    pub fn remove_dispatch(&mut self, id: DispatchId) {
        self.remove_dispatch_binding(id);
    }
    pub fn bind_slot(&mut self, slot: CallbackSlot, id: DispatchId) -> TimerResult<()> {
        self.ensure_running()?;
        let rec = self.slot(slot)?;
        if rec.state == SlotState::Closed
            || (rec.state == SlotState::Armed && rec.dispatch_id != Some(id))
        {
            return Err(TimerError::SlotClosed);
        }
        self.try_register_dispatch(id)?;
        self.arm_slot(slot, id)
    }
    pub fn arm_slot(&mut self, slot: CallbackSlot, id: DispatchId) -> TimerResult<()> {
        self.ensure_running()?;
        if !self.dispatch_table.contains(&id) {
            return Err(TimerError::SlotDispatchMismatch);
        }
        let rec = self.slot_mut(slot)?;
        match rec.state {
            SlotState::Unbound => {
                rec.state = SlotState::Armed;
                rec.dispatch_id = Some(id);
                Ok(())
            }
            SlotState::Armed if rec.dispatch_id == Some(id) => Ok(()),
            _ => Err(TimerError::SlotClosed),
        }
    }
    pub fn close_slot(&mut self, slot: CallbackSlot) -> TimerResult<()> {
        self.ensure_running()?;
        let rec = self.slot_mut(slot)?;
        match rec.state {
            SlotState::Unbound => Err(TimerError::SlotUnbound),
            SlotState::Closed => Err(TimerError::SlotClosed),
            SlotState::Armed => {
                rec.state = SlotState::Closed;
                for item in &mut rec.queue {
                    item.valid = false;
                }
                Ok(())
            }
        }
    }
    fn slot(&self, slot: CallbackSlot) -> TimerResult<&SlotRecord> {
        self.slots
            .get(slot.index() as usize)
            .filter(|s| s.generation == slot.generation())
            .ok_or(TimerError::SlotClosed)
    }
    fn slot_mut(&mut self, slot: CallbackSlot) -> TimerResult<&mut SlotRecord> {
        self.slots
            .get_mut(slot.index() as usize)
            .filter(|s| s.generation == slot.generation())
            .ok_or(TimerError::SlotClosed)
    }
    pub fn slot_lifecycle(&self, slot: CallbackSlot) -> TimerResult<SlotLifecycle> {
        Ok(match self.slot(slot)?.state {
            SlotState::Unbound => SlotLifecycle::Unbound,
            SlotState::Armed => SlotLifecycle::Armed,
            SlotState::Closed => SlotLifecycle::Closed,
        })
    }
    pub fn register_scope_from_u8(&mut self, kind: u8, id: u64) -> TimerResult<TimerScope> {
        self.register_scope(id, ScopeKind::from_abi(kind)?)
    }
    pub fn register_scope(&mut self, id: u64, kind: ScopeKind) -> TimerResult<TimerScope> {
        self.ensure_running()?;
        if let Some(rec) = self.scopes.get_mut(&id) {
            if rec.alive && rec.kind != kind {
                return Err(TimerError::ScopeInvalid);
            }
            if !rec.alive {
                rec.generation = bump_generation(rec.generation);
                rec.kind = kind;
                rec.active = 0;
                rec.alive = true;
            }
            return Ok(TimerScope::new(id, kind, rec.generation));
        }
        if self.scopes.len() >= self.budget.max_scopes {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        self.scopes.insert(
            id,
            ScopeRecord {
                kind,
                generation: 1,
                active: 0,
                alive: true,
            },
        );
        Ok(TimerScope::new(id, kind, 1))
    }
    fn resolve_scope(&self, scope: TimerScope) -> TimerResult<&ScopeRecord> {
        let rec = self
            .scopes
            .get(&scope.scope_id())
            .filter(|s| s.alive)
            .ok_or(TimerError::ScopeInvalid)?;
        if rec.generation != scope.generation() {
            return Err(TimerError::ScopeGenerationMismatch);
        }
        if rec.kind != scope.kind() {
            return Err(TimerError::ScopeInvalid);
        }
        Ok(rec)
    }
    fn retire_scope_timers(&mut self, id: u64) {
        for index in 0..self.timers.len() {
            if self.timers[index]
                .record
                .as_ref()
                .is_some_and(|r| r.scope.scope_id() == id)
            {
                self.retire_index(index as u32);
            }
        }
        for slot in &mut self.slots {
            for item in &mut slot.queue {
                if item.scope.scope_id() == id {
                    item.valid = false;
                }
            }
        }
    }
    pub fn teardown_scope(&mut self, id: u64) -> TimerResult<TimerScope> {
        self.ensure_running()?;
        let rec = self
            .scopes
            .get(&id)
            .filter(|s| s.alive)
            .ok_or(TimerError::ScopeInvalid)?;
        let generation = bump_generation(rec.generation);
        self.retire_scope_timers(id);
        let rec = self.scopes.get_mut(&id).expect("scope checked");
        rec.generation = generation;
        Ok(TimerScope::new(id, rec.kind, generation))
    }
    pub fn destroy_scope(&mut self, id: u64) -> TimerResult<()> {
        self.ensure_running()?;
        if !self.is_scope_alive(id) {
            return Err(TimerError::ScopeInvalid);
        }
        self.retire_scope_timers(id);
        self.scopes.get_mut(&id).expect("scope checked").alive = false;
        Ok(())
    }
    pub fn schedule_one_shot(
        &mut self,
        scope: TimerScope,
        due: u64,
        slot: CallbackSlot,
    ) -> TimerResult<TimerHandle> {
        self.schedule(scope, due, None, slot)
    }
    pub fn schedule_repeating(
        &mut self,
        scope: TimerScope,
        due: u64,
        interval: u64,
        slot: CallbackSlot,
    ) -> TimerResult<TimerHandle> {
        self.ensure_running()?;
        let min = if self.mode == TimerMode::WallClock {
            self.limits.min_interval_ms
        } else {
            self.limits.min_interval_ticks
        };
        if interval < min {
            return Err(TimerError::InvalidInterval);
        }
        self.schedule(scope, due, Some(interval), slot)
    }
    fn schedule(
        &mut self,
        scope: TimerScope,
        due: u64,
        interval: Option<u64>,
        slot: CallbackSlot,
    ) -> TimerResult<TimerHandle> {
        self.ensure_running()?;
        let active = self.resolve_scope(scope)?.active;
        let rec = self.slot(slot)?;
        match rec.state {
            SlotState::Unbound => return Err(TimerError::SlotUnbound),
            SlotState::Closed => return Err(TimerError::SlotClosed),
            SlotState::Armed => {}
        }
        let dispatch = rec.dispatch_id.ok_or(TimerError::SlotUnbound)?;
        if due <= self.committed_tick {
            return Err(TimerError::InvalidDueTick);
        }
        let schedule_limit = if self.mode == TimerMode::WallClock {
            self.limits.max_schedules_per_pump
        } else {
            self.limits.max_schedules_per_tick
        };
        if self.schedules_this_tick >= schedule_limit
            || active >= self.limits.max_active_timers_per_scope
            || (self.timer_free.is_empty() && self.timers.len() >= self.budget.max_timers)
        {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        let sequence = self.next_sequence;
        let next = sequence
            .checked_add(1)
            .ok_or(TimerError::ScheduleBudgetExceeded)?;
        let record = TimerRecord {
            scope,
            due,
            interval,
            sequence,
            slot,
            dispatch,
            queued: false,
        };
        let handle = if let Some(index) = self.timer_free.pop() {
            let timer = &mut self.timers[index as usize];
            timer.record = Some(record);
            TimerHandle::new(index, timer.generation, self.context)
        } else {
            let index = self.timers.len() as u32;
            self.timers.push(TimerSlot {
                generation: 1,
                record: Some(record),
            });
            TimerHandle::new(index, 1, self.context)
        };
        self.next_sequence = next;
        self.schedules_this_tick += 1;
        self.live += 1;
        self.scopes
            .get_mut(&scope.scope_id())
            .expect("scope checked")
            .active += 1;
        Ok(handle)
    }
    fn get_timer(&self, handle: TimerHandle) -> TimerResult<&TimerRecord> {
        if handle.context() != self.context {
            return Err(TimerError::StaleHandle);
        }
        self.timers
            .get(handle.index() as usize)
            .filter(|s| s.generation == handle.generation())
            .and_then(|s| s.record.as_ref())
            .ok_or(TimerError::StaleHandle)
    }
    /// Full identity validation is mandatory even for internal error paths.
    fn retire(&mut self, handle: TimerHandle) {
        if self.get_timer(handle).is_ok() {
            self.retire_index(handle.index());
        }
    }
    fn retire_index(&mut self, index: u32) {
        let timer = &mut self.timers[index as usize];
        let Some(record) = timer.record.take() else {
            return;
        };
        if let Some(scope) = self.scopes.get_mut(&record.scope.scope_id()) {
            scope.active = scope.active.saturating_sub(1);
        }
        self.live -= 1;
        timer.generation = bump_generation(timer.generation);
        self.timer_free.push(index);
    }
    pub fn cancel(&mut self, handle: TimerHandle) -> TimerResult<bool> {
        self.ensure_running()?;
        self.get_timer(handle)?;
        for slot in &mut self.slots {
            for item in &mut slot.queue {
                if item.record.handle == handle {
                    item.valid = false;
                }
            }
        }
        self.retire(handle);
        Ok(true)
    }
    pub fn pump(&mut self, now_ms: u64) -> TimerResult<AdvanceReport> {
        self.advance(now_ms)
    }
    /// Reject excessive catch-up atomically. The caller can retry smaller windows;
    /// no tick is committed, firing dropped, or allocation proportional to an
    /// unbounded time jump performed when this budget is exceeded.
    pub fn advance(&mut self, to_tick: u64) -> TimerResult<AdvanceReport> {
        self.ensure_running()?;
        if to_tick < self.committed_tick {
            return Err(TimerError::InvalidDueTick);
        }
        if to_tick == self.committed_tick {
            return Ok(AdvanceReport::default());
        }
        let mut total = 0u64;
        for timer in &self.timers {
            if let Some(rec) = &timer.record {
                if rec.queued || rec.due > to_tick {
                    continue;
                }
                let count = match rec.interval {
                    Some(interval) => (to_tick - rec.due) / interval + 1,
                    None => 1,
                };
                total = total
                    .checked_add(count)
                    .ok_or(TimerError::ScheduleBudgetExceeded)?;
                if total > self.budget.max_firings_per_advance as u64 {
                    return Err(TimerError::ScheduleBudgetExceeded);
                }
            }
        }
        let mut collected = Vec::with_capacity(total as usize);
        let mut next_due = Vec::new();
        for (index, timer) in self.timers.iter().enumerate() {
            let Some(rec) = &timer.record else {
                continue;
            };
            if rec.queued || rec.due > to_tick {
                continue;
            }
            let handle = TimerHandle::new(index as u32, timer.generation, self.context);
            let mut due = rec.due;
            loop {
                collected.push(FiringRecord {
                    handle,
                    due_tick: due,
                    schedule_sequence: rec.sequence,
                    slot_dispatch_id: rec.dispatch,
                });
                let Some(interval) = rec.interval else {
                    break;
                };
                match due.checked_add(interval) {
                    Some(next) if next <= to_tick => due = next,
                    next => {
                        next_due.push((handle, next));
                        break;
                    }
                }
            }
        }
        collected.sort_by_key(|r| (r.due_tick, r.schedule_sequence, r.handle.index()));
        let mut report = AdvanceReport::default();
        for record in collected {
            let Ok(timer) = self.get_timer(record.handle) else {
                continue;
            };
            let slot = timer.slot;
            let scope = timer.scope;
            let one_shot = timer.interval.is_none();
            match self.enqueue(record, scope, slot) {
                Ok(()) => {
                    report.push_firing(record);
                    if one_shot {
                        self.timers[record.handle.index() as usize]
                            .record
                            .as_mut()
                            .expect("live")
                            .queued = true;
                    }
                }
                Err(code) => {
                    report.push_rejection(FiringRejection {
                        handle: record.handle,
                        due_tick: record.due_tick,
                        schedule_sequence: record.schedule_sequence,
                        code,
                    });
                    self.diagnose(code, record);
                    self.retire(record.handle);
                }
            }
        }
        for (handle, next) in next_due {
            if self.get_timer(handle).is_err() {
                continue;
            }
            if let Some(due) = next {
                self.timers[handle.index() as usize]
                    .record
                    .as_mut()
                    .expect("live")
                    .due = due;
            } else {
                self.retire(handle);
            }
        }
        self.committed_tick = to_tick;
        self.schedules_this_tick = 0;
        Ok(report)
    }
    fn enqueue(
        &mut self,
        record: FiringRecord,
        scope: TimerScope,
        slot: CallbackSlot,
    ) -> TimerResult<()> {
        let depth = self.limits.delivery_queue_depth_per_slot;
        if self.pending >= self.budget.max_pending_records {
            return Err(TimerError::SlotQueueFull);
        }
        let rec = self.slot_mut(slot)?;
        match rec.state {
            SlotState::Closed => return Err(TimerError::SlotClosed),
            SlotState::Unbound => return Err(TimerError::SlotUnbound),
            SlotState::Armed => {}
        }
        // Invalid entries still occupy memory until drained and count toward capacity.
        if rec.queue.len() >= depth {
            return Err(TimerError::SlotQueueFull);
        }
        rec.queue.push(QueueItem {
            record,
            scope,
            valid: true,
        });
        self.pending += 1;
        Ok(())
    }
    pub fn pending_record_count(&self) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.state != SlotState::Closed)
            .map(|s| s.queue.iter().filter(|q| q.valid).count())
            .sum::<usize>() as u32
    }
    pub fn drain_records(&mut self) -> TimerResult<Vec<FiringRecord>> {
        self.ensure_running()?;
        Ok(self.drain_internal(false).records().to_vec())
    }
    pub fn drain(&mut self) -> DrainReport {
        self.drain_internal(true)
    }
    fn drain_internal(&mut self, emit_trace: bool) -> DrainReport {
        let mut report = DrainReport::default();
        let mut pending = Vec::with_capacity(self.pending);
        for slot in &mut self.slots {
            for item in std::mem::take(&mut slot.queue) {
                pending.push((slot.state, slot.dispatch_id, item));
            }
        }
        self.pending = 0;
        pending.sort_by_key(|(_, _, item)| {
            (
                item.record.due_tick,
                item.record.schedule_sequence,
                item.record.handle.index(),
            )
        });
        for (state, dispatch, item) in pending {
            let code = if !self.running
                || !item.valid
                || state == SlotState::Closed
                || self.resolve_scope(item.scope).is_err()
            {
                Some(TimerError::LateCompletion)
            } else if dispatch != Some(item.record.slot_dispatch_id)
                || !self.dispatch_table.contains(&item.record.slot_dispatch_id)
            {
                Some(TimerError::SlotDispatchMismatch)
            } else {
                None
            };
            if let Some(code) = code {
                report.push_rejection(FiringRejection {
                    handle: item.record.handle,
                    due_tick: item.record.due_tick,
                    schedule_sequence: item.record.schedule_sequence,
                    code,
                });
                self.retire(item.record.handle);
                continue;
            }
            report.push_record(item.record);
            report.push_delivery(Delivery {
                dispatch_id: item.record.slot_dispatch_id,
                due_tick: item.record.due_tick,
                handle: item.record.handle,
            });
            if emit_trace {
                self.trace.push(SliceTraceEvent::Dispatched {
                    dispatch_id: item.record.slot_dispatch_id,
                    due_tick: item.record.due_tick,
                });
            }
            if self
                .get_timer(item.record.handle)
                .is_ok_and(|r| r.interval.is_none())
            {
                self.retire(item.record.handle);
            }
        }
        report
    }
    #[cfg(feature = "test-support")]
    pub fn force_timer_generation(&mut self, handle: TimerHandle, generation: u32) -> TimerHandle {
        if self.get_timer(handle).is_ok() {
            self.timers[handle.index() as usize].generation = generation;
        }
        TimerHandle::new(handle.index(), generation, handle.context())
    }
    #[cfg(feature = "test-support")]
    pub fn force_scope_generation(&mut self, id: u64, generation: u32) {
        if let Some(scope) = self.scopes.get_mut(&id) {
            scope.generation = generation;
        }
    }
}
