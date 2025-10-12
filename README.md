# levents

Monorepo that unifies League of Legends Live Client Data API polling and LCU WebSocket insights into
a fast Rust daemon with TypeScript and Python bindings.

## Repository Layout

- `levents/` – Rust workspace (`levents-core`, `levents-daemon`, `levents-model`)
- `bindings/ts` – TypeScript SDK (`@levents/sdk`)
- `bindings/py` – Python SDK (`levents-py`)
- `doc/` – Product specifications and reference material

## Getting Started

### Prerequisites

- Rust 1.74+
- Node.js 20+ with `pnpm`
- Python 3.10+

### Rust daemon

```bash
cd levents
cargo check
cargo run --bin levents-daemon
```

### TypeScript SDK

```bash
pnpm install
pnpm --filter @levents/sdk build
pnpm --filter @levents/sdk example:basic
```

### Python SDK

```bash
pip install -e bindings/py[dev]
pytest bindings/py
python -m levents watch
```

## Conventions

- Licensing: MIT (see `LICENSE`)
- Commits: Conventional Commit format (see `CONTRIBUTING.md`)
- Formatting: `rustfmt`, `eslint`, `black`
- Continuous Integration: GitHub Actions (`.github/workflows/ci.yml`)
