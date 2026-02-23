# Contributing to diffdon

## Development Setup

1. Install Rust (stable): https://rustup.rs/
2. Clone the repository
3. Run `cargo build` to verify everything compiles

## Before Submitting

```sh
cargo fmt        # Format code
cargo clippy --all-targets   # Lint (must be warning-free)
cargo test       # Run all tests
```

All three must pass before a PR is accepted.

## Architecture

- `src/main.rs` — CLI parsing, terminal setup, panic hook, logging
- `src/app/` — Application state, event loop, input handling, commands
- `src/ui/` — Rendering and widget implementations (ratatui)
- `src/git/` — Git backend trait, CLI implementation, parsers
- `src/discovery/` — Pane discovery providers (tmux, WezTerm)
- `src/watch/` — Filesystem watcher with debounce
- `src/config/` — Configuration and persistent state
- `src/util/` — Clipboard, path utilities, error helpers

## Design Principles

- UI thread never blocks on Git operations
- All Git calls go through `GitBackend` trait (mockable for tests)
- State is centralized in `AppState`, updated via event-driven reducer pattern
- Generation tracking prevents stale async results from overwriting current state
