"""Shared models that mirror the Rust JSON schema."""

from __future__ import annotations

from enum import Enum
from typing import Dict, Optional, Union

from pydantic import BaseModel, Field


class EventKind(str, Enum):
    KILL = "kill"
    DEATH = "death"
    ASSIST = "assist"
    LEVEL_UP = "levelUp"
    ITEM_ADDED = "itemAdded"
    ITEM_REMOVED = "itemRemoved"
    GOLD_DELTA = "goldDelta"
    RESPAWN = "respawn"
    PHASE_CHANGE = "phaseChange"
    HEARTBEAT = "heartbeat"


class PlayerRef(BaseModel):
    summoner_name: str = Field(..., alias="summonerName")
    team: str
    slot: int


class PlayerEvent(BaseModel):
    payload_kind: str = Field("player", alias="payloadKind")
    player: PlayerRef


class ItemEvent(BaseModel):
    payload_kind: str = Field("playerItem", alias="payloadKind")
    player: PlayerRef
    item_id: int = Field(..., alias="itemId")
    item_name: Optional[str] = Field(default=None, alias="itemName")


class LevelEvent(BaseModel):
    payload_kind: str = Field("playerLevel", alias="payloadKind")
    player: PlayerRef
    level: int


class GoldEvent(BaseModel):
    payload_kind: str = Field("playerGold", alias="payloadKind")
    player: PlayerRef
    delta: int
    total: int


class PhaseEvent(BaseModel):
    payload_kind: str = Field("phase", alias="payloadKind")
    phase: str


class HeartbeatEvent(BaseModel):
    payload_kind: str = Field("heartbeat", alias="payloadKind")
    seq: int


class CustomEvent(BaseModel):
    payload_kind: str = Field("custom", alias="payloadKind")
    data: Dict[str, object]


EventPayload = Union[
    PlayerEvent,
    ItemEvent,
    LevelEvent,
    GoldEvent,
    PhaseEvent,
    HeartbeatEvent,
    CustomEvent,
]


class Event(BaseModel):
    kind: EventKind
    ts: int
    payload: EventPayload

    class Config:
        populate_by_name = True
        use_enum_values = True
