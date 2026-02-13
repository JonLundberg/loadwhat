# Contributing

## Development setup

1. Install Rust stable (`rustup default stable`).
2. Build locally:
   - `cargo build`
   - `cargo test`
3. Run formatter and lints before opening a PR:
   - `cargo fmt`
   - `cargo clippy -- -D warnings`

## Pull request expectations

1. Keep changes scoped and explain the behavior impact.
2. Preserve line-oriented output compatibility (`TOKEN key=value ...`).
3. Add or update tests when parser or search behavior changes.
4. Keep the project dependency-free unless there is a strong reason.

## Issue reports

When reporting DLL load failures, include:
- command used
- target executable path
- complete `loadwhat` output
- Windows version and architecture
