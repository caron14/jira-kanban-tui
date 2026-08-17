# Repository Guidelines

## Project Structure & Module Organization

This is a Rust 2021 terminal application. `src/main.rs` is the binary entry point and `src/lib.rs` exposes reusable modules. Keep business rules in `src/domain/`, Jira API code in `src/jira/`, state and events in `src/app/`, rendering in `src/ui/`, and configuration, caching, authentication, and integrations in `src/infrastructure/`. Integration and performance tests live in `tests/`; supporting material belongs in `docs/` and `prompts/`.

## Build, Test, and Development Commands

- `cargo run` starts the TUI with the default configuration.
- `cargo run -- doctor` validates the Jira account and configured Boards.
- `RUST_LOG=debug cargo run` enables diagnostic logging.
- `cargo test --locked --all-targets` runs unit, integration, rendering, and load tests.
- `cargo fmt --check` verifies formatting; `cargo fmt` applies it.
- `cargo clippy --locked --all-targets -- -D warnings` treats every lint warning as an error.
- `cargo build --locked --release` builds the optimized binary from locked dependencies.

Run formatting, Clippy, tests, and the release build before submitting changes.

## Coding Style & Naming Conventions

Follow standard Rust naming: `snake_case` for modules, functions, and tests; `PascalCase` for types and traits; and `SCREAMING_SNAKE_CASE` for constants. `rustfmt.toml` sets a 100-column maximum. Prefer explicit domain types and keep Jira-specific network behavior out of the UI. Keep network work out of rendering paths and return contextual errors rather than panicking in production code.

## Testing Guidelines

Place unit tests beside their modules and cross-module tests in `tests/*.rs`. Name tests after observable behavior, such as `render_board_deterministic`. TUI tests should use Ratatui's `TestBackend`; API tests should use `wiremock` and require neither credentials nor network access. Preserve `tests/load.rs` timing assertions and add regression coverage for fixes.

## Commit & Pull Request Guidelines

History is minimal and establishes no formal convention. Use concise, imperative subjects (for example, `Handle expired Jira tokens`) and keep commits focused. Pull requests should explain user-visible behavior, identify affected views or configuration, link issues, and list verification commands. Include a screenshot or recording for UI changes and flag configuration compatibility changes.

## Security & Configuration

Never commit API tokens, personal configuration, cache data, or logs. Credentials must continue to resolve through the OS keyring, environment variables, or argv-based token commands; do not invoke token commands through a shell or log authorization data.
