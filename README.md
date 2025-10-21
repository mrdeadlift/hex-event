# levents

Fast, local event bridge for League of Legends that fuses the Live Client Data REST API (polling) with LCU WebSocket insights, delivered as a Rust daemon with TypeScript and Python SDKs.

The daemon streams normalized events (kill/death/assist, level/skill ups, item changes, gold delta, respawn, phase changes, heartbeat) over gRPC for lightweight consumption in Node and Python apps.

## Repository Layout

- `levents/` – Rust workspace
  - `levents-core` – Live Client poller + LCU WS connector + normalization
  - `levents-daemon` – gRPC server entrypoint
  - `levents-model` – shared event types + JSON schema generator
- `bindings/ts` – TypeScript SDK `@levents/sdk` (gRPC client) with examples in `examples/`
- `bindings/py` – Python SDK `levents-py` (in-memory bus for now; mirrors future gRPC API)
- `doc/` – Product specs and reference material

## Features

- Adaptive Live Client polling with hashing-based diffing (xxh3) and backoff
- LCU WebSocket with lockfile discovery, auto-reconnect, and phase change events
- Single gRPC streaming API: `Subscribe(stream Event)` plus a small control RPC
- Strongly-typed, language-friendly event model shared across Rust/TS/Python

## Quick Start

### Prerequisites

- Rust 1.74+
- Node.js 20+ with `pnpm`
- Python 3.10+

### Run the Rust daemon

```bash
cd levents
cargo check
cargo run --bin levents-daemon
```

Defaults:
- gRPC bind: `127.0.0.1:50051` (override with `LEVENTS_GRPC_ADDR`)
- Live Client base: `https://127.0.0.1:2999`
- LCU lockfile: auto-discovered (override with `LEVENTS_LCU_LOCKFILE`)

### Use the TypeScript SDK

```bash
pnpm install
pnpm --filter @levents/sdk build
pnpm --filter @levents/sdk example:basic
```

Minimal usage (Node):

```ts
import { createClient } from '@levents/sdk';

const client = createClient({ endpoint: '127.0.0.1:50051' });
client.on('kill', (event) => {
  // event.payload.payloadKind === 'player'
  console.log('[kill]', event.payload.player.summonerName);
});
await client.connect();
```

Notes:
- The client auto-reconnects on stream errors and exposes `onError`.
- Event kinds: `kill | death | assist | levelUp | skillLevelUp | itemAdded | itemRemoved | goldDelta | respawn | phaseChange | heartbeat`.
- If the daemon is not running, `@levents/sdk` will try to auto-start `levents-daemon` on first `connect()`:
  - Looks for `LEVENTS_DAEMON_BIN`, then `levents-daemon` in `PATH`, then local build outputs under `levents/target/{release,debug}`.
  - Binds to `endpoint` via env `LEVENTS_GRPC_ADDR`. Disable with `createClient({ autoStartDaemon: false })`.
  - You can pass `daemonPath`, `daemonArgs`, `daemonEnv`, and `daemonReadyTimeoutMs` for finer control.

### Use the Python SDK

```bash
pip install -e bindings/py[dev]
pytest bindings/py
python -m levents watch
```

Current status: the Python package provides an in-memory event bus mirroring the public API and a `typer`-based CLI. It is structured to plug into the same gRPC stream exposed by the daemon in a follow-up iteration.

## gRPC API

- Service: `levents.v1.EventService` (proto in `levents/levents-daemon/proto/events.proto` and mirrored under `bindings/ts/proto/events.proto`)
- Endpoints:
  - `Subscribe(SubscribeRequest) -> (stream Event)` — optional kind filter
  - `Control(ControlRequest) -> ControlResponse` — e.g., `EmitSyntheticKill` for local testing
- Address: `127.0.0.1:50051` by default; override via `LEVENTS_GRPC_ADDR`

Event model highlights:
- `Event { kind, ts, payload }`
- Payloads: `player`, `playerItem`, `playerLevel`, `playerSkillLevel`, `playerGold`, `phase`, `heartbeat`, `custom`

## Configuration

- `LEVENTS_GRPC_ADDR` — gRPC bind address for the daemon (default `127.0.0.1:50051`)
- `LEVENTS_LCU_LOCKFILE` — absolute path to the LCU lockfile; when unset, common OS-specific paths are scanned automatically

Internal timing defaults (see `levents-core`):
- Heartbeat: 1s
- Poll intervals: combat ~150ms, normal ~750ms, idle ~1500ms with cooldowns and error backoff

TLS notes:
- The daemon talks to local endpoints only and accepts the Live Client/LCU’s local certificates. The gRPC server is plaintext on localhost by default.

## JSON Schema

Generate a JSON Schema for `EventBatch` types from the Rust model:

```bash
cargo run -p levents-model --bin event-schema -- doc/event_schema.json
```

## Development

- Rust daemon
  - `cd levents && cargo check`
  - `cargo run --bin levents-daemon`
  - Format/lint: `cargo fmt && cargo clippy --all-targets --all-features`
- TypeScript SDK
  - `pnpm install`
  - `pnpm --filter @levents/sdk build`
  - Example: `pnpm --filter @levents/sdk example:basic`
- Python SDK
  - `pip install -e bindings/py[dev]`
  - CLI: `python -m levents watch`

Testing:
- `cargo test --workspace`
- `pnpm test` (runs repo-wide Node tests)
- `pytest bindings/py`

## Conventions

- Commit style: Conventional Commits (see `CONTRIBUTING.md`)
- Formatting: `rustfmt`, `eslint`, `black --line-length 100`
- CI: GitHub Actions covers Rust/Node/Python (see `.github/workflows/ci.yml`)

## License

MIT — see `LICENSE`.
