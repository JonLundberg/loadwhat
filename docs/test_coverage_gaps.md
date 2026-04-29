# Test Coverage Gap Analysis

**Date:** 2026-03-21
**Current test count:** 271 total across default and harness-backed runs at last cleanup review (169 unit/bin tests plus 102 harness-backed integration tests)

---

## 1. Missing Real-World Integration Scenarios

### 1.1 Malformed / Corrupted PE Files

**Gap:** The harness can write "junk bytes" to simulate a bad image, but no test feeds a *structurally malformed* PE through the full run or transitive-import pipeline.

| Scenario | Why it matters |
|----------|----------------|
| Truncated PE header (valid MZ, PE offset past EOF) | Loader may crash or hang; Phase B must not panic |
| Valid PE header but corrupted import table (RVA points outside sections) | `pe::direct_imports` returns an error â€” does the recursive BFS skip gracefully or abort? |
| DLL with valid PE header but zero-length sections | Edge case in `rva_to_offset` â€” could produce out-of-bounds reads |
| 32-bit (PE32) DLL in an x64 import chain | Covered by `architecture_cleanup::imports_x64_chain_reports_x86_dependency_as_bad_image` |
| PE with import table containing self-referencing imports (circular) | Visited-set should prevent infinite recursion â€” untested in integration |
| Corrupted DLL encountered *transitively* (A â†’ B â†’ corrupt C) | Only direct bad-image is tested via `run`; transitive bad-image is tested for `imports` but not `run` with full runtime observation |

**Unit tests in `pe.rs` cover parse rejection** (truncated headers, bad signatures, etc.), but **no integration test feeds a malformed file through `loadwhat run`** to verify the full error-propagation path from PE parse failure â†’ Phase B skip â†’ Phase C fallback â†’ exit code.

### 1.2 DLL Init Failures (DllMain returns FALSE)

| Scenario | Status |
|----------|--------|
| Direct dependency DllMain returns FALSE | `dynamic_other_includes_status_for_init_failure` covers this |
| **Transitive** dependency DllMain returns FALSE (A loads B, B's DllMain fails) | **Not tested** |
| DllMain hangs (exceeds loader lock timeout) | **Not tested** â€” different from loadwhat's own timeout |
| DllMain raises an SEH exception | **Not tested** |

### 1.3 LoadLibrary Patterns

| Scenario | Status |
|----------|--------|
| `LoadLibraryExW` with `LOAD_LIBRARY_AS_DATAFILE` flag | **Not tested** â€” should not trigger DllMain or import walk |
| `LoadLibrary` with absolute path where the DLL exists but a transitive dep is missing | Partially tested in `dynamic_loadlibrary_fullpath` |
| `LoadLibrary` called from a DllMain (nested during init) | **Not tested** â€” loader lock reentrancy edge case |
| Delay-loaded imports that fail at runtime | **Not tested** â€” delay-load failures look different from static import failures |
| `LoadLibrary` returning NULL but process continuing (non-fatal) | Tested (optional probe), but **not tested for multiple non-fatal failures in sequence** |

### 1.4 Process Lifecycle Edge Cases

| Scenario | Status |
|----------|--------|
| Target exits with code 0 but loaded zero non-system DLLs | **Not tested** â€” should emit SUCCESS, not false diagnosis |
| Target crashes (access violation) *after* successful init | **Not tested** â€” Phase B should NOT trigger (not a loader failure) |
| Target spawns child process then exits | **Not tested** â€” `DEBUG_ONLY_THIS_PROCESS` should ignore child |
| Target calls `ExitProcess(0)` from DllMain | **Not tested** â€” early exit with code 0 + few modules |
| Target calls `TerminateProcess` on itself | **Not tested** â€” different exit path than `ExitProcess` |
| Target produces thousands of `OutputDebugString` calls | **Not tested** â€” performance / buffer limits |

### 1.5 Search Order Edge Cases

| Scenario | Status |
|----------|--------|
| DLL found via PATH but it's a bad image; valid copy exists later in PATH | **Not tested** â€” first bad image should win per search model |
| `app_dir == cwd` (same directory) | Unit test covers dedup in `search.rs`, but **no integration test** |
| UNC paths in app directory (`\\server\share\app.exe`) | **Not tested** |
| PATH contains relative entries (`.`, `..\..\lib`) | **Not tested** |
| PATH contains empty segments (`C:\a;;C:\b`) | **Not tested** |
| Very long PATH (> 32KB) | **Not tested** |
| Symlinked DLL in search path | **Not tested** |

---

## 2. Untested Error-Handling Code Paths

### 2.1 Phase A (debug_run.rs)

| Code path | Description |
|-----------|-------------|
| `CreateProcessW` failure | Only "file not found" tested; permission denied, path-too-long, etc. untested |
| `ReadProcessMemory` failure for debug strings | Tested (unreadable fallback), but not for *module names* |
| Debug events arriving in unusual orders | e.g., LOAD_DLL before CREATE_PROCESS completes |
| `WaitForDebugEvent` returning unexpected event types | `RIP_EVENT` handling untested |
| Timeout of exactly 0ms | Boundary condition |

### 2.2 Phase B (main.rs â€” static diagnosis)

| Code path | Description |
|-----------|-------------|
| `pe::direct_imports` returns `Err` mid-walk | Should skip that node and continue BFS â€” untested |
| Module path has no parent directory | `app_dir` fallback â€” untested |
| `SearchContext::from_environment` fails | Should degrade gracefully â€” untested in `run` (tested for dynamic path) |
| Import graph deeper than ~10 levels | Unbounded depth â€” no stress test |

### 2.3 Phase C (dynamic inference)

| Code path | Description |
|-----------|-------------|
| All candidates are later-loaded (all discarded) | Should emit no DYNAMIC_MISSING â€” **untested** |
| Multiple candidates with identical scores | Tie-breaking by event_idx â†’ dll â†’ tid â€” **untested at integration level** |
| Debug strings with non-UTF-8 / non-ASCII content | Encoding edge case â€” **untested** |
| Loader-snaps lines that match multiple classifier patterns | **untested** â€” which pattern wins? |

### 2.4 Loader Snaps (loader_snaps.rs)

| Code path | Description |
|-----------|-------------|
| PEB already has `FLG_SHOW_LDR_SNAPS` set | Should be a no-op or OR the flag â€” **untested** |
| IFEO registry key exists with non-DWORD type | Error handling â€” **untested** |
| `RegSetValueExW` succeeds but `RegDeleteValueW` on restore fails | Partial coverage (restore-failed note tested), but underlying cause untested |
| Permission denied on IFEO registry write | **Untested** (tests override via env vars) |

---

## 3. Untested CLI / UX Paths

| Scenario | Description |
|----------|-------------|
| `--timeout-ms 0` | Should it be rejected or treated as "no wait"? |
| `--timeout-ms 4294967296` (u32 overflow) | Parse error â€” **untested** |
| `--cwd` pointing to a non-existent directory | Error at parse time or at `CreateProcessW` time? |
| Target path without `.exe` extension | PATH-based resolution with extension appending â€” untested in integration |
| Target is a `.bat` or `.cmd` file | Should fail or run via cmd.exe? |
| `loadwhat imports` on a DLL (not an EXE) | Works by design but **not tested** |
| `loadwhat imports` on a directory path | Error handling â€” **untested** |
| Relative target path (`loadwhat run .\app.exe`) | Path normalization â€” **untested** |
| Target path with spaces and special characters | `unicode_and_spaced_paths` test exists but only for runtime output, not for CLI parsing |

---

## 4. Output Contract Gaps

| Scenario | Description |
|----------|-------------|
| SUMMARY token field ordering stability | No test asserts exact field order |
| Very long DLL paths in token output | Quote-escaping with deeply nested paths â€” **untested** |
| Multiple STATIC_MISSING at same depth | First-issue selection tested, but **not whether all are emitted in trace mode** |
| `imports` command on a DLL with hundreds of imports | Stability/performance â€” **untested** |
| Verbose mode with both static AND dynamic findings | Static takes precedence, but is DYNAMIC_MISSING fully suppressed in verbose summary? |

---

## 5. Priority Recommendations

### High Priority (real-world scenarios most likely to hit users)

1. **Malformed PE transitively** â€” `run` a fixture where `A.exe â†’ good.dll â†’ corrupt.dll`. Verify Phase B reports the bad image with correct depth/via and doesn't panic.
2. **Corrupted PE as root target** â€” `loadwhat run corrupt.exe` should exit 21 cleanly, not crash.
3. **32-bit DLL in x64 chain** â€” covered by the architecture cleanup tests. Keep coverage as v2 expands x86 support.
4. **Post-init crash (non-loader exception)** â€” `run` a target that loads everything fine then segfaults. Verify no false STATIC_MISSING / DYNAMIC_MISSING.
5. **Delay-load failure** â€” a target that uses `/DELAYLOAD` and the delayed DLL is missing. Currently invisible to Phase B's static walk.
6. **`loadwhat imports` on a DLL** â€” users will do this. Verify it works.

### Medium Priority (edge cases that protect correctness)

7. **PE parse failure mid-BFS** â€” inject one unparseable DLL in a chain of 3. Verify BFS continues past it.
8. **All dynamic candidates later-loaded** â€” verify no false DYNAMIC_MISSING.
9. **DllMain SEH exception in transitive dep** â€” verify Phase C picks it up.
10. **PATH with empty segments and relative entries** â€” verify search order doesn't break.
11. **`app_dir == cwd` integration test** â€” verify no duplicate search paths in output.
12. **Timeout 0ms** â€” verify defined behavior.

### Low Priority (hardening)

13. Non-ASCII / Unicode DLL names through full pipeline.
14. Very deep import graph (10+ levels).
15. Thousands of `OutputDebugString` calls.
16. Very long PATH environment variable.
17. UNC paths in target / search directories.
