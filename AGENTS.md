# Repository Guidelines

## Project Structure & Module Organization
- `levents/` – Rust workspace containing `levents-core` (API clients), `levents-model` (shared types), and `levents-daemon` (runtime entrypoint).
- `bindings/ts` – TypeScript SDK `@levents/sdk`; builds to `dist/` and ships Node examples in `examples/`.
- `bindings/py` – Python SDK `levents-py` with CLI entrypoints under `src/levents` and tests in `bindings/py/tests`.
- `doc/` – Product specifications and reference material that guide API semantics.

## Build, Test, and Development Commands
- `cargo check` / `cargo run --bin levents-daemon` (from `levents/`) validate and run the Rust daemon locally.
- `cargo fmt && cargo clippy --all-targets --all-features` enforce Rust formatting and linting.
- `pnpm install` then `pnpm --filter @levents/sdk build` compile the TypeScript SDK; `pnpm --filter @levents/sdk example:basic` runs the sample client.
- `pip install -e bindings/py[dev]` prepares the Python environment; `python -m levents watch` starts the live event watcher.
- `pnpm test`, `cargo test --workspace`, and `pytest bindings/py` execute the monorepo test suites before pushing.

## Coding Style & Naming Conventions
- Mandatory formatters: `rustfmt`, `eslint`, and `black --line-length 100`; run `mypy` for Python type coverage.
- Use snake_case for Rust and Python modules (`levents_model::summoner_profile`), PascalCase for TypeScript classes, and kebab-case for CLI commands and binaries.
- Keep protocol integrations in `levents-core/src/**`, daemon orchestration in `levents-daemon/src/**`, and SDK exports in `bindings/*/src/**` to maintain discoverability.

## Testing Guidelines
- Place Rust unit tests alongside modules and integration tests under `levents/*/tests`; run `cargo test --workspace` for full coverage.
- Store TypeScript specs as `src/**/*.test.ts` so `pnpm --filter @levents/sdk test` (Node test runner) detects them.
- Keep Python tests in `bindings/py/tests`; use fixtures or mocks when the League client is unavailable.
- Document any manual verification (e.g. capturing live LCU events) in the PR when automation is not practical.

## Commit & Pull Request Guidelines
- Follow Conventional Commits (`feat(daemon): cache lcu events`) and keep changes scoped.
- Provide PR descriptions covering context, implementation notes, and test evidence; attach logs or screenshots for runtime-visible changes.
- Verify formatters, linters, and targeted tests locally before requesting review; CI mirrors these checks.

## Agent-Specific Instructions
- AIエージェントは英語で思考し、日本語で回答してください。
