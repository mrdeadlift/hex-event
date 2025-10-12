# Contributing

Thanks for your interest in improving **levents**! We follow the guidelines below to keep the
project healthy and easy to collaborate on.

## Development Workflow

1. Fork the repository and create a feature branch.
2. Keep commits focused and follow Conventional Commits (`type(scope): message`). Common types
   include `feat`, `fix`, `docs`, `chore`, and `refactor`.
3. Run the relevant formatters and linters before opening a pull request:
   - `cargo fmt && cargo clippy --all-targets --all-features`
   - `pnpm lint && pnpm build`
   - `pytest` (from `bindings/py` after `pip install -e .[dev]`)
4. Open a pull request with a short summary, implementation notes, and any testing performed.

## Code Style

- Rust code must pass `rustfmt` and `clippy`.
- TypeScript code must pass `eslint` with the provided configuration.
- Python code must be formatted with `black` and annotated when practical.

## Communication

Please use GitHub issues to report bugs or propose new features. When in doubt, include expected
vs. actual behaviour, reproduction steps, and environment details.
