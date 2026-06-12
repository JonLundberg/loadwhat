# loadwhat — Code Review Findings & Work Items

Review date: 2026-06-11. Scope: full source (`src/*.rs`, `xtask`, `build.rs`),
test harness, docs. Verification: `cargo test` (164 unit + 4 harness, all pass),
`cargo clippy --all-targets` (1 warning), one FFI claim checked experimentally.

This document is a work backlog for a follow-up agent. Each item is independently
actionable. Items are grouped by priority. File/line references were accurate at
review time — re-confirm before editing since line numbers drift.

---

## Context for the implementing agent

`loadwhat` is a Windows x64 Rust CLI that diagnoses DLL load failures via the
Win32 debug API. Zero runtime dependencies. The architecture is three phases:
Phase A (debug-loop runtime observation in `debug_run.rs`), Phase B (static PE
import BFS in `main.rs` + `search.rs` + `pe.rs`), Phase C (dynamic `LoadLibrary`
failure inference from loader-snaps debug strings in `main.rs`). Output is a
line-oriented `TOKEN key=value` contract (`emit.rs`).

The code is high quality — these are gaps and cleanups, not a rewrite. Preserve
the existing output-token contract and the deterministic tie-breaking behavior;
both are covered by tests that encode the v1 spec (`docs/loadwhat_spec_v1.md`).

---

## P1 — Correctness / user-visible behavior

### 1. Default (summary) mode is silent on several failure paths
**Where:** `run_command`, `src/main.rs` (summary emission at ~line 510).
**Problem:** In summary mode a line is printed only when a diagnosis is found or
`code == 0`. These cases print **nothing** and exit non-zero:
- Target runs and exits non-zero with no diagnosable load issue → exit 21, silent.
- Target dies of a non-loader exception (e.g. access violation in `main`) → silent.
- IFEO-fallback enable failure (~line 104-116) emits a `NOTE` only in trace mode,
  so a registry-permission failure is fully silent in default mode.
**Fix:** Emit a single summary token (or stderr line) for the "ran, no load issue,
exit code N" and "non-loader exception" cases so default mode is never silent on
failure. Update `README.md` accordingly.
**Tests:** Add integration cases asserting a non-empty stdout/known token for a
clean-nonzero-exit target and a crashing-in-main target.

### 2. Empty target arguments are dropped
**Where:** `quote_cmd_arg`, `src/debug_run.rs:413`.
**Problem:** An empty string arg returns unquoted, so it vanishes from the built
command line. `loadwhat run app.exe "" next` makes the target see shifted argv.
**Fix:** Treat `arg.is_empty()` as needing quotes (emit `""`).
**Tests:** Extend `build_command_line_*` unit tests with an empty-arg case.

### 3. IFEO loader-snaps state can leak permanently
**Where:** `LoaderSnapsGuard` / `Drop`, `src/loader_snaps.rs:102`; `Cargo.toml:27`.
**Problem:** The IFEO fallback writes machine-wide `GlobalFlag` under HKLM Image
File Execution Options for the image name. Restoration relies on `Drop`, but
release builds set `panic = "abort"` (Drop skipped on panic) and there is no
console-control (Ctrl+C) handler, so an interrupted run leaves loader-snaps
enabled for that exe indefinitely.
**Fix (minimum):** Document the risk prominently. **Better:** install a Ctrl+C /
console-control handler that runs restore before exit; consider whether the
release `panic = "abort"` is worth the lost cleanup.

### 4. `LOADWHAT_TEST_MODE` is active in release builds
**Where:** `test_mode_enabled`, `src/main.rs:1039`.
**Problem:** Every other test hook is gated behind `#[cfg(debug_assertions)]`, but
this one is not. A production binary with the env var set emits `LWTEST:` lines and
switches to a different exit-code contract (0/2/3).
**Fix:** Gate behind `#[cfg(debug_assertions)]` like the rest, or have `xtask`
exercise a debug binary so production never honors it.

### 5. Version-detection for the PEB offset is a no-op but advertised
**Where:** `select_ntglobalflag_offset`, `src/loader_snaps.rs:180`.
**Problem:** All three match arms return `0xBC`. `README.md` claims loader-snaps
setup uses "Windows version/build detection to pick the x64 offset."
**Fix:** Either implement real per-version differentiation or collapse the function
to a constant and remove the README claim.

### 6. `extract_first_hex_u32` can misparse a pointer as a status
**Where:** `src/main.rs:1629`.
**Problem:** Matches the first `0x` + 8 hex digits without checking the 9th char
isn't also hex, so a 16-digit address (e.g. `0x00007FFFECEF10F0`) appearing before
the status yields `status=0x00007FFF`. Latent today because real ldr-snaps lines
usually print bare addresses, but fragile.
**Fix:** Reject the match if the character after the 8 hex digits is also a hex
digit (i.e. require exactly a u32-width token).
**Tests:** Add a unit case with a 16-digit address preceding a real status code.

### 7. Debuggee keeps running after timeout
**Where:** debug loop, `src/debug_run.rs:157` (break on timeout).
**Problem:** On timeout the loop breaks without terminating the target; it only
dies when loadwhat exits. Phase B/C static diagnosis then races a live process.
Also `--timeout-ms 0` silently means "wait forever" and is undocumented.
**Fix:** Explicitly `TerminateProcess` on timeout before returning the outcome.
Document (or reject) `--timeout-ms 0`.

---

## P2 — Smaller correctness / honesty nits

### 8. `HKEY_LOCAL_MACHINE` constant differs from Windows headers
**Where:** `src/win.rs:40` — defined zero-extended (`0x80000002u32 as isize`).
Windows headers sign-extend to `0xFFFFFFFF80000002`. Verified experimentally that
advapi32 accepts both on current Windows, so this is not a live bug — but match the
header definition (`0x8000_0002u32 as i32 as isize`) for safety.

### 9. `is_app_local_path` name oversells its logic
**Where:** `src/main.rs:1559`. Returns true for any path not under the Windows
dirs (e.g. `C:\Program Files\Common Files`). Rename to reflect "app-relevant" or
tighten the predicate to actually mean app-local.

### 10. Nearly all timeouts report `SUCCESS`
**Where:** `run_result_code` timeout branch, `src/main.rs:989`. Because every
process loads ntdll within milliseconds, `Timeout && !loaded_modules.is_empty()`
is almost always true, so a hung app that never finished startup emits
`SUCCESS status=0`. Consider a distinct token (e.g. `TIMEOUT_PROGRESS`) so a hang
isn't reported as success. Behavior is documented but misleading.

---

## P3 — Maintainability / cleanup

### 11. Split `src/main.rs` (2,527 lines)
It mixes orchestration, the dynamic-candidate state machine, scoring heuristics,
and three test modules. Extract dynamic detection
(`detect_dynamic_missing_from_debug_strings` + ~15 helpers + the
`dynamic_missing_tests` module, ~800 lines) into its own module
(e.g. `src/dynamic.rs`). Pure refactor — keep behavior and tests identical.

### 12. Collapse duplicated emission blocks in `run_command`
- WOW64 `NOTE` emitted identically at ~line 142-157 and ~line 198-213.
- STATIC_MISSING/BAD_IMAGE emission duplicated at ~line 294-321 vs ~line 350-375.
- DYNAMIC_MISSING field-building duplicated at ~line 417-424 vs ~line 458-465.
Introduce small helpers and call them from both sites.

### 13. Remove dead scaffolding
- `env_path_override(&[])`, `src/main.rs:1734` — unconditionally returns `None`,
  threaded through every diagnosis call. Either wire up a real override or drop it.
- `--strict`, `src/cli.rs:117` — accepted as a silent no-op in both subcommands and
  absent from `usage()`. Remove or implement and document.

### 14. Hidden I/O inside a comparator
`prefer_runtime_observed_path`, `src/main.rs:975` calls `fs::canonicalize` (via
`normalize_module_visit_key`) on both paths per comparison, per loaded module.
Fine at current scale but it reads like a pure function. Consider caching the
normalized key.

### 15. `build.rs` hardcodes the version
`build.rs:8-9` sets `FileVersion`/`ProductVersion` to literal `"1.0.0"`. Use
`env!("CARGO_PKG_VERSION")` so it can't drift from `Cargo.toml`.

### 16. Clippy warning
`tests/harness/paths.rs:133` — `push_str("\n")` → `push('\n')`.
Run `cargo clippy --fix --test integration -p loadwhat`.

### 17. Doc typo
`docs/architecture.md` says `main.rs` is "~21K lines" — it's ~2.1K.

---

## P4 — Tests / repo hygiene

### 18. `run_loadwhat::run` silently implies test mode
**Where:** `tests/harness/run_loadwhat.rs:37`. `run` aliases the **test-mode**
runner (`LOADWHAT_TEST_MODE=1`). A test author reaching for the obvious `run` gets
the test-mode exit-code contract implicitly. Rename to `run_test_mode` and make the
public-contract runner the obvious default to prevent accidental drift.

### 19. Triplicated PE-builder test helpers
`build_test_pe` / `build_valid_pe` logic is duplicated across `src/pe.rs` tests,
`src/search.rs` tests, and the harness `pe_builder`. Consolidate into one shared
builder.

### 20. `Cargo.toml` metadata + dependency
Add `repository` and `rust-version` fields. `winres 0.1` is unmaintained; the
maintained fork is `winresource` if a swap is ever wanted (low priority).

---

## Suggested order

Start with **#1, #2, #4** (silent failure paths, dropped empty args, test-mode
gating) — highest user impact, low risk. Then the P2 honesty nits and the P3
refactors (#11/#12 make subsequent work easier). P4 is opportunistic.

## What NOT to change
- The `TOKEN key=value` output contract and field ordering (`emit.rs`) — externally
  depended upon and test-locked.
- The deterministic candidate tie-breaking in `detect_dynamic_missing_from_debug_strings`
  and `consider_first_issue` — encodes the v1 spec; refactors must keep tests green.
- The bounds-checked PE parser in `pe.rs` — already correct and well-tested.
