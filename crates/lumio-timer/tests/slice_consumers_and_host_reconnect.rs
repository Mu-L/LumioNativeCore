//! Consumer-owned policies use generic timer delivery, not production business events.

use lumio_timer::{
    BOT_CHAT_CADENCE_DISPATCH, BOT_CHAT_CADENCE_TICKS, DispatchId, DispatchTarget,
    RECONNECT_RETENTION_DISPATCH, RECONNECT_RETENTION_MS, SERVER_WORLD_HEARTBEAT_DISPATCH,
    SERVER_WORLD_HEARTBEAT_TICKS, ScopeKind, SliceTraceEvent, TimerManager, TimerMode,
};

fn schedule_slice_repeating(
    manager: &mut TimerManager,
    scope_id: u64,
    kind: ScopeKind,
    dispatch: DispatchId,
    interval: u64,
) {
    manager.register_dispatch(dispatch, DispatchTarget::Registered);
    let scope = manager.register_scope(scope_id, kind).expect("scope");
    let slot = manager.create_slot().expect("slot");
    manager.bind_slot(slot, dispatch).expect("bind");
    let first_due = manager.committed_tick().saturating_add(interval);
    manager
        .schedule_repeating(scope, first_due, interval, slot)
        .expect("schedule repeating");
}

#[test]
fn bot_chat_cadence_runs_on_tick_frame_kernel() {
    assert_eq!(BOT_CHAT_CADENCE_TICKS, 5);
    let mut manager = TimerManager::new(1);
    schedule_slice_repeating(
        &mut manager,
        7,
        ScopeKind::Adapter,
        BOT_CHAT_CADENCE_DISPATCH,
        BOT_CHAT_CADENCE_TICKS,
    );
    manager.advance(20).expect("advance");
    let delivered = manager.drain();
    assert_eq!(delivered.delivered().len(), 4);
    assert_eq!(manager.trace().bot_utterance_ticks(), [5, 10, 15, 20]);
    assert!(manager.trace().events().iter().any(|event| matches!(
        event,
        SliceTraceEvent::Dispatched { dispatch_id, due_tick: 5 }
            if *dispatch_id == BOT_CHAT_CADENCE_DISPATCH
    )));
}

#[test]
fn server_periodic_task_runs_on_tick_frame_kernel() {
    assert_eq!(SERVER_WORLD_HEARTBEAT_TICKS, 10);
    let mut manager = TimerManager::new(2);
    schedule_slice_repeating(
        &mut manager,
        1,
        ScopeKind::World,
        SERVER_WORLD_HEARTBEAT_DISPATCH,
        SERVER_WORLD_HEARTBEAT_TICKS,
    );
    manager.advance(20).expect("advance");
    let delivered = manager.drain();
    assert_eq!(delivered.delivered().len(), 2);
    assert_eq!(manager.trace().server_checkpoint_ticks(), [10, 20]);
    assert_eq!(manager.live_timer_count(), 1);
    assert!(manager.trace().events().iter().any(|event| matches!(
        event,
        SliceTraceEvent::Dispatched { dispatch_id, due_tick: 10 }
            if *dispatch_id == SERVER_WORLD_HEARTBEAT_DISPATCH
    )));
}

#[test]
fn reconnect_deadline_runs_on_kernel_wall_clock() {
    let mut wall = TimerManager::with_mode(3, TimerMode::WallClock);
    let mut ticks_client = TimerManager::new(1);
    let mut ticks_server = TimerManager::new(2);
    schedule_slice_repeating(
        &mut ticks_client,
        1,
        ScopeKind::Adapter,
        BOT_CHAT_CADENCE_DISPATCH,
        BOT_CHAT_CADENCE_TICKS,
    );
    schedule_slice_repeating(
        &mut ticks_server,
        1,
        ScopeKind::World,
        SERVER_WORLD_HEARTBEAT_DISPATCH,
        SERVER_WORLD_HEARTBEAT_TICKS,
    );
    let wall_scope = wall
        .register_scope(1, ScopeKind::Session)
        .expect("wall scope");
    let wall_slot = wall.create_slot().expect("wall slot");
    wall.bind_slot(wall_slot, RECONNECT_RETENTION_DISPATCH)
        .expect("bind reconnect");
    let handle = wall
        .schedule_one_shot(wall_scope, RECONNECT_RETENTION_MS, wall_slot)
        .expect("reconnect window");
    ticks_client.advance(20).expect("native client ticks");
    ticks_client.drain();
    ticks_server.advance(20).expect("native server ticks");
    ticks_server.drain();
    let early = wall
        .pump(RECONNECT_RETENTION_MS - 1)
        .expect("pump before five minutes");
    assert!(early.firings().is_empty());
    assert!(wall.drain_records().expect("drain early").is_empty());
    assert!(!ticks_client.trace().bot_utterance_ticks().is_empty());
    assert!(!ticks_server.trace().server_checkpoint_ticks().is_empty());
    let expired = wall.pump(RECONNECT_RETENTION_MS).expect("pump at 300s");
    assert_eq!(expired.firings().len(), 1);
    assert_eq!(expired.firings()[0].handle, handle);
    assert_eq!(
        expired.firings()[0].slot_dispatch_id,
        RECONNECT_RETENTION_DISPATCH
    );
    let records = wall.drain_records().expect("drain reconnect");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].due_tick, RECONNECT_RETENTION_MS);
    assert_eq!(wall.cancel(handle), Err(lumio_timer::TimerError::StaleHandle));
    assert!(
        ticks_client
            .trace()
            .events()
            .iter()
            .chain(ticks_server.trace().events())
            .all(|event| matches!(
                event,
                SliceTraceEvent::Dispatched { dispatch_id, .. }
                    if *dispatch_id != RECONNECT_RETENTION_DISPATCH
            ))
    );
}
