# Contributing

## Development setup

1. Use Windows x64.
2. Install Rust stable:
   - `rustup default stable`
3. For fixture/integration workflow, ensure MSBuild is available (Visual Studio Build Tools or Visual Studio installation).

## Build and local checks

- `cargo build`
- `cargo test --tests`
- `cargo fmt`
- `cargo clippy -- -D warnings`

## Supported test workflow

Primary integration workflow:

- `cargo xtask test`

`cargo xtask test` does the following:

1. Removes `target/loadwhat-tests/` from prior runs.
2. Builds C++ fixtures into `target/loadwhat-tests/fixtures/bin/`.
3. Runs `cargo test --tests` with harness environment variables set.

## MSBuild discovery used by `xtask`

`xtask` resolves MSBuild in this order:

1. `msbuild` on `PATH`
2. `MSBUILD_EXE_PATH` environment variable
3. `vswhere.exe` discovery
4. known Visual Studio install paths

If fixture build fails with "program not found", install Build Tools or set `MSBUILD_EXE_PATH`.

## Harness environment variables (internal)

Set by `cargo xtask test`:

- `LOADWHAT_TEST_ROOT`
- `LOADWHAT_FIXTURE_BIN_ROOT`
- `LOADWHAT_TEST_MODE=1`

Optional:

- `LOADWHAT_KEEP_TEST_ARTIFACTS=1` keeps test artifacts for debugging
- `RUST_TEST_THREADS=1` is set if the user has not already set it

`LWTEST:*` lines are internal harness diagnostics and not part of the public user-facing token contract.

## Pull request expectations

1. Keep changes scoped.
2. Preserve token contract compatibility with `docs/loadwhat_spec_v1.md`.
3. Add/update tests when behavior changes.
4. Update docs when CLI, output tokens, or workflow changes.

## Issue reports

For DLL-load diagnosis issues, include:

- exact command
- target executable path
- complete `loadwhat` output
- Windows version/build and architecture
- if relevant, `--loader-snaps -v` output so loader-snaps enable/offset notes are visible
