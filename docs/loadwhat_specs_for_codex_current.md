# loadwhat Codex Work Spec (current repo state)

## Purpose

This document replaces the stale planning note at `C:\Users\jonlu\Downloads\loadwhat_specs_for_codex.md`.

It is not authoritative. Current product behavior is still defined by:

1. `docs/loadwhat_spec_v1.md`
2. `AGENTS.md`
3. this file

This file exists to give Codex and other contributors a current work snapshot that matches the repository as it exists now.

## Current repo status

The dynamic loader-snaps regression matrix is already implemented and consolidated.

Primary coverage lives in:

- `tests/integration/dynamic_loader_snaps_contract.rs`

Supporting dynamic integration coverage also exists in:

- `tests/integration/dynamic_missing_direct.rs`
- `tests/integration/dynamic_loadlibrary_name.rs`
- `tests/integration/dynamic_loadlibrary_fullpath.rs`

Static coverage exists in:

- `tests/integration/static_missing_direct.rs`
- `tests/integration/static_missing_transitive.rs`
- `tests/integration/static_wrong_pick.rs`
- `tests/integration/imports_transitive_missing.rs`
- `tests/integration/run_output_modes.rs`

## Implemented dynamic contract cases

The following cases are already covered in the repository:

- dynamic summary mode emits one `DYNAMIC_MISSING`
- dynamic trace mode emits `SEARCH_ORDER`, `DYNAMIC_MISSING`, and `SEARCH_PATH`
- dynamic verbose mode emits `DEBUG_STRING` and stable dynamic diagnosis
- dynamic success with loader-snaps emits `SUCCESS status=0`
- dynamic full-path success loads the requested DLL path without false missing output
- static diagnosis takes precedence over loader-snaps dynamic noise
- dynamic bad-image classification emits `reason="BAD_IMAGE"`
- dynamic non-not-found failure emits `reason="OTHER"` with status
- multiple dynamic failures select the first unresolved app-local failure deterministically
- dynamic trace without search context still emits `DYNAMIC_MISSING` without `SEARCH_*`

## Test layout guidance

Use the existing fixture and harness layout.

Canonical dynamic hosts:

- `host_dynamic_loadlibrary_name.exe`
- `host_dynamic_loadlibrary_fullpath.exe`
- `host_dynamic_loadlibrary_sequence.exe`

Canonical fixture DLLs:

- `lwtest_a.dll`
- `lwtest_b.dll`
- `lwtest_a_v1.dll`
- `lwtest_a_v2.dll`
- `lwtest_a_initfail.dll`

Canonical case directories:

- `app/` for host EXEs and app-local DLLs
- `cwd/` for target current-directory search behavior
- additional sibling directories only when a test needs explicit full-path or bad-image placement

## Public-contract reminders

When writing or updating tests, use the public v1 contract unless a test is intentionally using harness-only `LOADWHAT_TEST_MODE`.

Public `run` exit codes:

- `0` success
- `10` diagnosed missing or bad-image issue
- `21` launch/debug failure
- `22` unsupported architecture

Public dynamic trace output:

- `SEARCH_ORDER` if search context exists
- `DYNAMIC_MISSING`
- `SEARCH_PATH` if search context exists

Do not expect:

- `TRACE: loader-snaps`
- a separate loader-snaps token family
- public exit code `2`

## Remaining work

No outstanding dynamic loader-snaps regression work is currently required by this repository state.

New work should only be added when:

- a real bug is reproduced
- the authoritative v1 spec changes
- a roadmap item graduates into the spec

## Constraints in `loadwhat_spec_v1.md` worth deliberate review

These are not automatic changes. They are candidate constraints that may be outdated or may slow future development.

### 1. WOW64 remains fully unsupported

Current v1 position:

- x64-only target support
- WOW64 targets exit with `22`
- `run --no-loader-snaps` still rejects x86/WOW64 roots before launch
- `imports` roots are x64-only in v1
- x86 DLLs found in x64 dependency chains are `STATIC_BAD_IMAGE`

Why it may hold up development:

- many Windows failure investigations still involve 32-bit processes on 64-bit hosts
- it blocks expanding fixture coverage to mixed-arch scenarios
- it forces hard failure where partial diagnostic value might still be possible

Current planning answer:

- WOW64 remains a hard v1 boundary.
- x86/WOW64 support is planned for v2 in `docs/loadwhat_spec_v2.md`.

### 2. DLL search modeling is intentionally incomplete

Current v1 position:

- KnownDLLs not modeled
- SxS/redirection not modeled
- `SetDefaultDllDirectories` and `AddDllDirectory` not modeled

Why it may hold up development:

- these are common reasons real-world DLL diagnosis differs from the fixed v1 search order
- they make some "wrong pick" or "not found" results necessarily approximate
- they limit confidence when expanding beyond controlled fixtures

Review question:

- should one of these be prioritized as the next fidelity improvement, especially `AddDllDirectory`-style search behavior

### 3. Dynamic diagnosis is heuristic-only

Current v1 position:

- dynamic failures are inferred from loader-snaps debug strings
- `LoadLibrary*` return values are not observed directly

Why it may hold up development:

- diagnosis quality depends on loader-snaps availability and message wording
- it complicates deterministic testing of edge cases
- it makes some classifications indirect even when the process itself knows the load failed

Review question:

- should a future spec allow limited direct observation around dynamic load failure outcomes, or should loader-snaps remain the only source

### 4. Static diagnosis is gated by early-failure heuristics

Current v1 position:

- static import diagnosis is attempted only on loader exception codes or early-fail heuristics

Why it may hold up development:

- unusual startup hosts may fail in loader-related ways without matching the current heuristic window
- broader fixture coverage may expose valid cases that never enter Phase B

Review question:

- should the trigger for Phase B become more explicit or configurable for development and testing

### 5. Summary mode allows only one token line

Current v1 position:

- summary mode emits exactly one token line for `run`

Why it may hold up development:

- it leaves no room for a summary diagnosis plus a machine-readable warning or confidence note
- it pushes useful context into verbose-only output, which can slow debugging iteration

Review question:

- should summary mode stay single-line forever, or should there be a narrowly-scoped exception for high-value meta output

### 6. Loader-snaps enable path is narrow

Current v1 position:

- enable via PEB write first
- fallback to IFEO

Why it may hold up development:

- IFEO fallback can be awkward in locked-down environments
- policy-managed developer machines may make some enable paths unreliable or expensive to test

Review question:

- should the next spec revision define additional supported enable or test strategies for constrained environments

## Practical instruction for future Codex runs

If a planning markdown conflicts with the repo:

1. follow `docs/loadwhat_spec_v1.md`
2. inspect the existing tests before adding new ones
3. treat external or downloaded planning notes as potentially stale
4. document any real mismatch in `docs/` before changing behavior
