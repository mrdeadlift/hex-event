import { EventEmitter } from "node:events";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { existsSync } from "node:fs";
import {
  credentials as grpcCredentials,
  Client,
  type CallOptions,
  type ChannelCredentials,
  type ClientReadableStream,
  type Metadata,
  type ServiceError,
} from "@grpc/grpc-js";
import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import type { Event, EventKind, EventPayload, PlayerRef } from "./types.js";

// Resolve proto path - handle both ts-node (src/) and built (dist/) contexts
function resolveProtoPath(): string {
  // Try relative to current module (works for built dist/)
  const distPath = fileURLToPath(
    new URL("../proto/events.proto", import.meta.url)
  );
  if (existsSync(distPath)) {
    return distPath;
  }

  // Try relative to src/ directory (works for ts-node)
  const currentFile = fileURLToPath(import.meta.url);
  const srcDir = path.dirname(currentFile);
  const srcPath = path.join(srcDir, "..", "proto", "events.proto");
  if (existsSync(srcPath)) {
    return srcPath;
  }

  throw new Error(
    `Could not locate events.proto. Tried: ${distPath}, ${srcPath}`
  );
}

const PROTO_PATH = resolveProtoPath();

const LOADER_OPTIONS: protoLoader.Options = {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
};

const packageDefinition = protoLoader.loadSync(PROTO_PATH, LOADER_OPTIONS);
const grpcDescriptor = grpc.loadPackageDefinition(
  packageDefinition
) as unknown as ProtoGrpcType;
const EventServiceClientCtor = grpcDescriptor?.levents?.v1?.EventService;

if (!EventServiceClientCtor) {
  throw new Error(
    `Failed to load EventService definition from ${path.relative(
      process.cwd(),
      PROTO_PATH
    )}`
  );
}

export interface LiveConfig {
  enabled?: boolean;
  intervalBaseMs?: number;
}

export interface LcuConfig {
  enabled?: boolean;
}

export interface ClientOptions {
  live?: LiveConfig;
  lcu?: LcuConfig;
  endpoint?: string;
  useTls?: boolean;
  connectionTimeoutMs?: number;
  reconnectInitialDelayMs?: number;
  reconnectMaxDelayMs?: number;
}

type EventHandler<T extends EventPayload = EventPayload> = (
  event: Event<T>
) => void;
type ErrorHandler = (error: Error) => void;

interface ResolvedLiveConfig {
  enabled: boolean;
  intervalBaseMs: number;
}

interface ResolvedLcuConfig {
  enabled: boolean;
}

export interface ResolvedClientOptions {
  live: ResolvedLiveConfig;
  lcu: ResolvedLcuConfig;
  endpoint: string;
  useTls: boolean;
  connectionTimeoutMs: number;
  reconnectInitialDelayMs: number;
  reconnectMaxDelayMs: number;
}

interface ProtoGrpcType {
  levents: {
    v1: {
      EventService: EventServiceClientConstructor;
    };
  };
}

interface EventServiceClient extends Client {
  subscribe(
    request: SubscribeRequest,
    metadata?: Metadata,
    options?: Partial<CallOptions>
  ): ClientReadableStream<GrpcEvent>;
  subscribe(
    request: SubscribeRequest,
    options?: Partial<CallOptions>
  ): ClientReadableStream<GrpcEvent>;
}

type EventServiceClientConstructor = new (
  address: string,
  credentials: ChannelCredentials,
  options?: Record<string, unknown>
) => EventServiceClient;

interface SubscribeRequest {
  kinds?: Array<number | string>;
}

interface GrpcPlayerRef {
  summonerName?: string;
  team?: string | number;
  slot?: number | string;
}

interface GrpcPlayerEvent {
  player?: GrpcPlayerRef;
}

interface GrpcItemEvent {
  player?: GrpcPlayerRef;
  itemId?: number | string;
  itemName?: string | null;
}

interface GrpcLevelEvent {
  player?: GrpcPlayerRef;
  level?: number | string;
}

interface GrpcSkillLevelEvent {
  player?: GrpcPlayerRef;
  ability?: number | string;
  level?: number | string;
}

interface GrpcGoldEvent {
  player?: GrpcPlayerRef;
  delta?: number | string;
  total?: number | string;
}

interface GrpcPhaseEvent {
  phase?: string;
}

interface GrpcHeartbeatEvent {
  seq?: number | string;
}

interface GrpcCustomEvent {
  json?: string;
}

interface GrpcEvent {
  kind?: string | number;
  ts?: string | number;
  player?: GrpcPlayerEvent;
  playerItem?: GrpcItemEvent;
  playerLevel?: GrpcLevelEvent;
  playerSkillLevel?: GrpcSkillLevelEvent;
  playerGold?: GrpcGoldEvent;
  phase?: GrpcPhaseEvent;
  heartbeat?: GrpcHeartbeatEvent;
  custom?: GrpcCustomEvent;
}

const EVENT_KIND_FROM_STRING: Record<string, EventKind> = {
  EVENT_KIND_KILL: "kill",
  EVENT_KIND_DEATH: "death",
  EVENT_KIND_ASSIST: "assist",
  EVENT_KIND_LEVEL_UP: "levelUp",
  EVENT_KIND_SKILL_LEVEL_UP: "skillLevelUp",
  EVENT_KIND_ITEM_ADDED: "itemAdded",
  EVENT_KIND_ITEM_REMOVED: "itemRemoved",
  EVENT_KIND_GOLD_DELTA: "goldDelta",
  EVENT_KIND_RESPAWN: "respawn",
  EVENT_KIND_PHASE_CHANGE: "phaseChange",
  EVENT_KIND_HEARTBEAT: "heartbeat",
};

const EVENT_KIND_FROM_NUMBER: Record<number, EventKind> = {
  1: "kill",
  2: "death",
  3: "assist",
  4: "levelUp",
  11: "skillLevelUp",
  5: "itemAdded",
  6: "itemRemoved",
  7: "goldDelta",
  8: "respawn",
  9: "phaseChange",
  10: "heartbeat",
};

const ABILITY_FROM_STRING: Record<string, "q" | "w" | "e" | "r"> = {
  ABILITY_SLOT_UNSPECIFIED: "q",
  ABILITY_SLOT_Q: "q",
  ABILITY_SLOT_W: "w",
  ABILITY_SLOT_E: "e",
  ABILITY_SLOT_R: "r",
};

const ABILITY_FROM_NUMBER: Record<number, "q" | "w" | "e" | "r"> = {
  1: "q",
  2: "w",
  3: "e",
  4: "r",
};

const TEAM_FROM_STRING: Record<string, PlayerRef["team"]> = {
  TEAM_UNSPECIFIED: "neutral",
  TEAM_ORDER: "order",
  TEAM_CHAOS: "chaos",
  TEAM_NEUTRAL: "neutral",
};

const TEAM_FROM_NUMBER: Record<number, PlayerRef["team"]> = {
  0: "neutral",
  1: "order",
  2: "chaos",
  3: "neutral",
};

function resolveOptions(options: ClientOptions): ResolvedClientOptions {
  const live: ResolvedLiveConfig = {
    enabled: options.live?.enabled ?? true,
    intervalBaseMs: options.live?.intervalBaseMs ?? 750,
  };

  const lcu: ResolvedLcuConfig = {
    enabled: options.lcu?.enabled ?? true,
  };

  return {
    live,
    lcu,
    endpoint: options.endpoint ?? "127.0.0.1:50051",
    useTls: options.useTls ?? false,
    connectionTimeoutMs: options.connectionTimeoutMs ?? 5_000,
    reconnectInitialDelayMs: options.reconnectInitialDelayMs ?? 1_000,
    reconnectMaxDelayMs: options.reconnectMaxDelayMs ?? 10_000,
  };
}

function waitForClientReady(client: Client, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const deadline =
      timeoutMs === Infinity
        ? new Date(8640000000000000)
        : new Date(Date.now() + Math.max(timeoutMs, 0));

    client.waitForReady(deadline, (error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

export class LeventsClient {
  private readonly emitter = new EventEmitter();
  private readonly options: ResolvedClientOptions;
  private client?: EventServiceClient;
  private stream?: ClientReadableStream<GrpcEvent>;
  private reconnectTimer?: NodeJS.Timeout;
  private shuttingDown = false;
  private currentReconnectDelayMs: number;

  constructor(options: ClientOptions = {}) {
    this.options = resolveOptions(options);
    this.currentReconnectDelayMs = this.options.reconnectInitialDelayMs;
  }

  get config(): ResolvedClientOptions {
    return this.options;
  }

  async connect(): Promise<void> {
    this.shuttingDown = false;
    const client = this.ensureClient();
    await waitForClientReady(client, this.options.connectionTimeoutMs);
    this.startStream();
  }

  on<T extends EventPayload = EventPayload>(
    kind: EventKind,
    handler: EventHandler<T>
  ): void {
    this.emitter.on(kind, handler as EventHandler);
  }

  off<T extends EventPayload = EventPayload>(
    kind: EventKind,
    handler: EventHandler<T>
  ): void {
    this.emitter.off(kind, handler as EventHandler);
  }

  onError(handler: ErrorHandler): void {
    this.emitter.on("error", handler);
  }

  offError(handler: ErrorHandler): void {
    this.emitter.off("error", handler);
  }

  /**
   * Dispatch an event into the client manually.
   *
   * Useful for local testing before the daemon is running.
   */
  emit<T extends EventPayload = EventPayload>(event: Event<T>): void {
    this.emitter.emit(event.kind, event);
  }

  async disconnect(): Promise<void> {
    this.shuttingDown = true;
    this.clearReconnectTimer();
    this.teardownStream();
    if (this.client) {
      this.client.close();
      this.client = undefined;
    }
  }

  removeAllListeners(): void {
    this.emitter.removeAllListeners();
  }

  private ensureClient(): EventServiceClient {
    if (this.client) {
      return this.client;
    }

    const channelCredentials = this.options.useTls
      ? grpcCredentials.createSsl()
      : grpcCredentials.createInsecure();

    const client = new EventServiceClientCtor(
      this.options.endpoint,
      channelCredentials,
      {
        "grpc.keepalive_time_ms": 30_000,
        "grpc.keepalive_timeout_ms": 5_000,
      }
    );
    this.client = client;
    return client;
  }

  private startStream(): void {
    const client = this.client ?? this.ensureClient();
    if (this.stream || this.shuttingDown) {
      return;
    }

    this.clearReconnectTimer();
    this.currentReconnectDelayMs = this.options.reconnectInitialDelayMs;

    const stream = client.subscribe({ kinds: [] });
    this.stream = stream;

    stream.on("data", (message) => {
      try {
        const event = convertGrpcEvent(message);
        this.emitter.emit(event.kind, event);
      } catch (error) {
        this.notifyError(error);
      }
    });

    stream.on("error", (error) => {
      if (this.shuttingDown) {
        return;
      }
      this.notifyError(error);
      this.teardownStream();
      this.scheduleReconnect();
    });

    stream.on("end", () => {
      if (this.shuttingDown) {
        return;
      }
      this.teardownStream();
      this.scheduleReconnect();
    });

    stream.on("close", () => {
      if (this.shuttingDown) {
        return;
      }
      this.teardownStream();
      this.scheduleReconnect();
    });
  }

  private teardownStream(): void {
    if (!this.stream) {
      return;
    }

    this.stream.removeAllListeners();
    try {
      this.stream.cancel();
    } catch (error) {
      this.notifyError(error);
    }
    this.stream = undefined;
  }

  private scheduleReconnect(): void {
    if (this.shuttingDown || this.reconnectTimer) {
      return;
    }

    const delay = this.currentReconnectDelayMs;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = undefined;
      this.startStream();
    }, delay);

    this.currentReconnectDelayMs = Math.min(
      this.currentReconnectDelayMs * 2,
      this.options.reconnectMaxDelayMs
    );
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
  }

  private notifyError(reason: unknown): void {
    if (this.emitter.listenerCount("error") > 0) {
      const error =
        reason instanceof Error ? reason : new Error(String(reason));
      this.emitter.emit("error", error);
    } else if (reason) {
      // eslint-disable-next-line no-console
      console.error("[levents] gRPC client error", reason);
    }
  }
}

function convertGrpcEvent(message: GrpcEvent): Event {
  const kind = normalizeEventKind(message.kind);
  const ts = normalizeNumber(message.ts, "ts");
  const payload = convertGrpcPayload(message);

  return { kind, ts, payload };
}

function convertGrpcPayload(message: GrpcEvent): EventPayload {
  if (message.playerItem) {
    return {
      payloadKind: "playerItem",
      player: convertGrpcPlayerRef(message.playerItem.player),
      itemId: normalizeNumber(message.playerItem.itemId, "itemId"),
      itemName: message.playerItem.itemName ?? undefined,
    };
  }

  if (message.playerLevel) {
    return {
      payloadKind: "playerLevel",
      player: convertGrpcPlayerRef(message.playerLevel.player),
      level: normalizeNumber(message.playerLevel.level, "level"),
    };
  }

  if (message.playerSkillLevel) {
    return {
      payloadKind: "playerSkillLevel",
      player: convertGrpcPlayerRef(message.playerSkillLevel.player),
      ability: normalizeAbility(message.playerSkillLevel.ability),
      level: normalizeNumber(message.playerSkillLevel.level, "level"),
    };
  }

  if (message.playerGold) {
    return {
      payloadKind: "playerGold",
      player: convertGrpcPlayerRef(message.playerGold.player),
      delta: normalizeNumber(message.playerGold.delta, "delta"),
      total: normalizeNumber(message.playerGold.total, "total"),
    };
  }

  if (message.player) {
    return {
      payloadKind: "player",
      player: convertGrpcPlayerRef(message.player.player),
    };
  }

  if (message.phase) {
    const phase = message.phase.phase;
    if (phase === undefined) {
      throw new Error("Missing phase value on event payload");
    }
    return {
      payloadKind: "phase",
      phase,
    };
  }

  if (message.heartbeat) {
    return {
      payloadKind: "heartbeat",
      seq: normalizeNumber(message.heartbeat.seq, "seq"),
    };
  }

  if (message.custom) {
    return {
      payloadKind: "custom",
      data: parseCustomPayload(message.custom),
    };
  }

  throw new Error("Received event without a recognised payload");
}

function convertGrpcPlayerRef(player?: GrpcPlayerRef): PlayerRef {
  if (!player) {
    throw new Error("Missing player information on event payload");
  }

  return {
    summonerName: player.summonerName ?? "",
    team: normalizeTeam(player.team),
    slot: normalizeNumber(player.slot, "slot"),
  };
}

function normalizeEventKind(value: string | number | undefined): EventKind {
  if (typeof value === "string") {
    const result = EVENT_KIND_FROM_STRING[value];
    if (result) {
      return result;
    }
  } else if (typeof value === "number") {
    const result = EVENT_KIND_FROM_NUMBER[value];
    if (result) {
      return result;
    }
  }

  throw new Error(`Unsupported event kind: ${value as string}`);
}

function normalizeTeam(value: string | number | undefined): PlayerRef["team"] {
  if (typeof value === "string") {
    const result = TEAM_FROM_STRING[value];
    if (result) {
      return result;
    }
  } else if (typeof value === "number") {
    const result = TEAM_FROM_NUMBER[value];
    if (result) {
      return result;
    }
  }

  throw new Error(`Unsupported team value: ${value as string}`);
}

function normalizeNumber(
  value: string | number | undefined,
  label: string
): number {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (!Number.isNaN(parsed)) {
      return parsed;
    }
  }
  throw new Error(`Invalid numeric value for ${label}: ${value as string}`);
}

function parseCustomPayload(payload: GrpcCustomEvent): Record<string, unknown> {
  if (!payload.json) {
    return {};
  }

  try {
    const parsed = JSON.parse(payload.json);
    if (parsed && typeof parsed === "object") {
      return parsed as Record<string, unknown>;
    }
    return {};
  } catch (error) {
    throw new Error("Failed to parse custom payload JSON", { cause: error });
  }
}

function normalizeAbility(
  value: string | number | undefined
): "q" | "w" | "e" | "r" {
  if (typeof value === "string") {
    const result = ABILITY_FROM_STRING[value];
    if (result) {
      return result;
    }
  } else if (typeof value === "number") {
    const result = ABILITY_FROM_NUMBER[value];
    if (result) {
      return result;
    }
  }
  // Default to 'q' if undefined or unknown, but this should not happen.
  return "q";
}

export function createClient(options: ClientOptions = {}): LeventsClient {
  return new LeventsClient(options);
}
