//! Domain-neutral timer identities, budgets and bounded diagnostics.
use crate::error::{TimerError, TimerResult};
pub const MAX_ACTIVE_TIMERS_PER_SCOPE: u32 = 1024;
pub const MAX_SCHEDULES_PER_TICK: u32 = 4096;
pub const MAX_SCHEDULES_PER_PUMP: u32 = 4096;
pub const DELIVERY_QUEUE_DEPTH_PER_SLOT: usize = 256;
pub const MIN_INTERVAL_TICKS: u64 = 1;
pub const MIN_INTERVAL_MS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerMode {
    WallClock = 0,
    TickFrame = 1,
}
impl TimerMode {
    pub const fn from_abi(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::WallClock),
            1 => Some(Self::TickFrame),
            _ => None,
        }
    }
    pub const fn to_abi(self) -> u32 {
        self as u32
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerLimits {
    pub max_active_timers_per_scope: u32,
    pub max_schedules_per_tick: u32,
    pub max_schedules_per_pump: u32,
    pub delivery_queue_depth_per_slot: usize,
    pub min_interval_ticks: u64,
    pub min_interval_ms: u64,
}
impl TimerLimits {
    pub const CONTRACT: Self = Self {
        max_active_timers_per_scope: MAX_ACTIVE_TIMERS_PER_SCOPE,
        max_schedules_per_tick: MAX_SCHEDULES_PER_TICK,
        max_schedules_per_pump: MAX_SCHEDULES_PER_PUMP,
        delivery_queue_depth_per_slot: DELIVERY_QUEUE_DEPTH_PER_SLOT,
        min_interval_ticks: MIN_INTERVAL_TICKS,
        min_interval_ms: MIN_INTERVAL_MS,
    };
    pub(crate) fn validate(self) -> TimerResult<()> {
        if self.min_interval_ticks == 0 || self.min_interval_ms == 0 {
            return Err(TimerError::InvalidInterval);
        }
        if self.max_active_timers_per_scope == 0
            || self.max_schedules_per_tick == 0
            || self.max_schedules_per_pump == 0
            || self.delivery_queue_depth_per_slot == 0
        {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        Ok(())
    }
}
/// Manager-wide bounds, including tombstones, invalid queue entries and staging.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerBudget {
    pub max_scopes: usize,
    pub max_slots: usize,
    pub max_timers: usize,
    pub max_pending_records: usize,
    pub max_firings_per_advance: usize,
    pub max_diagnostics: usize,
}
impl Default for TimerBudget {
    fn default() -> Self {
        Self {
            max_scopes: 4096,
            max_slots: 4096,
            max_timers: 65536,
            max_pending_records: 65536,
            max_firings_per_advance: 16384,
            max_diagnostics: 1024,
        }
    }
}
impl TimerBudget {
    pub(crate) fn validate(self) -> TimerResult<()> {
        if self.max_scopes == 0
            || self.max_slots == 0
            || self.max_timers == 0
            || self.max_slots > u32::MAX as usize
            || self.max_timers > u32::MAX as usize
            || self.max_pending_records == 0
            || self.max_pending_records > u32::MAX as usize
            || self.max_firings_per_advance == 0
        {
            return Err(TimerError::ScheduleBudgetExceeded);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TimerHandle {
    index: u32,
    generation: u32,
    context: u64,
}
impl TimerHandle {
    pub const fn from_abi(index: u32, generation: u32, context: u64) -> Self {
        Self {
            index,
            generation,
            context,
        }
    }
    pub(crate) const fn new(index: u32, generation: u32, context: u64) -> Self {
        Self::from_abi(index, generation, context)
    }
    pub const fn index(self) -> u32 {
        self.index
    }
    pub const fn generation(self) -> u32 {
        self.generation
    }
    pub const fn context(self) -> u64 {
        self.context
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DispatchId(u32);
impl DispatchId {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u32 {
        self.0
    }
}
pub type SlotDispatchId = DispatchId;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchTarget {
    Registered,
    #[cfg(feature = "test-support")]
    BotChatCadence,
    #[cfg(feature = "test-support")]
    ServerPeriodicCheckpoint,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CallbackSlot {
    index: u32,
    generation: u32,
}
impl CallbackSlot {
    pub const fn from_abi(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
        Self::from_abi(index, generation)
    }
    pub const fn index(self) -> u32 {
        self.index
    }
    pub const fn generation(self) -> u32 {
        self.generation
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeKind {
    World,
    Session,
    Adapter,
}
impl ScopeKind {
    pub const fn from_abi(raw: u8) -> TimerResult<Self> {
        match raw {
            0 => Ok(Self::World),
            1 => Ok(Self::Session),
            2 => Ok(Self::Adapter),
            _ => Err(TimerError::ScopeInvalid),
        }
    }
    pub const fn to_abi(self) -> u8 {
        match self {
            Self::World => 0,
            Self::Session => 1,
            Self::Adapter => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerScope {
    pub scope_id: u64,
    pub kind: ScopeKind,
    pub generation: u32,
}
impl TimerScope {
    pub const fn new(scope_id: u64, kind: ScopeKind, generation: u32) -> Self {
        Self {
            scope_id,
            kind,
            generation,
        }
    }
    pub const fn scope_id(self) -> u64 {
        self.scope_id
    }
    pub const fn kind(self) -> ScopeKind {
        self.kind
    }
    pub const fn generation(self) -> u32 {
        self.generation
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerKind {
    OneShot,
    Repeating,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiringRecord {
    pub handle: TimerHandle,
    pub due_tick: u64,
    pub schedule_sequence: u64,
    pub slot_dispatch_id: DispatchId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrainOutcome {
    pub record: FiringRecord,
    pub result: Result<(), TimerError>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiringRejection {
    pub handle: TimerHandle,
    pub due_tick: u64,
    pub schedule_sequence: u64,
    pub code: TimerError,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdvanceReport {
    firings: Vec<FiringRecord>,
    rejections: Vec<FiringRejection>,
}
impl std::ops::Deref for AdvanceReport {
    type Target = [FiringRecord];
    fn deref(&self) -> &Self::Target {
        &self.firings
    }
}
impl AdvanceReport {
    pub fn firings(&self) -> &[FiringRecord] {
        &self.firings
    }
    pub fn rejections(&self) -> &[FiringRejection] {
        &self.rejections
    }
    pub(crate) fn push_firing(&mut self, record: FiringRecord) {
        self.firings.push(record);
    }
    pub(crate) fn push_rejection(&mut self, record: FiringRejection) {
        self.rejections.push(record);
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DrainReport {
    delivered: Vec<Delivery>,
    rejections: Vec<FiringRejection>,
    records: Vec<FiringRecord>,
}
impl DrainReport {
    pub fn delivered(&self) -> &[Delivery] {
        &self.delivered
    }
    pub fn rejections(&self) -> &[FiringRejection] {
        &self.rejections
    }
    pub fn records(&self) -> &[FiringRecord] {
        &self.records
    }
    pub(crate) fn push_delivery(&mut self, delivery: Delivery) {
        self.delivered.push(delivery);
    }
    pub(crate) fn push_record(&mut self, record: FiringRecord) {
        self.records.push(record);
    }
    pub(crate) fn push_rejection(&mut self, rejection: FiringRejection) {
        self.rejections.push(rejection);
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Delivery {
    pub dispatch_id: DispatchId,
    pub due_tick: u64,
    pub handle: TimerHandle,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerDiagnostic {
    pub code: TimerError,
    pub due_tick: Option<u64>,
    pub schedule_sequence: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotLifecycle {
    Unbound,
    Armed,
    Closed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliceTraceEvent {
    Dispatched {
        dispatch_id: DispatchId,
        due_tick: u64,
    },
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SliceTrace {
    events: Vec<SliceTraceEvent>,
    dropped: u64,
}
impl SliceTrace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, event: SliceTraceEvent) {
        if self.events.len() < 1024 {
            self.events.push(event);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
    pub fn events(&self) -> &[SliceTraceEvent] {
        &self.events
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn clear(&mut self) {
        self.events.clear();
        self.dropped = 0;
    }
    pub fn dispatched_ticks(&self, id: DispatchId) -> Vec<u64> {
        self.events
            .iter()
            .filter_map(|event| match event {
                SliceTraceEvent::Dispatched {
                    dispatch_id,
                    due_tick,
                } if *dispatch_id == id => Some(*due_tick),
                _ => None,
            })
            .collect()
    }
}
pub(crate) fn bump_generation(current: u32) -> u32 {
    current
        .checked_add(1)
        .expect("timer generation overflow is fatal")
}
