# Test Implementation Plan

This document specifies new integration tests for `loadwhat`. Each test has enough context for an AI agent (Claude Code, Codex, etc.) to implement it without further guidance.

---

## How To Use This Document

1. Pick a test from any section below.
2. Create the file listed in **File**.
3. Register it in `tests/integration.rs` following the existing pattern.
4. Implement the test following the acceptance criteria exactly.
5. Run via `cargo xtask test` (requires the harness environment).

---

## Project Context

### Architecture in One Paragraph

`loadwhat` is a Windows x64 CLI that diagnoses DLL loading failures. It runs in three phases: **Phase A** launches the target under a debug loop and captures loaded modules + debug strings. **Phase B** does a static BFS walk of PE import tables to find missing/bad-image DLLs. **Phase C** heuristically infers `LoadLibrary` failures from captured loader-snaps debug strings. Phases B and C are fallbacks — B triggers only on early failure, C triggers only if B found nothing.

### Test Infrastructure

All integration tests live in `tests/integration/` and are registered in `tests/integration.rs` with:

```rust
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/your_new_file.rs"]
mod your_new_file;
```

Every test file starts with:

```rust
use crate::harness;
use std::ffi::OsString;
use std::time::Duration;
```

### Key Harness APIs

| API | Purpose |
|-----|---------|
| `harness::paths::require_from_env()` | Get `HarnessPaths` (panics with setup instructions if env missing) |
| `harness::case::TestCase::new(&paths, "name")` | Create isolated test directory (auto-cleaned on drop) |
| `case.mkdir("subdir")` | Create subdirectory, returns `PathBuf` |
| `case.copy_fixture(FIXTURE_CONST, "relative/path")` | Copy pre-built fixture to test dir |
| `case.copy_fixture_as(FIXTURE, "dir", "new_name.dll")` | Copy fixture with rename |
| `harness::case::os(&path)` | Convert `&Path` to `OsString` for arg building |
| `harness::pe_builder::write_import_test_pe(&path, &["dll1.dll", "dll2.dll"])` | Write synthetic PE with specified imports |
| `harness::pe_builder::build_import_test_pe(&["dll.dll"])` | Build synthetic PE bytes in memory (for manual mutation) |
| `harness::run_loadwhat::run_public(&paths, cwd, &args, timeout)` | Run loadwhat without `LOADWHAT_TEST_MODE` (public output only) |
| `harness::run_loadwhat::run_public_with_env(&paths, cwd, &args, timeout, &[("K","V")])` | Run with custom env vars |
| `harness::run_loadwhat::run(&paths, cwd, &args, timeout)` | Run with `LOADWHAT_TEST_MODE=1` (emits `LWTEST:*` internal tokens) |
| `harness::assert::assert_exit_code(&result, code)` | Assert loadwhat exit code |
| `harness::assert::assert_not_timed_out(&result)` | Assert loadwhat didn't timeout |
| `harness::assert::assert_missing_dll(&stdout, "dll.dll")` | Assert `LWTEST:RESULT kind=missing_dll name=dll.dll` present |
| `harness::assert::assert_no_missing_result(&stdout)` | Assert no `LWTEST:RESULT kind=missing_dll` present |
| `harness::assert::assert_target_exit_code(&stdout, code)` | Assert `LWTEST:TARGET exit_code=N` present |
| `harness::assert::assert_loaded_path(&stdout, "dll.dll", &path)` | Assert `LWTEST:LOAD` with normalized path match |

### Available Fixtures

| Constant | Description |
|----------|-------------|
| `HOST_STATIC_IMPORTS_A_EXE` | EXE that statically imports `lwtest_a.dll` (no missing deps when provided) |
| `HOST_STATIC_IMPORTS_MISSING_EXE` | EXE that statically imports `lwtest_a.dll` (designed to test missing) |
| `HOST_STATIC_A_DEPENDS_ON_B_EXE` | EXE → `lwtest_a.dll` → `lwtest_b.dll` (chain) |
| `HOST_DYNAMIC_LOADLIBRARY_NAME_EXE` | EXE that calls `LoadLibrary` by name (DLL name passed as arg) |
| `HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE` | EXE that calls `LoadLibrary` with full path (path passed as arg) |
| `HOST_DYNAMIC_LOADLIBRARY_NESTED_EXE` | EXE with nested `LoadLibrary` calls |
| `HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE` | EXE that calls `LoadLibrary` on each arg in sequence. Special prefixes: `optional:` (try then continue), `sleep:N` (sleep N ms) |
| `HOST_ECHO_ARGV_CWD_EXE` | EXE that prints its CWD and args. Supports `--lwtest-exit-code N` to exit with specific code |
| `DLL_LWTEST_A` | Standard test DLL (no transitive deps) |
| `DLL_LWTEST_A_V1` / `DLL_LWTEST_A_V2` | Versioned variants of DLL A |
| `DLL_LWTEST_A_INITFAIL` | DLL whose `DllMain` returns `FALSE` |
| `DLL_LWTEST_A_NESTED` | DLL that itself calls `LoadLibrary` |
| `DLL_LWTEST_B` | Standard test DLL B |

### Synthetic PE Builder

`harness::pe_builder::write_import_test_pe(path, &["imports..."])` creates a minimal x64 PE file with the specified import table. These PEs are structurally valid but are **not executable** — they're for `loadwhat imports` (static analysis) only, not `loadwhat run`.

`harness::pe_builder::build_import_test_pe(imports)` returns `Vec<u8>` for manual mutation before writing.

### `token_lines` Helper

Most test files define a local `token_lines` function. Copy this into your new file:

```rust
fn token_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty()
                && (line.starts_with("STATIC_")
                    || line.starts_with("DYNAMIC_")
                    || line.starts_with("SEARCH_")
                    || line.starts_with("RUN_")
                    || line.starts_with("RUNTIME_")
                    || line.starts_with("FIRST_BREAK")
                    || line.starts_with("SUMMARY")
                    || line.starts_with("SUCCESS")
                    || line.starts_with("NOTE ")
                    || line.starts_with("DEBUG_STRING"))
        })
        .collect()
}
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (or timeout with load progress) |
| 10 | Diagnosis found (public mode) |
| 20 | CLI parse error |
| 21 | Runtime error / no diagnosis applicable |
| 22 | Unsupported architecture (WOW64) |

### Output Token Families

- **Summary mode** (default): Single line — `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `DYNAMIC_MISSING`, or `SUCCESS status=0`
- **Trace mode** (`--trace`): Adds `SEARCH_ORDER`, `SEARCH_PATH` lines
- **Verbose mode** (`-v`): Adds `RUN_START`, `RUNTIME_LOADED`, `RUN_END`, `DEBUG_STRING`, `FIRST_BREAK`, `SUMMARY`

### Internal Test Mode

When `LOADWHAT_TEST_MODE=1` (used by `run()` / `run_with_env()`), loadwhat emits extra `LWTEST:*` lines:
- `LWTEST:RESULT kind=missing_dll name=<dll>` — first detected missing lwtest_* dll
- `LWTEST:TARGET exit_code=<N>` — target process exit code
- `LWTEST:LOAD name=<dll> path=<path>` — loaded lwtest_* dll with path

---

## Test Specifications

---

### File: `tests/integration/malformed_pe_handling.rs`

Tests for malformed and corrupted PE files through both `imports` and `run` pipelines.

---

#### Test 1: `imports_truncated_pe_header_fails_cleanly`

**What:** A PE file with valid MZ signature but the PE offset (`0x3C`) points past end-of-file.

**Setup:**
1. Use `build_import_test_pe(&["kernel32.dll"])` to get a valid PE in memory.
2. Truncate the `Vec<u8>` to 96 bytes (past MZ header but before PE header at offset `0x80`).
3. Write to disk as `truncated.exe`.
4. Run `loadwhat imports truncated.exe`.

**Acceptance Criteria:**
- Exit code: **21**
- No `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `SUMMARY`, or `DYNAMIC_*` tokens in output.
- loadwhat must not panic (process completes normally).

---

#### Test 2: `imports_corrupted_import_table_rva_fails_cleanly`

**What:** A PE file where the import directory RVA points outside all sections.

**Setup:**
1. Use `build_import_test_pe(&["kernel32.dll"])` to get valid PE bytes.
2. Overwrite the import directory RVA at byte offset `0x88 + 112 + 8 = 0x100` (which is `DATA_DIR_START + 8` = `OPTIONAL_HEADER_OFFSET + 112 + 8`) with value `0xFFFF0000` (unmappable RVA).
   - More precisely: the import directory RVA lives at offset `IMPORT_DIRECTORY_RVA_OFFSET` which is `0x80 + 24 + 112 + 8 = 0x100`. Write `0xFFFF0000u32` in little-endian at bytes `[0x100..0x104]`.
3. Write to disk as `bad_rva.exe`.
4. Run `loadwhat imports bad_rva.exe`.

**Acceptance Criteria:**
- Exit code: **21**
- No `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `SUMMARY`, or `DYNAMIC_*` tokens.
- Must not panic.

---

#### Test 3: `imports_transitive_corrupt_pe_skips_node_and_continues`

**What:** An import chain `root.exe → good.dll → corrupt.dll` where `corrupt.dll` has valid MZ+PE headers but a corrupted import table. Phase B should report `corrupt.dll` as `BAD_IMAGE` or skip it gracefully.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["good.dll"])`.
2. Create `good.dll` via `write_import_test_pe(&["corrupt.dll"])`.
3. Create `corrupt.dll`: write `build_import_test_pe(&["kernel32.dll"])`, then corrupt the import RVA (same technique as Test 2) and write to disk.
4. Place all three in the same directory.
5. Run `loadwhat imports root.exe --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_BAD_IMAGE` with `dll="corrupt.dll"`.
- loadwhat must not panic or hang.
- The `good.dll` node should be walked successfully (it appears in `STATIC_IMPORT` or `STATIC_FOUND` if trace mode used).

---

#### Test 4: `run_malformed_target_exe_exits_cleanly`

**What:** Running `loadwhat run` on a file that is not a valid PE at all.

**Setup:**
1. Write raw bytes `b"this is definitely not a PE file"` to `broken.exe`.
2. Run `loadwhat run broken.exe`.

**Acceptance Criteria:**
- Exit code: **21**
- No `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `DYNAMIC_*` tokens.
- Must not panic.

**Note:** This tests the `run` path (not `imports`). The existing `imports_malformed_root_fails_cleanly_without_diagnosis_tokens` test covers the `imports` path. This verifies `run` has the same clean failure.

---

#### Test 5: `imports_junk_bytes_dll_in_chain_reports_bad_image`

**What:** `root.exe → lwtest_a.dll` where `lwtest_a.dll` is junk bytes (not PE at all). This is what the existing bad_image tests do, but this test uses a synthetic PE root and runs through `imports` (not `run`).

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["lwtest_a.dll"])`.
2. Write `b"not a pe image"` as `lwtest_a.dll` in same directory.
3. Run `loadwhat imports root.exe --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_BAD_IMAGE` with `dll="lwtest_a.dll"` and `reason="BAD_IMAGE"`.
- No `DYNAMIC_*` tokens (imports command never runs dynamic analysis).

---

#### Test 6: `imports_pe32_dll_in_x64_chain_reports_bad_image`

> **SKIP — Needs answer:** Does `pe::is_probably_pe_file` check machine type (0x8664 for x64 vs 0x14C for x86)? If it only checks structural validity, a PE32 file would be classified as `Found` not `BadImage`, making this test incorrect. **Verify by reading `is_probably_pe_file` in `src/pe.rs` before implementing.** If it doesn't check machine type, this test should be filed as a feature request instead.

---

### File: `tests/integration/post_init_crash.rs`

Tests that non-loader crashes do not produce false DLL diagnoses.

---

#### Test 7: `run_post_init_access_violation_does_not_diagnose_dll`

**What:** A target that loads all its DLLs successfully, then crashes with an access violation. Loadwhat should NOT trigger Phase B (this isn't a loader failure) and should NOT emit false `STATIC_MISSING` or `DYNAMIC_MISSING`.

**Setup:**
1. Use `HOST_ECHO_ARGV_CWD_EXE` with `--lwtest-exit-code 3221225477` (0xC0000005 = ACCESS_VIOLATION as unsigned i32 wrapping).

> **SKIP — Needs answer:** Does `HOST_ECHO_ARGV_CWD_EXE` support triggering an actual access violation (SEH exception), or only `ExitProcess(code)`? If it only calls `ExitProcess`, the exception code in the debug loop will be different from a real AV. The existing test `unrelated_non_loader_failure_does_not_invent_dll_diagnoses` uses exit code 7 — this tests a higher exit code. **A new fixture that deliberately dereferences null may be needed for a true AV test.** If the echo fixture only supports `ExitProcess`, implement this test using `--lwtest-exit-code` with a large non-loader code (e.g., `42`) and note that a real AV fixture is a future TODO.

**Fallback implementation** (using echo fixture with non-loader exit code):

1. Copy `HOST_ECHO_ARGV_CWD_EXE` to `app/`.
2. Run `loadwhat run <exe> --lwtest-exit-code 42`.
3. Assert:
   - Exit code: **21** (no diagnosis applicable)
   - No `STATIC_*` or `DYNAMIC_*` tokens

**Acceptance Criteria (fallback):**
- Exit code: **21**
- No `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `DYNAMIC_MISSING` in output.
- `token_lines` may contain `SUCCESS` or nothing — either is acceptable as long as no false DLL diagnosis appears.

---

### File: `tests/integration/imports_on_dll.rs`

Tests that `loadwhat imports` works on DLL files, not just EXEs.

---

#### Test 8: `imports_on_dll_with_missing_transitive_dep`

**What:** Run `loadwhat imports` on a DLL (not an EXE) that has a missing transitive dependency.

**Setup:**
1. Create `root.dll` via `write_import_test_pe(&["child.dll"])`.
2. Create `child.dll` via `write_import_test_pe(&["missing.dll"])`.
3. Do NOT create `missing.dll`.
4. Place `root.dll` and `child.dll` in same directory.
5. Run `loadwhat imports root.dll --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_MISSING` with `dll="missing.dll"`.
- `STATIC_MISSING` includes `via="child.dll"` and `depth=2`.
- No runtime tokens (`RUN_START`, `RUNTIME_LOADED`, etc.).

---

#### Test 9: `imports_on_dll_with_no_issues`

**What:** Run `loadwhat imports` on a DLL whose entire import chain is satisfied.

**Setup:**
1. Create `root.dll` via `write_import_test_pe(&["kernel32.dll"])`.
2. Run `loadwhat imports root.dll`.

**Acceptance Criteria:**
- Exit code: **0**
- `SUMMARY` line contains `static_missing=0` and `static_bad_image=0`.
- No `STATIC_MISSING` or `STATIC_BAD_IMAGE` tokens.

---

### File: `tests/integration/phase_b_trigger_heuristic.rs`

Tests the Phase B early-exit heuristic boundary conditions.

---

#### Test 10: `run_nonzero_exit_after_long_runtime_skips_phase_b`

**What:** A target that runs for 2+ seconds then exits with code 1. The elapsed time exceeds the 1500ms heuristic threshold, so Phase B should NOT trigger.

**Setup:**
1. Use `HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE`.
2. Copy `DLL_LWTEST_A` to app dir as a dependency.
3. Args: `run --cwd <app_dir> <exe> sleep:2000` — this loads OK then sleeps 2s.

> **SKIP — Needs answer:** `HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE` exits with code 0 after completing its sequence. To test Phase B heuristic suppression we need a target that runs > 1.5s AND exits non-zero. Does the sequence host support a way to set exit code? (e.g., via an `exit:N` arg prefix.) If not, a new fixture or fixture enhancement is needed. **Check the fixture source before implementing.**

---

#### Test 11: `run_fast_exit_with_many_modules_skips_phase_b`

**What:** A target exits with code 1 in < 1.5s but loads more than 6 modules. The module-count check (`<= 6`) should prevent Phase B from triggering.

> **SKIP — Needs answer:** Same fixture limitation as Test 10. Also, loading > 6 distinct DLLs requires either many test DLLs or a fixture that loads system DLLs explicitly. **Defer until fixture capabilities are extended.**

---

### File: `tests/integration/static_circular_dependency.rs`

---

#### Test 12: `imports_circular_dependency_terminates`

**What:** Two DLLs that import each other: `a.dll → b.dll` and `b.dll → a.dll`. The BFS visited set should prevent infinite recursion.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["a.dll"])`.
2. Create `a.dll` via `write_import_test_pe(&["b.dll"])`.
3. Create `b.dll` via `write_import_test_pe(&["a.dll"])`.
4. Place all in same directory.
5. Run `loadwhat imports root.exe --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **0** (all DLLs found, no issues)
- `SUMMARY` line contains `static_missing=0` and `static_bad_image=0`.
- loadwhat terminates (does not hang). Use a 20-second timeout on the harness runner.
- Run twice and assert output is identical (deterministic).

---

### File: `tests/integration/static_deep_chain.rs`

---

#### Test 13: `imports_deep_transitive_chain_reports_correct_depth`

**What:** A chain 5 levels deep: `root.exe → a.dll → b.dll → c.dll → d.dll → missing.dll`. Verify depth tracking is correct.

**Setup:**
1. Create each PE with `write_import_test_pe`:
   - `root.exe` imports `["a.dll"]`
   - `a.dll` imports `["b.dll"]`
   - `b.dll` imports `["c.dll"]`
   - `c.dll` imports `["d.dll"]`
   - `d.dll` imports `["missing.dll"]`
2. Do NOT create `missing.dll`.
3. Place all in same directory.
4. Run `loadwhat imports root.exe --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_MISSING` with `dll="missing.dll"`, `via="d.dll"`, `depth=5`.
- No panic, no hang.

---

### File: `tests/integration/static_multiple_missing_at_same_depth.rs`

---

#### Test 14: `imports_multiple_missing_at_same_depth_selects_lexicographic_first`

**What:** A root that imports two DLLs, both of which are missing. The first-issue selection should pick the lexicographically first one.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["z_missing.dll", "a_missing.dll"])`.
2. Do NOT create either DLL.
3. Run `loadwhat imports root.exe --cwd <dir>` (default summary mode).

**Acceptance Criteria:**
- Exit code: **10**
- The first `STATIC_MISSING` line references `dll="a_missing.dll"` (alphabetically first).
- In **trace mode** (`imports --cwd <dir> root.exe` with output parsed): both `a_missing.dll` and `z_missing.dll` should appear as `STATIC_MISSING`.

**Implementation Note:** Run twice — once in default mode to check first-issue selection, once with trace output to verify both are reported. The `imports` command always emits in Full mode, so a single run should show both STATIC_MISSING lines. Check the output for both.

---

### File: `tests/integration/search_dedup_app_equals_cwd.rs`

---

#### Test 15: `imports_app_dir_equals_cwd_no_duplicate_search_paths`

**What:** When app directory and CWD are the same path, search order should not list the directory twice.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["missing.dll"])` in a directory.
2. Run `loadwhat imports root.exe --cwd <same_dir_as_exe>`.
3. Parse output for `SEARCH_PATH` lines.

**Acceptance Criteria:**
- Exit code: **10** (missing.dll not found)
- In the `SEARCH_PATH` lines for `dll="missing.dll"`, the app directory path should appear at most **once**.
- No duplicate paths in the search order output.

**Implementation Note:** The `imports` command emits `SEARCH_PATH` lines by default (Full emit mode). Filter for lines matching `SEARCH_PATH` and `dll="missing.dll"`, extract the `path=` field values, normalize them, and assert no duplicates.

---

### File: `tests/integration/dynamic_all_later_loaded.rs`

---

#### Test 16: `dynamic_all_candidates_later_loaded_emits_no_dynamic_missing`

**What:** A target attempts to load a DLL, fails initially, then loads it successfully from a different path. Phase C should discard the failure candidate because the DLL was later loaded.

**Setup:**
1. Use `HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE`.
2. Create `app/` dir with the exe.
3. Create `good/lwtest_probe.dll` (copy from `DLL_LWTEST_A_V1`).
4. Args: `run --cwd <app_dir> <exe> optional:lwtest_probe.dll <full_path_to_good/lwtest_probe.dll>`
   - First call: `LoadLibrary("lwtest_probe.dll")` — fails (not in app dir)
   - Second call: `LoadLibrary("<full_path>")` — succeeds
5. Copy `DLL_LWTEST_B` to `app/` as `lwtest_b.dll` if needed for transitive deps.

**Acceptance Criteria:**
- Exit code: **0**
- Output: `SUCCESS status=0` (single line)
- No `DYNAMIC_MISSING` token in output.

---

### File: `tests/integration/cli_validation_edge_cases.rs`

---

#### Test 17: `cli_missing_target_returns_exit_20`

**What:** Running `loadwhat run` with no target argument should produce a CLI parse error.

**Setup:**
1. Run `loadwhat run` (no target).

**Acceptance Criteria:**
- Exit code: **20**
- stderr contains "missing target executable" (case-insensitive check).
- No `STATIC_*`, `DYNAMIC_*`, `SUCCESS` tokens in stdout.

---

#### Test 18: `cli_nonexistent_target_returns_exit_21`

**What:** Running `loadwhat run` on a path that does not exist.

**Setup:**
1. Run `loadwhat run C:\nonexistent_path_12345\fake.exe`.

**Acceptance Criteria:**
- Exit code: **21**
- No `STATIC_*`, `DYNAMIC_*`, `SUCCESS` tokens in stdout.
- Must not panic.

---

#### Test 19: `cli_timeout_zero_is_accepted`

> **SKIP — Needs answer:** What is the expected behavior for `--timeout-ms 0`? Looking at the source, `timeout_ms == 0` means "no timeout" (waits indefinitely with 250ms poll). If the target exits quickly this is fine, but this needs confirmation. **If `0` means no timeout, this test should use a fast-exiting target and assert success. If `0` means immediate timeout, it should assert `SUCCESS` with `exit_kind="TIMEOUT"`.** Verify the `run_target` function behavior before implementing.

---

#### Test 20: `cli_very_large_timeout_is_accepted`

**What:** `--timeout-ms 4294967295` (u32::MAX) should parse successfully.

**Setup:**
1. Use `HOST_ECHO_ARGV_CWD_EXE` (exits quickly).
2. Args: `run --timeout-ms 4294967295 <exe>`.

**Acceptance Criteria:**
- Exit code: **0** (target exits successfully, timeout doesn't fire).
- Output: `SUCCESS status=0`.

---

#### Test 21: `cli_timeout_overflow_returns_parse_error`

**What:** `--timeout-ms 4294967296` (u32::MAX + 1) should fail parsing.

**Setup:**
1. Args: `run --timeout-ms 4294967296 <exe>`.

**Acceptance Criteria:**
- Exit code: **20**
- stderr contains "invalid --timeout-ms value" (case-insensitive).

**Implementation Note:** You still need to provide a valid exe path after the timeout flag. Use `HOST_ECHO_ARGV_CWD_EXE`.

---

### File: `tests/integration/dynamic_transitive_init_failure.rs`

---

#### Test 22: `dynamic_transitive_init_failure_diagnosed_by_phase_c`

**What:** A → B where B's DllMain returns FALSE. The failure is transitive (A dynamically loads B). Phase C should diagnose B's init failure.

**Setup:**
1. Use `HOST_DYNAMIC_LOADLIBRARY_NESTED_EXE`.
2. Copy `DLL_LWTEST_A_NESTED` as `lwtest_a.dll` in app dir.
3. Copy `DLL_LWTEST_A_INITFAIL` as `lwtest_b.dll` in app dir (this is the DLL that `lwtest_a_nested` will try to load).

> **SKIP — Needs answer:** Does `DLL_LWTEST_A_NESTED` load `lwtest_b.dll` by name? What DLL name does it attempt to load? **Check the fixture source to determine the exact DLL name the nested loader requests.** The harness fixture `DLL_LWTEST_A_NESTED` description says "DLL that itself calls LoadLibrary" but the target name isn't documented. If it loads `lwtest_b.dll`, this test works as described. Otherwise adjust the filename.

---

### File: `tests/integration/search_path_edge_cases.rs`

---

#### Test 23: `imports_path_with_empty_segments_does_not_crash`

**What:** A PATH environment variable with empty segments (`C:\a;;C:\b`) should not cause a crash or incorrect search behavior.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["lwtest_a.dll"])`.
2. Create directory `path_a/` and `path_b/` inside test case root.
3. Copy `DLL_LWTEST_A` to `path_b/lwtest_a.dll`.
4. Build PATH as: `"<path_a>;;<path_b>"` (note the empty segment `;;`).
5. Run `loadwhat imports root.exe --cwd <dir>` with custom PATH env.

**Acceptance Criteria:**
- Exit code: **0** (dll found via path_b)
- `SUMMARY` contains `static_missing=0`.
- No panic or crash.

---

#### Test 24: `imports_bad_image_in_search_path_stops_search`

**What:** A bad-image DLL is found earlier in the search path than a valid copy. The search model returns `BAD_IMAGE` immediately without continuing.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["target.dll"])`.
2. Create `early/target.dll` as junk bytes (`b"not pe"`).
3. Create `late/target.dll` as valid PE via `write_import_test_pe(&[])`.
4. Build PATH with early before late: `"<early_dir>;<late_dir>"`.
5. Run `loadwhat imports root.exe --cwd <root_exe_dir>` with custom PATH.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_BAD_IMAGE` with `dll="target.dll"`.
- No `STATIC_FOUND` for `target.dll` (the later valid copy should NOT be found — search stops at bad image).
- If trace output is examined, `SEARCH_PATH` for `target.dll` should show the early path with `result="BAD_IMAGE"`.

---

### File: `tests/integration/imports_stability.rs`

---

#### Test 25: `imports_output_is_deterministic_across_runs`

**What:** Running `loadwhat imports` twice on the same input produces identical output.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["kernel32.dll", "a.dll", "b.dll"])`.
2. Create `a.dll` via `write_import_test_pe(&["kernel32.dll"])`.
3. Create `b.dll` via `write_import_test_pe(&["kernel32.dll"])`.
4. Place all in same directory.
5. Run `loadwhat imports root.exe --cwd <dir>` twice.

**Acceptance Criteria:**
- Both runs produce identical stdout.
- Both runs produce exit code **0**.
- `STATIC_IMPORT` lines for `root.exe` are in alphabetical order: `a.dll`, `b.dll`, `kernel32.dll`.

**Note:** The existing `imports_static_import_order_is_lexicographic_and_stable` test is similar but uses different imports. This test exercises the multi-module BFS walk stability, not just import ordering.

---

### File: `tests/integration/shared_bad_image_dedup.rs`

---

#### Test 26: `imports_shared_bad_image_dep_reported_once`

**What:** Two modules both import the same bad-image DLL. The `STATIC_BAD_IMAGE` should appear only for the first encounter in BFS order.

**Setup:**
1. Create `root.exe` via `write_import_test_pe(&["a.dll", "b.dll"])`.
2. Create `a.dll` via `write_import_test_pe(&["shared_bad.dll"])`.
3. Create `b.dll` via `write_import_test_pe(&["shared_bad.dll"])`.
4. Write `shared_bad.dll` as junk bytes.
5. Place all in same directory.
6. Run `loadwhat imports root.exe --cwd <dir>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains exactly **one** `STATIC_BAD_IMAGE` line for `shared_bad.dll`.
- The `module=` field in that line should reference whichever parent is visited first in BFS (either `a.dll` or `b.dll` — but it must be stable across runs).
- Run twice and assert both outputs are identical.

---

### File: `tests/integration/dynamic_bad_image_not_found_precedence.rs`

---

#### Test 27: `dynamic_not_found_preferred_over_bad_image_at_same_depth`

**What:** When Phase C sees both a NOT_FOUND and BAD_IMAGE failure, NOT_FOUND should be preferred (higher confidence for user action).

**Setup:**
1. Use `HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE`.
2. Create a bad image file `app/bad_image.dll` (junk bytes).
3. Do NOT create `lwtest_missing.dll`.
4. Args: sequence loads `bad_image.dll` (full path) then `lwtest_missing.dll` (by name).
   - `run --cwd <app_dir> <exe> <full_path_to_bad_image.dll> lwtest_missing.dll`

> **SKIP — Needs answer:** The Phase C candidate ranking sorts by `DynamicCandidateKind` first, then score. Both NOT_FOUND and BAD_IMAGE can produce `UnableToLoadDll` kind depending on the debug string pattern. The actual precedence depends on which debug strings the Windows loader emits for each failure type. **This test requires verifying what debug strings Windows actually produces for each failure mode.** Implement after capturing real loader-snaps output for both failure types on the target Windows version.

---

### File: `tests/integration/run_success_edge_cases.rs`

---

#### Test 28: `run_target_exits_zero_with_no_app_dlls_reports_success`

**What:** A target that loads only system DLLs (kernel32, ntdll, etc.) and exits with code 0.

**Setup:**
1. Use `HOST_ECHO_ARGV_CWD_EXE` (has no lwtest_* dependencies).
2. Args: `run <exe>` (no extra args, exits with code 0).

**Acceptance Criteria:**
- Exit code: **0**
- Output: `SUCCESS status=0` (single line).
- No `STATIC_*` or `DYNAMIC_*` tokens.

---

#### Test 29: `run_success_with_all_deps_present_emits_success`

**What:** A target where every static dependency is present.

**Setup:**
1. Use `HOST_STATIC_IMPORTS_A_EXE`.
2. Copy `DLL_LWTEST_A` to app dir as `lwtest_a.dll`.
3. Copy `DLL_LWTEST_B` to app dir as `lwtest_b.dll` (if A depends on B).
4. Args: `run --cwd <app_dir> <exe>`.

**Acceptance Criteria:**
- Exit code: **0**
- Output: `SUCCESS status=0`.
- No `STATIC_*` or `DYNAMIC_*` tokens.

---

### File: `tests/integration/verbose_static_and_dynamic.rs`

---

#### Test 30: `verbose_mode_static_finding_suppresses_dynamic_in_summary`

**What:** When Phase B finds a static issue, the SUMMARY line should report `dynamic_missing=0` even if loader-snaps captured dynamic failures too.

**Setup:**
1. Use `HOST_STATIC_IMPORTS_MISSING_EXE` (has static missing dep).
2. Do NOT copy `lwtest_a.dll` (so Phase B will find it missing).
3. Args: `run --cwd <app_dir> -v <exe>`.

**Acceptance Criteria:**
- Exit code: **10**
- Output contains `STATIC_MISSING` with `dll="lwtest_a.dll"`.
- Output contains `SUMMARY`.
- `SUMMARY` contains `dynamic_missing=0`.
- No `DYNAMIC_MISSING` token in output (static takes precedence).

---

## Summary of Skipped Tests

| Test | Reason | What To Resolve |
|------|--------|-----------------|
| **Test 6** — PE32 DLL in x64 chain | Unknown if `is_probably_pe_file` checks machine type | Read `pe.rs` `is_probably_pe_file` impl; if it doesn't check, file a feature request |
| **Test 7** — Real access violation | `HOST_ECHO_ARGV_CWD_EXE` may not support triggering real AV | Check fixture source; may need new fixture. Fallback test provided. |
| **Test 10** — Non-zero exit after long runtime | Sequence host may not support setting exit codes | Check if `exit:N` arg prefix exists in fixture source |
| **Test 11** — Fast exit with many modules | Needs > 6 DLLs loaded + non-zero exit | Needs fixture enhancement |
| **Test 19** — `--timeout-ms 0` behavior | Ambiguous: no-timeout vs. immediate-timeout | Read `run_target` in `debug_run.rs` to confirm |
| **Test 22** — Transitive init failure | Unknown what DLL name `DLL_LWTEST_A_NESTED` loads | Check nested fixture source |
| **Test 27** — NOT_FOUND vs BAD_IMAGE precedence | Depends on actual Windows loader-snaps output | Capture real debug strings on target OS first |

---

## Registration Template

For each new test file, add to `tests/integration.rs`:

```rust
#[cfg(all(windows, feature = "harness-tests"))]
#[path = "integration/your_new_file.rs"]
mod your_new_file;
```

Keep entries in alphabetical order by module name, matching the existing convention.
