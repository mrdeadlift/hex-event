"""Shared models that mirror the Rust JSON schema."""

from __future__ import annotations

from enum import Enum
from typing import Dict, Optional, Union

from pydantic import BaseModel, Field, ConfigDict


class EventKind(str, Enum):
    KILL = "kill"
    DEATH = "death"
    ASSIST = "assist"
    LEVEL_UP = "levelUp"
    SKILL_LEVEL_UP = "skillLevelUp"
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


class SkillLevelEvent(BaseModel):
    payload_kind: str = Field("playerSkillLevel", alias="payloadKind")
    player: PlayerRef
    ability: str
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
    SkillLevelEvent,
    GoldEvent,
    PhaseEvent,
    HeartbeatEvent,
    CustomEvent,
]


class Event(BaseModel):
    kind: EventKind
    ts: int
    payload: EventPayload

    model_config = ConfigDict(populate_by_name=True)
