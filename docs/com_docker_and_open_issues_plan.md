# COM Docker Test Framework and Open Issues Plan

Status: active work plan.

Primary instruction for agents: follow this plan in order. The Docker-based COM test framework is the priority lane because it protects the developer's host registry and creates the safest place to verify COM behavior. Do not make broad COM behavior changes before the container test lane is scaffolded, unless a step below explicitly allows it.

Authoritative specs:

1. `docs/loadwhat_spec_v2.md`
2. `docs/loadwhat_spec_v1.md` for `run` / `imports` as incorporated by v2
3. `AGENTS.md`

Supporting docs:

- `docs/com_testing_strategy.md`
- `docs/code_review_findings.md`
- `docs/roadmap.md`
- `docs/testing.md`

## State System

Every agent working this plan must update this file before ending a turn.

Update these sections:

1. `Current State`
2. `Work Item Status`
3. `State Log`
4. `Decisions`
5. `Blocked Items`, if anything is blocked

Rules:

- Add a dated log entry for each meaningful change.
- Include exact commands run and whether they passed.
- Include commit hashes if commits are created.
- Keep status values to: `todo`, `in_progress`, `blocked`, `done`.
- Do not delete old state log entries. Append new ones.
- If implementation changes public output, update the relevant spec first or in the same change.

## Current State

Last updated: 2026-07-09 by Codex.

- v2 is now the top-level authoritative spec.
- v1 remains incorporated for `run` and `imports`.
- COM implementation exists on `main`.
- COM tests are mostly mock/unit tests.
- Existing `cargo test` passed with 264 tests.
- Existing `cargo xtask test` passed during review with 99 integration tests.
- Docker/container COM testing is not implemented.
- Recent review findings are recorded in `docs/code_review_findings.md`.
- This plan requires all COM registry mutation to happen inside a Windows container. Host-side registry mutation is out of bounds.
- Added `docs/windows_docker_container_setup.md` for host setup.
- Current local Docker state is ready for initial Windows container work:
  Docker reports `OSType=windows`, active context is `desktop-windows`, Server
  Core `ltsc2022` is pulled, and a `docker run --rm ... cmd /c ver` smoke test
  passed.
- Windows feature setup reported `RestartNeeded : True` for Hyper-V. Windows
  containers are currently working with Hyper-V isolation, but a reboot is still
  recommended before relying on longer test runs.

## Work Item Status

| ID | Status | Priority | Owner | Summary |
|----|--------|----------|-------|---------|
| D1 | done | P0 | unassigned | Define Windows container test contract and host prerequisites |
| D2 | done | P0 | Codex | Add Docker files and container PowerShell test runner |
| D3 | done | P0 | Codex | Add `cargo xtask test-container` entry point |
| D4 | done | P0 | Codex | Add container-safe registry fixture scripts |
| D5 | done | P0 | Codex | Add COM container smoke tests for real registry views |
| D5A | done | P0 | Codex | Add host-registry sentinel checks around container tests |
| D6 | in_progress | P1 | Codex | Add container tests for HKCU/HKLM override behavior |
| D7 | in_progress | P1 | Codex | Add container tests for x86/x64 registry views and WOW64 server paths |
| D8 | in_progress | P1 | Codex | Add container tests for COM server dependency failures |
| D9 | done | P1 | Codex | Document container workflow and cleanup expectations |
| C1 | todo | P1 | unassigned | Fix `com audit` dependency walk target-context issue |
| C2 | todo | P2 | unassigned | Fix HKCU-present broken value falling through to HKLM |
| C3 | todo | P2 | unassigned | Decide and fix COM indeterminate-error token contract |
| C4 | todo | P3 | unassigned | Add fixture-backed non-container COM CLI tests |
| C5 | todo | P3 | unassigned | Clean up COM docs after behavior fixes |
| V1-1 | todo | P2 | unassigned | Address summary-mode silent failure paths |
| V1-2 | todo | P2 | unassigned | Preserve empty target arguments |
| V1-3 | todo | P2 | unassigned | Gate `LOADWHAT_TEST_MODE` out of release builds |
| V1-4 | todo | P3 | unassigned | Document or harden IFEO cleanup risk |
| V1-5 | todo | P3 | unassigned | Fix timeout process cleanup / timeout semantics |

## Registry Safety Contract

This section is mandatory. If implementation work conflicts with this contract,
stop and update this plan before continuing.

Host-side commands may:

- build Rust code
- build MSVC fixtures
- build a Docker image
- run a Docker container
- copy files into a staging directory under `target/`
- inspect host registry only for sentinel checks

Host-side commands must not:

- create, update, or delete `HKCU:\Software\Classes` keys
- create, update, or delete `HKLM:\Software\Classes` keys
- run `reg add`, `reg delete`, `New-Item`, `Set-ItemProperty`, or
  `Remove-Item` against host COM registry paths
- run `loadwhat com` against test CLSIDs that were registered on the host
- depend on host COM test registrations
- require administrator elevation for host registry mutation

Container-side commands may:

- create, update, and delete `HKCU:\Software\Classes` and
  `HKLM:\Software\Classes` test keys inside the container
- run `loadwhat com ...` against those container-local keys
- test IFEO/loader-snaps registry fallback only inside the container

Implementation guardrails:

- All registry fixture scripts must live under `tests/com/container/`.
- Registry fixture scripts must be copied into the image and invoked by
  `docker run`, not invoked directly by `cargo xtask test-container` on the host.
- `cargo xtask test-container` must treat registry script execution on the host
  as a bug.
- Use a unique test registry prefix, for example
  `LoadWhat.Container.ComTests`.
- Use unique CLSIDs with a fixed comment/header in scripts so accidental host
  keys are easy to recognize.
- Add a host-registry sentinel check before and after the container run. The
  sentinel should verify that the chosen test prefix does not exist on the host
  before the run and still does not exist after the run.
- If the sentinel finds host test keys before the run, abort and tell the user
  exactly which keys exist. Do not delete them automatically.
- If the sentinel finds host test keys after the run, fail loudly and mark the
  plan blocked.

Suggested sentinel paths:

```text
HKCU:\Software\Classes\LoadWhat.Container.ComTests
HKLM:\Software\Classes\LoadWhat.Container.ComTests
HKCU:\Software\Classes\CLSID\{LOADWHAT-CONTAINER-*}
HKLM:\Software\Classes\CLSID\{LOADWHAT-CONTAINER-*}
```

PowerShell note: wildcard CLSID checks should enumerate under `...\CLSID` and
filter names; do not use broad delete commands.

## Prerequisites

Host requirements:

- Windows 10/11 or Windows Server with Windows container support.
- Docker Desktop or another Docker-compatible runtime that supports Windows
  containers.
- Docker switched to Windows containers, not Linux containers.
- Windows optional features enabled as required by the runtime:
  - Containers
  - Hyper-V, if using Hyper-V isolation
- Enough disk space for Windows base images and test layers. Budget at least
  20 GB free.
- Network access to pull the base image, unless it is already cached.
- Rust stable toolchain.
- Visual Studio Build Tools or full Visual Studio with MSVC x64 tools.
- MSBuild discoverable by the existing `xtask` logic.

Container image requirements:

- Use Windows Server Core, not Nano Server.
- Initial base image: `mcr.microsoft.com/windows/servercore:ltsc2022`.
- If process isolation fails because the host OS version is incompatible, use
  Hyper-V isolation or switch to a matching Server Core tag.

Operator checks before running container tests:

```powershell
docker version
docker info --format '{{.OSType}}'
docker pull mcr.microsoft.com/windows/servercore:ltsc2022
cargo test
cargo xtask test
```

Expected:

- `docker info --format '{{.OSType}}'` prints `windows`.
- `cargo test` and `cargo xtask test` pass before starting Docker work.

If Docker reports Linux containers, switch Docker Desktop to Windows containers
before running `cargo xtask test-container`.

## Phase 0 - Safety and Branch Hygiene

Goal: start from a known state and avoid mixing unrelated changes.

Steps:

1. Run `git status --short --branch`.
2. If there are uncommitted changes, identify whether they are authority-doc updates, plan updates, or unrelated user work.
3. Do not revert user work.
4. Prefer one branch for this effort, for example `codex/com-container-tests`, unless the user names a branch.
5. Keep Docker harness changes separate from COM behavior fixes when practical.

Acceptance criteria:

- Working tree state is recorded in `State Log`.
- The active branch is recorded.

## Phase 1 - Docker Test Contract

Goal: define a minimal Windows container workflow before writing behavior fixes.

Design target:

```text
cargo xtask test-container
```

Expected behavior:

1. Build `loadwhat.exe`.
2. Build or collect COM fixture binaries.
3. Build a Windows Server Core container image.
4. Copy `loadwhat.exe`, fixtures, and test scripts into the image.
5. Run the container test script.
6. The script creates COM registry keys inside the container only.
7. The script runs `loadwhat com ...` commands.
8. The script exits nonzero on failed assertions.
9. The host-side runner verifies host-registry sentinel paths before and after
   the container run.

Required host prerequisites:

- Windows host with Docker Desktop or compatible container runtime.
- Windows containers enabled.
- Base image available, initially `mcr.microsoft.com/windows/servercore:ltsc2022`.
- Container isolation documented: process isolation preferred if compatible; Hyper-V isolation acceptable.

Files to add or update:

- `Dockerfile.com-tests` or `tests/com/container/Dockerfile`
- `tests/com/container/run_container_tests.ps1`
- `tests/com/container/setup_registry.ps1`
- `tests/com/container/assert.ps1`
- `xtask` command support for `test-container`
- `docs/testing.md`
- `docs/com_testing_strategy.md`

Acceptance criteria:

- A no-op container test can run and report PASS.
- If Docker is unavailable, `cargo xtask test-container` fails clearly with setup guidance and does not mutate host registry.
- If Docker is in Linux-container mode, `cargo xtask test-container` fails before
  building/running tests and does not mutate host registry.
- The no-op container test includes host-registry sentinel checks.

## Phase 2 - Container Registry Fixtures

Goal: create real COM registrations safely inside the container.

Registry fixture rules:

- Use unique CLSIDs under a fixed test prefix.
- Do not use real system CLSIDs for mutation tests.
- Prefer HKCU/HKLM `Software\Classes` paths directly.
- Create both 64-bit and 32-bit view registrations where needed.
- Always print created keys in the test log.
- Cleanup inside the container is nice to have but not relied on for host safety.
- Registry fixture scripts must assert they are running inside the expected
  container environment before writing registry keys. For example, require an
  environment variable such as `LOADWHAT_CONTAINER_TESTS=1` set only in the
  Dockerfile or `docker run` command.
- Registry fixture scripts must not accept arbitrary registry root paths from
  caller input.

Scenarios:

1. HKLM 64-bit `InprocServer32` registration pointing to an x64 fixture DLL.
2. HKCU override of the same CLSID pointing to a different fixture DLL.
3. HKCU-present broken registration with HKLM fallback available.
4. ProgID -> CLSID.
5. ProgID -> CurVer -> CLSID.
6. TreatAs redirect.
7. 32-bit registry view registration.
8. LocalServer32 command line with quoted path and arguments.

Acceptance criteria:

- Container script can create and read these keys using PowerShell.
- `loadwhat com clsid` and `loadwhat com progid` see expected registrations.
- Tests assert both stdout token shape and exit code.
- Running the registry setup script directly on the host without
  `LOADWHAT_CONTAINER_TESTS=1` fails before writing anything.

## Phase 3 - Container Fixture Binaries

Goal: provide real files for COM server validation.

Preferred first implementation:

- Reuse existing MSBuild fixture infrastructure where possible.
- Add COM-specific fixture projects only if needed.
- Keep generated artifacts under `target/loadwhat-tests/` or a container staging folder.

Needed fixtures:

| Fixture | Purpose |
|---------|---------|
| `lwtest_com_server_x64.dll` | Healthy x64 in-proc server file |
| `lwtest_com_server_dep_missing.dll` | x64 DLL with missing direct dependency |
| `lwtest_com_server_dep_transitive.dll` | x64 DLL with missing transitive dependency |
| `lwtest_com_localserver_x64.exe` | Healthy local server executable |
| `lwtest_com_target_x64.exe` | Target executable for `com audit` |
| `lwtest_com_server_x86.dll` | x86 in-proc server for 32-bit view and mismatch tests |
| `lwtest_com_target_x86.exe` | x86 target for audit view-selection tests |

If x86 fixture builds are too expensive initially:

- Mark x86 scenarios blocked.
- Still implement x64 container tests first.
- Record the blocker in `Blocked Items`.

Acceptance criteria:

- Fixture binaries are built by a repeatable command.
- Container tests do not depend on files outside the mounted/copied test root.

## Phase 4 - First Container Test Set

Goal: establish high-value end-to-end COM coverage before changing behavior.

Implement these tests first:

1. `com progid` resolves a container-created HKLM registration.
2. `com clsid --view 64` resolves x64 registration.
3. `com clsid --view 32` resolves 32-bit view registration or reports the expected blocker if x86 fixtures are not ready.
4. `com server` validates a healthy server binary.
5. `com server` reports dependency failure for a broken server binary.
6. `com audit` falls back to registry for a target and class.
7. `com audit` uses sidecar manifest before registry when both exist.

Acceptance criteria:

- Tests run only inside the container.
- Tests assert exact token family and important fields.
- Tests assert exit code `0`, `10`, `20`, `21`, or `22` as appropriate.
- Host registry is not touched.
- Host-registry sentinel checks pass before and after the test run.

## Phase 5 - Fix COM Issues Under Container Coverage

Only start this phase after Phases 1-4 have at least a working smoke test lane.

### C1 - `com audit` target-context dependency walk

Problem:

- Current audit validates server dependencies using the server directory as context.
- Target activation should be evaluated from the target process context, or the spec must say otherwise.

Implementation options:

1. Add target-context-aware dependency walking to the COM file-system abstraction.
2. Pass target app directory / cwd into audit validation.
3. Keep `com server` server-centric, but make `com audit` target-centric.

Required tests:

- Container or fixture test where target dir contains the dependency and server dir does not.
- Container or fixture test where server dir contains the dependency but target dir does not.

Spec work:

- Update `docs/loadwhat_spec_v2.md` before or with implementation.

### C2 - HKCU-present broken value fallback

Problem:

- HKCU key presence with unusable value can fall through to HKLM.

Implementation direction:

- Teach merge reads to distinguish absent key from present key with absent/unusable value.
- Return `BROKEN_REGISTRATION` or an explicit spec-defined status for present-but-invalid server registration.

Required tests:

- Mock unit tests.
- Container registry test with HKCU broken registration and HKLM healthy fallback.

Spec work:

- Clarify HKCU override semantics for present keys with missing, empty, or non-string default values.

### C3 - COM indeterminate-error token contract

Problem:

- Some COM failures exit before emitting a `COM_*` token.
- Current v2 text says summary mode emits exactly one token line.

Decision required:

- Option A: Add token output for indeterminate COM command failures.
- Option B: Document exceptions where setup/input failures may emit stderr only.

Preferred direction:

- Option A for greppability, unless implementation would fabricate fields.

Required tests:

- Missing audit target.
- Non-PE audit target.
- Unsupported machine target.
- Unreadable server path if practical.

## Phase 6 - Non-Container COM Tests

Goal: keep fast local feedback without touching real registry.

Work:

- Add public-contract CLI tests for COM commands that use mockable or file-only behavior.
- Add fixture-backed `com server` tests under `cargo xtask test`.
- Keep real registry mutation only in container tests.

Acceptance criteria:

- `cargo test` remains fast.
- `cargo xtask test` covers file-based COM server behavior.
- `cargo xtask test-container` covers registry behavior.

## Phase 7 - Older v1 Issues

These are important but should not preempt the Docker COM test framework unless they block it.

Recommended order:

1. V1-2: preserve empty target arguments.
2. V1-3: gate `LOADWHAT_TEST_MODE` out of release builds.
3. V1-1: summary-mode silent failure paths.
4. V1-5: timeout process cleanup / timeout semantics.
5. V1-4: IFEO cleanup risk documentation or hardening.

Reasoning:

- V1-2 and V1-3 are narrow and low-conflict.
- V1-1 and V1-5 affect public output/exit semantics and need spec care.
- V1-4 may involve process/control-handler design and should be isolated.

## Phase 8 - Documentation Cleanup

Goal: make docs match the implemented workflow.

Update:

- `docs/testing.md` with `cargo xtask test-container`.
- `docs/com_testing_strategy.md` with final Docker command names and fixture list.
- `README.md` with a short COM testing note.
- `docs/roadmap.md` to move completed Docker work out of future work.
- `docs/code_review_findings.md` to mark addressed items.

Acceptance criteria:

- A new agent can find the correct test command.
- A human can understand whether a test mutates host registry.
- Docker limitations are explicit.

## Completion Criteria

The full effort is complete when:

- `cargo test` passes.
- `cargo xtask test` passes.
- `cargo xtask test-container` passes on a Windows container-capable host.
- Container tests create and use COM registry keys without touching host registry.
- Host-registry sentinel checks prove the test prefix was not created on the host.
- COM review findings C1-C3 are resolved or explicitly re-scoped in v2.
- `docs/code_review_findings.md` reflects final status.
- `docs/loadwhat_spec_v2.md` matches implemented public behavior.

## State Log

### 2026-07-08 - Codex

- Created this work plan.
- Prioritized Docker/container COM tests ahead of COM behavior fixes.
- Incorporated open COM review findings from `docs/code_review_findings.md`.
- Included older v1 issues as a later phase to avoid conflicts.
- No implementation commands run for this plan file.

### 2026-07-08 - Codex registry-safety review

- Added mandatory `Registry Safety Contract`.
- Added host/container prerequisite checklist.
- Added host-registry sentinel requirement before and after container tests.
- Added requirement that registry fixture scripts refuse to run unless
  `LOADWHAT_CONTAINER_TESTS=1` is set inside the container.
- Added D5A work item for host-registry sentinel checks.

### 2026-07-09 - Codex Windows Docker setup check

- Added `docs/windows_docker_container_setup.md`.
- Ran `docker version`; Docker CLI is installed, but Docker Desktop reported:
  `Docker Desktop is manually paused. Unpause it through the Whale menu or Dashboard.`
- Ran `docker context ls`; active context is `desktop-linux`.
- Ran `DockerCli.exe -SwitchWindowsEngine`; it failed with:
  `switching to windows engine: windows containers have been disabled for this installation`.
- Ran `Get-Service com.docker.service,hns,vmcompute`; `hns` and `vmcompute`
  are running, but `com.docker.service` is stopped.
- Ran `Start-Service com.docker.service`; it failed because this shell cannot
  open the service without elevation.
- Ran non-elevated DISM feature checks for `Containers` and
  `Microsoft-Hyper-V-All`; both failed with error 740 because elevation is
  required.
- Confirmed this shell is not elevated with `IsElevated=False`.
- After the user ran Docker Desktop Installer from an elevated PowerShell,
  inspected `C:\ProgramData\DockerDesktop\install-log-admin.txt`; the installer
  completed successfully but logged `settings from previous installation exist:
  match the backend mode to that` and selected `wsl-2`.
- Inspected `C:\ProgramData\DockerDesktop\install-settings.json`; it contains
  `"noWindowsContainers": true`.
- Updated `docs/windows_docker_container_setup.md` to remove that persisted
  setting before rerunning the installer with `--quiet --backend=windows`.
- No host COM registry mutation was performed.

### 2026-07-09 - Codex container framework implementation

- Committed the authority, review, Docker plan, and setup documentation as
  `b0efa4a` (`Document COM Docker test plan and v2 authority`).
- Added `cargo xtask test-container`, a Windows Server Core Dockerfile, guarded
  PowerShell registry fixtures, exact summary-token assertions, x64/x86 native
  fixtures, and read-only host registry sentinel checks.
- The container runner uses `--isolation=hyperv`. Registry-writing scripts
  require `LOADWHAT_CONTAINER_TESTS=1`, an image-only marker,
  `ContainerType=2`, and `C:\WcSandboxState`.
- Added static MSVC runtime linking after the first container execution exposed
  a `vcruntime140.dll` dependency (`0xC0000135`). The release executable now
  starts in Server Core without copying host runtime DLLs into the image.
- Ran `cargo check -p xtask`; passed.
- Ran `cargo build --release --locked`; passed. A recursive import scan of the
  release executable reported no missing imports and no `vcruntime140.dll`
  import.
- Ran `cargo xtask test-container`; passed with 12 end-to-end COM cases.
- Ran `cargo test --locked`; passed with 264 unit/default tests.
- Ran `cargo xtask test`; passed with 99 harness-backed integration tests.
- The passing cases cover HKLM ProgID/CLSID resolution, real 64/32-bit views,
  HKCU override, CurVer, TreatAs, quoted LocalServer32 parsing, healthy and
  missing-dependency servers, registry audit fallback, sidecar-manifest
  precedence, and a real x86 server image.
- Host registry sentinels passed before and after every Docker run. No host COM
  registry mutation was performed.

### 2026-07-09 - Codex Windows container setup success

- User enabled Windows optional features from elevated PowerShell:
  `Containers` completed with `RestartNeeded : False`; `Microsoft-Hyper-V-All`
  completed with `RestartNeeded : True`.
- User removed `C:\ProgramData\DockerDesktop\install-settings.json`; a backup
  exists at `C:\ProgramData\DockerDesktop\install-settings.json.bak-20260709002601`.
- The installed `Docker Desktop Installer.exe install ...` path printed
  `Missing package flag`, so the setup guide now says to use the downloaded full
  installer if repair is still needed.
- Ran `DockerCli.exe -Shutdown`; Docker shutdown completed according to
  `DockerCli.exe.log`.
- Restarted Docker Desktop with `Start-Process`.
- Ran `DockerCli.exe -SwitchWindowsEngine; docker info --format '{{.OSType}}'`;
  Docker reported `windows`.
- Ran `docker pull mcr.microsoft.com/windows/servercore:ltsc2022`; it passed
  with digest `sha256:a23b350061d76236e2c427e32175a2decfe3214200eee4ae9ee9cd9e98f26bf0`.
- Ran `docker context ls`; active context is `desktop-windows`.
- Ran `docker info`; Docker reports `OSType=windows`,
  `OperatingSystem=Microsoft Windows Version 25H2 (OS Build 26200.7462)`, and
  `Isolation=hyperv`.
- Ran `docker run --rm mcr.microsoft.com/windows/servercore:ltsc2022 cmd /c ver`;
  it passed and printed `Microsoft Windows [Version 10.0.20348.5256]`.
- No host COM registry mutation was performed.

## Decisions

### 2026-07-08 - Docker tests supersede COM behavior fixes

Docker/container COM testing is the first priority because it protects the host registry and gives confidence for registry-view behavior. COM behavior fixes should wait until at least a smoke container lane exists.

### 2026-07-08 - Host registry mutation is out of bounds for normal tests

Local `cargo test` and `cargo xtask test` must not mutate real host COM registry keys. Real registry mutation belongs in `cargo xtask test-container`.

### 2026-07-08 - Container scripts must be self-guarding

Registry fixture scripts must fail closed when invoked outside the container
test environment. The runner must not rely only on agent discipline or file
location to protect the host registry.

## Blocked Items

No blockers recorded yet.

Potential blockers to check early:

- Docker Desktop not installed.
- Windows containers not enabled.
- Host/container Windows version mismatch.
- Windows container cannot run debug APIs or required COM operations.
- x86 MSVC tools unavailable for x86 fixtures.

Active local blocker:

- No active Docker setup blocker for initial container work. A reboot is
  recommended because Windows reported `RestartNeeded : True` after enabling
  Hyper-V.
