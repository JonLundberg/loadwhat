# loadwhat Testing Notes

This repo uses an `xtask` harness to build native fixtures and run integration tests with a clean state.

## Run tests

- Windows only:
  - `cargo xtask test`

What it does (high level):

1. deletes `target/loadwhat-tests/` if it exists
2. creates `target/loadwhat-tests/fixtures/bin/`
3. builds MSVC fixtures via MSBuild into that bin directory
4. runs `cargo test --tests` with harness env vars set

## MSBuild requirements

`xtask` finds MSBuild using:

1. `msbuild` on PATH
2. `MSBUILD_EXE_PATH` (if set)
3. `vswhere.exe` lookup (Visual Studio installer)
4. common hardcoded locations for VS 2019/2022 editions

## Harness env vars (internal)

The harness sets:

- `LOADWHAT_TEST_ROOT` = `target/loadwhat-tests`
- `LOADWHAT_FIXTURE_BIN_ROOT` = `target/loadwhat-tests/fixtures/bin`
- `LOADWHAT_TEST_MODE=1` (enables internal testing output behavior)

Optional:

- `LOADWHAT_KEEP_TEST_ARTIFACTS=1` keeps the test root between runs for debugging
- `RUST_TEST_THREADS=1` is set if not already defined (keeps tests deterministic)

## Internal LWTEST output

When `LOADWHAT_TEST_MODE=1`, the binary may emit internal tokens like:

- `LWTEST:TARGET ...`
- `LWTEST:LOAD ...`
- `LWTEST:RESULT ...`

These are not part of the public output contract; tests should prefer asserting against public tokens when possible.
