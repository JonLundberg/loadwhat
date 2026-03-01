# loadwhat Testing Notes

This repository uses an `xtask` harness to build native fixtures and run integration tests from a clean state.

## Run tests

Windows workflow:

- `cargo xtask test`

High-level behavior:

1. deletes `target/loadwhat-tests/` from previous runs
2. creates `target/loadwhat-tests/fixtures/bin/`
3. builds MSVC fixtures via MSBuild
4. runs `cargo test --tests` with harness environment set

## MSBuild requirements

`xtask` discovers MSBuild using:

1. `msbuild` on `PATH`
2. `MSBUILD_EXE_PATH` if set
3. `vswhere.exe` lookup
4. common VS 2019/2022 install paths

## Harness environment variables (internal)

Set by `cargo xtask test`:

- `LOADWHAT_TEST_ROOT=target/loadwhat-tests`
- `LOADWHAT_FIXTURE_BIN_ROOT=target/loadwhat-tests/fixtures/bin`
- `LOADWHAT_TEST_MODE=1`

Optional:

- `LOADWHAT_KEEP_TEST_ARTIFACTS=1` keeps artifacts for debugging
- `RUST_TEST_THREADS=1` is set if not already defined

## Internal LWTEST lines

When `LOADWHAT_TEST_MODE=1`, the binary may emit internal lines:

- `LWTEST:LOAD ...`
- `LWTEST:RESULT ...`
- `LWTEST:TARGET ...`

These lines are for harness assertions and are not part of the public token contract in `docs/loadwhat_spec_v1.md`.

Some integration tests may assert optional fields like `via`/`depth` on `STATIC_MISSING` for transitive missing dependencies, and may assert `NOTE` lines related to loader-snaps setup details.

`run` defaults to one-line summary output. Tests that assert detailed trace lines (`SEARCH_ORDER`, `SEARCH_PATH`, runtime timeline tokens, or loader-snaps notes) must pass `--trace` (or `-v`, which implies trace).
