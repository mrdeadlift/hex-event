import { EventEmitter } from 'node:events';
import type { Event, EventKind, EventPayload } from './types.js';

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
}

type EventHandler<T extends EventPayload = EventPayload> = (event: Event<T>) => void;

/**
 * Thin wrapper around the future gRPC transport. For now we rely on an in-memory event emitter
 * so SDK adopters can begin integrating while the daemon implementation matures.
 */
export class LeventsClient {
  private readonly emitter = new EventEmitter();
  private readonly options: Required<ClientOptions>;

  constructor(options: ClientOptions = {}) {
    this.options = {
      live: { enabled: true, intervalBaseMs: 750, ...options.live },
      lcu: { enabled: true, ...options.lcu },
      endpoint: options.endpoint ?? 'http://127.0.0.1:50051'
    };
  }

  get config(): Required<ClientOptions> {
    return this.options;
  }

  async connect(): Promise<void> {
    // Placeholder for establishing a gRPC channel.
    return Promise.resolve();
  }

  on<T extends EventPayload = EventPayload>(kind: EventKind, handler: EventHandler<T>): void {
    this.emitter.on(kind, handler as EventHandler);
  }

  off<T extends EventPayload = EventPayload>(kind: EventKind, handler: EventHandler<T>): void {
    this.emitter.off(kind, handler as EventHandler);
  }

  /**
   * Dispatch an event into the client. Downstream consumers can use this until the transport is
   * wired to the daemon.
   */
  emit<T extends EventPayload = EventPayload>(event: Event<T>): void {
    this.emitter.emit(event.kind, event);
  }

  removeAllListeners(): void {
    this.emitter.removeAllListeners();
  }
}

export function createClient(options: ClientOptions = {}): LeventsClient {
  return new LeventsClient(options);
}
