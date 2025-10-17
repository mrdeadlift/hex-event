//! Shared data structures used across the levents workspace.

pub mod schema;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Millisecond timestamp sourced from the game client.
pub type TimestampMs = u64;

/// Identifies a specific player within the current game session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
pub struct PlayerRef {
    pub summoner_name: String,
    pub team: Team,
    /// Slot index [0, 4] for teammates, [5, 9] for opponents.
    pub slot: u8,
}

/// Teams recognised by the League of Legends client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Order,
    Chaos,
    Neutral,
}

/// Top-level event emitted by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Event {
    pub kind: EventKind,
    pub ts: TimestampMs,
    #[serde(flatten)]
    pub payload: EventPayload,
}

/// Accepted event kinds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Kill,
    Death,
    Assist,
    LevelUp,
    ItemAdded,
    ItemRemoved,
    GoldDelta,
    Respawn,
    PhaseChange,
    Heartbeat,
}

/// Event payload variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(tag = "payloadKind", content = "data", rename_all = "camelCase")]
pub enum EventPayload {
    Player(PlayerEvent),
    PlayerItem(ItemEvent),
    PlayerLevel(LevelEvent),
    PlayerGold(GoldEvent),
    Phase(PhaseEvent),
    Heartbeat(HeartbeatEvent),
    Custom(HashMap<String, serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct PlayerEvent {
    pub player: PlayerRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ItemEvent {
    pub player: PlayerRef,
    pub item_id: u32,
    pub item_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct LevelEvent {
    pub player: PlayerRef,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GoldEvent {
    pub player: PlayerRef,
    pub delta: i32,
    pub total: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct PhaseEvent {
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct HeartbeatEvent {
    pub seq: u64,
}

/// Batch of events emitted in a single poll cycle.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct EventBatch {
    pub events: Vec<Event>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_event() {
        let event = Event {
            kind: EventKind::Kill,
            ts: 1234,
            payload: EventPayload::Player(PlayerEvent {
                player: PlayerRef {
                    summoner_name: "Example".into(),
                    team: Team::Order,
                    slot: 0,
                },
            }),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"kind\":\"Kill\""));
        let back: Event = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind, EventKind::Kill);
    }

    #[test]
    fn schema_includes_event_kind_enum() {
        let schema = crate::schema::event_schema();
        let value = serde_json::to_value(schema).expect("schema to json");
        let enums = value["definitions"]["EventKind"]["enum"]
            .as_array()
            .expect("enum array");
        assert!(enums.iter().any(|value| value == "Kill"));
    }
}
