//! Legacy slice fixtures. Excluded from normal builds; policy belongs to consumers.
use crate::{DispatchId, SliceTrace};
pub const BOT_CHAT_CADENCE_TICKS: u64 = 5;
pub const BOT_CHAT_CADENCE_INTERVAL_TICKS: u64 = BOT_CHAT_CADENCE_TICKS;
pub const BOT_CHAT_CADENCE_DISPATCH: DispatchId = DispatchId::from_raw(100);
pub const SERVER_WORLD_HEARTBEAT_TICKS: u64 = 10;
pub const SERVER_PERIODIC_INTERVAL_TICKS: u64 = SERVER_WORLD_HEARTBEAT_TICKS;
pub const SERVER_WORLD_HEARTBEAT_DISPATCH: DispatchId = DispatchId::from_raw(101);
pub const RECONNECT_RETENTION_SECS: u64 = 300;
pub const RECONNECT_RETENTION_MS: u64 = RECONNECT_RETENTION_SECS * 1000;
pub const RECONNECT_RETENTION_DISPATCH: DispatchId = DispatchId::from_raw(102);
impl DispatchId {
    pub const BOT_CHAT_CADENCE: Self = BOT_CHAT_CADENCE_DISPATCH;
    pub const SERVER_PERIODIC_CHECKPOINT: Self = SERVER_WORLD_HEARTBEAT_DISPATCH;
    pub const RECONNECT_RETENTION: Self = RECONNECT_RETENTION_DISPATCH;
    pub fn from_static(id: &'static str) -> Self {
        Self::from_raw(match id {
            "test.slot" => 1,
            "test.slot.b" => 2,
            "test.other" => 3,
            "client.bot_chat_cadence" => 100,
            "server.world_authority_heartbeat" => 101,
            "server.reconnect_retention" => 102,
            _ => panic!("unknown fixture dispatch"),
        })
    }
    pub const fn as_str(self) -> &'static str {
        match self.raw() {
            1 => "test.slot",
            2 => "test.slot.b",
            3 => "test.other",
            100 => "client.bot_chat_cadence",
            101 => "server.world_authority_heartbeat",
            102 => "server.reconnect_retention",
            _ => "test.unknown",
        }
    }
}
impl SliceTrace {
    pub fn bot_utterance_ticks(&self) -> Vec<u64> {
        self.dispatched_ticks(BOT_CHAT_CADENCE_DISPATCH)
    }
    pub fn server_checkpoint_ticks(&self) -> Vec<u64> {
        self.dispatched_ticks(SERVER_WORLD_HEARTBEAT_DISPATCH)
    }
    pub fn ticks_named(&self, name: &str) -> Vec<u64> {
        match name {
            "bot_chat_cadence" => self.bot_utterance_ticks(),
            "world_authority_heartbeat" => self.server_checkpoint_ticks(),
            _ => Vec::new(),
        }
    }
}

impl crate::TimerScope {
    pub const fn debug_unregistered(id: u64, kind: crate::ScopeKind, generation: u32) -> Self {
        Self::new(id, kind, generation)
    }
}
