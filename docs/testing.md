# loadwhat Testing Notes

This repository uses an `xtask` harness to build native fixtures and run integration tests from a clean state.

## Run tests

Windows workflow:

- `cargo xtask test`
- `cargo xtask test-container` for real COM registry integration tests

Plain `cargo test` and `cargo test --tests` run the default test set without the harness-backed integration suite. The fixture-backed integration suite is gated behind the `harness-tests` feature and is driven by `cargo xtask test`.

High-level behavior:

1. deletes `target/loadwhat-tests/` from previous runs
2. creates `target/loadwhat-tests/fixtures/bin/`
3. builds MSVC fixtures via MSBuild
4. runs `cargo test --tests --features harness-tests` with harness environment set

## COM container tests

Run the registry-backed COM suite only when Docker is using Windows containers:

```powershell
cargo xtask test-container
```

The command builds a statically linked release executable, builds x64 and x86
MSVC fixtures, stages a Docker context under
`target/loadwhat-container-tests/`, builds a Windows Server Core image, and
runs the test script with Hyper-V isolation.

Registry safety is enforced in two places:

- The host runner performs read-only Win32 sentinel checks before and after the
  container. It never creates, updates, or deletes host registry keys.
- The registry setup script requires the image marker, Windows
  `ContainerType=2`, and `C:\WcSandboxState` before it writes any keys.

All COM fixture registrations are created inside the disposable container.
The scripts use fixed test CLSIDs and fixed HKCU/HKLM `Software\Classes` roots;
they do not accept a registry root from the caller. A failing host sentinel
aborts without deleting anything.

Prerequisite and troubleshooting instructions are in
`docs/windows_docker_container_setup.md`.

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

## Why tests can fail immediately

Harness-dependent integration tests call `require_from_env()` and panic if the harness is missing or invalid.

That failure is intentional. Use `cargo xtask test`, not plain `cargo test`, when you need fixture-backed integration coverage.

Common causes:

- `LOADWHAT_TEST_ROOT` or `LOADWHAT_FIXTURE_BIN_ROOT` is missing or empty.
- `loadwhat.exe` was not built where the harness expects it.
- Windows blocked executing `loadwhat.exe`; the error message preserves `raw_os_error=4551`.

If you hit `raw_os_error=4551`, Windows blocked the executable before the harness could probe it. Check Smart App Control, Defender, and Mark-of-the-Web, then unblock or rebuild the binary and rerun `cargo xtask test`.

If you explicitly run `cargo test --tests --features harness-tests` without the harness environment, the tests still fail loudly so the setup problem is obvious.

## Internal LWTEST lines

When `LOADWHAT_TEST_MODE=1`, the binary may emit internal lines:

- `LWTEST:LOAD ...`
- `LWTEST:RESULT ...`
- `LWTEST:TARGET ...`

These lines are for harness assertions and are not part of the public token contract in `docs/loadwhat_spec_v1.md`.

Public CLI contract tests must use the public runners, which remove `LOADWHAT_TEST_MODE` from the spawned `loadwhat` process. Those tests must not depend on `LWTEST:` lines.

Some integration tests may assert optional fields like `via`/`depth` on `STATIC_MISSING` for transitive missing dependencies, and may assert `NOTE` lines related to loader-snaps setup details. Summary-mode tests must not expect loader-snaps setup/restore notes; those notes are trace-visible or verbose-only diagnostics.

## `run` CLI contract

`run` uses this public synopsis:

```text
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
```

- All `run` options must appear before `<TARGET>`.
- Tokens after `<TARGET>` are passed through to the target unchanged.
- Loader-snaps is enabled by default; use `--no-loader-snaps` to disable it.
- Summary output is the default; use `--trace` or `-v` for detail.
- Tests that assert trace lines (`SEARCH_ORDER`, `SEARCH_PATH`, runtime timeline tokens, or loader-snaps notes) must pass `--trace` or `-v`.
- The current timeout contract is explicit in tests and docs:
  - timeout after runtime module-load progress returns `0`; summary mode emits `SUCCESS status=0`
  - timeout before meaningful runtime progress returns `21`, emits no public
    summary token, and explains the failure on stderr
  - a nonzero timeout explicitly terminates the target and drains its exit
    debug event; `--timeout-ms 0` disables the deadline
