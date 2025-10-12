export type EventKind =
  | 'kill'
  | 'death'
  | 'assist'
  | 'levelUp'
  | 'itemAdded'
  | 'itemRemoved'
  | 'goldDelta'
  | 'respawn'
  | 'phaseChange'
  | 'heartbeat';

export interface Timestamped {
  ts: number;
}

export interface PlayerRef {
  summonerName: string;
  team: 'order' | 'chaos' | 'neutral';
  slot: number;
}

export interface PlayerEventPayload {
  payloadKind: 'player';
  player: PlayerRef;
}

export interface ItemEventPayload {
  payloadKind: 'playerItem';
  player: PlayerRef;
  itemId: number;
  itemName?: string;
}

export interface LevelEventPayload {
  payloadKind: 'playerLevel';
  player: PlayerRef;
  level: number;
}

export interface GoldEventPayload {
  payloadKind: 'playerGold';
  player: PlayerRef;
  delta: number;
  total: number;
}

export interface PhaseEventPayload {
  payloadKind: 'phase';
  phase: string;
}

export interface HeartbeatEventPayload {
  payloadKind: 'heartbeat';
  seq: number;
}

export interface CustomEventPayload {
  payloadKind: 'custom';
  data: Record<string, unknown>;
}

export type EventPayload =
  | PlayerEventPayload
  | ItemEventPayload
  | LevelEventPayload
  | GoldEventPayload
  | PhaseEventPayload
  | HeartbeatEventPayload
  | CustomEventPayload;

export interface Event<T extends EventPayload = EventPayload> extends Timestamped {
  kind: EventKind;
  payload: T;
}
