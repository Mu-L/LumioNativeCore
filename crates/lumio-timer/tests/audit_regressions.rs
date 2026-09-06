//! Default-feature regressions for generation safety and bounded work.
use lumio_timer::*;
fn setup(limits: TimerLimits) -> (TimerManager, TimerScope, CallbackSlot) {
    let mut manager = TimerManager::with_limits(1, limits);
    let scope = manager.register_scope(1, ScopeKind::World).unwrap();
    let slot = manager.create_slot().unwrap();
    manager.bind_slot(slot, DispatchId::from_raw(7)).unwrap();
    (manager, scope, slot)
}
#[test]
fn stale_queued_error_cannot_retire_reused_timer_slot() {
    let limits = TimerLimits {
        delivery_queue_depth_per_slot: 1,
        ..TimerLimits::CONTRACT
    };
    let (mut manager, scope, slot) = setup(limits);
    let old = manager.schedule_repeating(scope, 1, 1, slot).unwrap();
    let first = manager.advance(2).unwrap();
    assert_eq!(first.firings().len(), 1);
    assert_eq!(first.rejections()[0].code, TimerError::SlotQueueFull);
    let next_slot = manager.create_slot().unwrap();
    manager
        .bind_slot(next_slot, DispatchId::from_raw(8))
        .unwrap();
    let new = manager.schedule_one_shot(scope, 3, next_slot).unwrap();
    assert_eq!(old.index(), new.index());
    assert_ne!(old.generation(), new.generation());
    manager.remove_dispatch_binding(DispatchId::from_raw(7));
    assert_eq!(
        manager.drain().rejections()[0].code,
        TimerError::SlotDispatchMismatch
    );
    assert_eq!(manager.cancel(new), Ok(true));
}
#[test]
fn excessive_catchup_is_rejected_before_commit_or_queue_changes() {
    let budget = TimerBudget {
        max_firings_per_advance: 4,
        ..TimerBudget::default()
    };
    let mut manager =
        TimerManager::try_with_budget(1, TimerMode::WallClock, TimerLimits::CONTRACT, budget)
            .unwrap();
    let scope = manager.register_scope(1, ScopeKind::Adapter).unwrap();
    let slot = manager.create_slot().unwrap();
    manager.bind_slot(slot, DispatchId::from_raw(1)).unwrap();
    manager.schedule_repeating(scope, 1, 1, slot).unwrap();
    assert_eq!(
        manager.pump(86_400_000),
        Err(TimerError::ScheduleBudgetExceeded)
    );
    assert_eq!(manager.committed_tick(), 0);
    assert_eq!(manager.pending_record_count(), 0);
    assert_eq!(manager.live_timer_count(), 1);
    assert_eq!(manager.pump(4).unwrap().firings().len(), 4);
}
#[test]
fn zero_interval_configuration_cannot_enter_advance_loop() {
    let mut manager = TimerManager::with_limits(
        1,
        TimerLimits {
            min_interval_ticks: 0,
            ..TimerLimits::CONTRACT
        },
    );
    assert_eq!(manager.create_slot(), Err(TimerError::InvalidInterval));
    assert_eq!(manager.advance(100), Err(TimerError::InvalidInterval));
}
#[test]
fn invalid_queue_entries_count_until_drained() {
    let (mut manager, scope, slot) = setup(TimerLimits {
        delivery_queue_depth_per_slot: 1,
        ..TimerLimits::CONTRACT
    });
    let first = manager.schedule_one_shot(scope, 1, slot).unwrap();
    manager.advance(1).unwrap();
    manager.cancel(first).unwrap();
    manager.schedule_one_shot(scope, 2, slot).unwrap();
    assert_eq!(
        manager.advance(2).unwrap().rejections()[0].code,
        TimerError::SlotQueueFull
    );
    assert_eq!(manager.drain().rejections().len(), 1);
    manager.schedule_one_shot(scope, 3, slot).unwrap();
    assert_eq!(manager.advance(3).unwrap().firings().len(), 1);
}
#[test]
fn scope_tombstones_and_slots_obey_manager_budget() {
    let budget = TimerBudget {
        max_scopes: 1,
        max_slots: 1,
        ..TimerBudget::default()
    };
    let mut manager =
        TimerManager::try_with_budget(1, TimerMode::TickFrame, TimerLimits::CONTRACT, budget)
            .unwrap();
    let first = manager.register_scope(1, ScopeKind::World).unwrap();
    manager.destroy_scope(1).unwrap();
    assert_eq!(
        manager.register_scope(2, ScopeKind::World),
        Err(TimerError::ScheduleBudgetExceeded)
    );
    let second = manager.register_scope(1, ScopeKind::World).unwrap();
    assert_ne!(first.generation(), second.generation());
    manager.create_slot().unwrap();
    assert_eq!(
        manager.create_slot(),
        Err(TimerError::ScheduleBudgetExceeded)
    );
}
