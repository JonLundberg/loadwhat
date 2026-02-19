# loadwhat v1 Spec (tight)

**Purpose:** a single-exe, x64 Rust CLI that answers "what broke first during process startup?" for DLL loading (static first), using **Win32 debug APIs directly** (no DbgEng). Output is greppable: one event per line, stable tokens, `key=value` pairs.

`run` supports an optional **loader snaps** mode (`--loader-snaps`) and captures `OUTPUT_DEBUG_STRING_EVENT` as structured output tokens.

**No knobs** for search order. `loadwhat` uses the modern Windows default DLL search behavior and prints the exact candidate paths it evaluated.

---

## 1) CLI

### Primary workflow
```text
loadwhat run <exe_path> [-- <args...>]
  [--cwd <dir>]
  [--env KEY=VALUE ...]
  [--timeout-ms <n>]
  [--loader-snaps]
  [--report <file>]
  [-v|--verbose] / [--quiet]
  [--strict]
```

### Helpers
```text
loadwhat imports <exe_or_dll>
  [--cwd <dir>]
  [--report <file>]
  [--verbose] / [--quiet]
  [--strict]

loadwhat com progid <ProgID>
loadwhat com clsid <{CLSID}>
  [--wow6432]
  [--test]
  [--report <file>]
  [--verbose] / [--quiet]
  [--strict]
```

**Removed:** attach, recursive, json, custom search modes/paths.

---

## 2) `run` behavior (authoritative)

`run` always does:

### Output level policy
- Default output mode is **failures-only**.
  - If startup succeeds, emit no token lines.
  - If startup fails early and static diagnosis identifies a missing/bad direct import, emit only:
    - `SEARCH_ORDER`
    - one `STATIC_MISSING` or `STATIC_BAD_IMAGE` (first-break import)
    - `SEARCH_PATH` lines for that same import, in evaluated order.
- `-v` / `--verbose` enables full event output.

### Phase A - observe real loader timeline
- `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`
- Debug loop: `WaitForDebugEvent` + `ContinueDebugEvent`
- Capture every `LOAD_DLL_DEBUG_EVENT` and `OUTPUT_DEBUG_STRING_EVENT`.
- In verbose mode, emit:
  - every `LOAD_DLL_DEBUG_EVENT` as `RUNTIME_LOADED` (in order)
  - every `OUTPUT_DEBUG_STRING_EVENT` as `DEBUG_STRING` (in order)
- Capture termination:
  - `EXIT_PROCESS_DEBUG_EVENT` -> `RUN_END exit_kind="EXIT_PROCESS"`
  - terminal `EXCEPTION_DEBUG_EVENT` -> `RUN_END exit_kind="EXCEPTION"`

### Phase B - if startup fails early, diagnose missing direct static imports
Trigger this only when the run is clearly "didn't really start", defined as either:
- `RUN_END exit_kind="EXCEPTION"` **and** exception code indicates loader/init failure (examples: `0xC0000135`, `0xC0000139`, `0xC000007B`, etc.), or
- process exits "too early" (very short runtime) **and** loaded module count is "minimal" (heuristic thresholds are internal, but output must say `confidence="MEDIUM"` when using heuristics).

Then:
- Parse the EXE's direct import table (no recursion).
- For each imported DLL:
  - If it was not observed loaded in Phase A, run Windows default search resolution (below) and emit:
    - `STATIC_FOUND` or `STATIC_MISSING` or `STATIC_BAD_IMAGE`
    - If missing/bad, print `SEARCH_PATH` lines in the exact order evaluated.
- Pick the first-break candidate:
  - If exit code strongly indicates loader failure, choose the first missing/bad import; if multiple, tie-break lexicographically by DLL name.
  - Always distinguish observed vs inferred with `confidence=`.

---

## 3) Loader Snaps mode (`run --loader-snaps`)

When `--loader-snaps` is present:
- Enable `FLG_SHOW_LDR_SNAPS` (`0x00000002`) using **AUTO** mode:
  1) Preferred (process-local, no registry): after `CreateProcessW` succeeds and before continuing the initial loader path, set `PEB->NtGlobalFlag |= 0x2` via `NtQueryInformationProcess(ProcessBasicInformation)` plus `ReadProcessMemory` / `WriteProcessMemory`.
  2) Fallback (gflags-style, persistent): set:
     `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\<ImageName>\GlobalFlag`
     preserving the original value (including "value absent"), then restore after run completion.
- Restoration is mandatory best effort on all terminal paths (normal exit, exception, timeout, internal error). If IFEO fallback was used, restore the original IFEO value.

Failure behavior:
- If enabling loader snaps fails (PEB enable failed and IFEO fallback failed), emit:
```text
NOTE topic="loader-snaps" detail="enable-failed" code=0x...
```
- Then terminate command with exit code `21`.
- If PEB enable failed but IFEO fallback succeeded, emit in verbose mode:
```text
NOTE topic="loader-snaps" detail="peb-enable-failed" code=0x...
```
- If restore fails, emit:
```text
NOTE topic="loader-snaps" detail="restore-failed" code=0x...
```
and continue normal result handling.

`OUTPUT_DEBUG_STRING_EVENT` handling requirements:
- Read event text from target process memory using event metadata.
- Honor Unicode/ANSI flag.
- Emit as `DEBUG_STRING`.
- Do not introduce a `SNAPS_*` token family in v1; loader snaps output is represented through `DEBUG_STRING`.
- If text cannot be read, emit `DEBUG_STRING` with `text="UNREADABLE"` and continue.

---

## 4) Windows default search order (single, fixed)

When resolving a DLL name that is not an absolute path, enumerate candidates in this order for printing and checking file existence:

1) **Application directory**: directory containing the target EXE  
2) **System directory**: `GetSystemDirectoryW()` (typically `...\System32`)  
3) **16-bit system directory**: `GetWindowsDirectoryW()` + `\System` (legacy; include if present)  
4) **Windows directory**: `GetWindowsDirectoryW()`  
5) **Current directory** (position depends on Safe DLL Search Mode; see below)  
6) **PATH directories**: each segment of the target environment's `%PATH%` in order

### Safe DLL Search Mode handling (required)
- Read `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\SafeDllSearchMode`
  - treat missing value as enabled (modern default).
- If enabled (`safedll=1`): CWD appears after Windows directory and before PATH (as listed above).
- If disabled (`safedll=0`): CWD moves up to just after application directory (before system directories).

This is the only search order `loadwhat` implements in v1.

### Not modeled in v1 (must be explicitly noted if suspected)
- `KnownDLLs`
- SxS manifests / loader redirection
- `SetDefaultDllDirectories` / `AddDllDirectory`
- packaged app search rules

If these could materially affect results (best-effort detection), emit a single:
```text
NOTE topic="search-order" detail="KnownDLLs/SxS/AddDllDirectory not modeled in v1"
```
and continue with the v1 search order above.

---

## 5) Output contract (line-oriented)

Line format: `TOKEN key=value key=value ...`

**Rules**
- `key=value` only; no freeform prose except `NOTE`.
- Always quote paths and strings.
- Emit `SEARCH_ORDER safedll=0|1` once per static diagnosis block.
- Deterministic ordering for static output: sort imported DLL names lexicographically.
- Preserve runtime event order for `RUNTIME_LOADED` and `DEBUG_STRING`.

### Required tokens
- Default mode (no `-v`):
  - success path: no output
  - early failure path: `SEARCH_ORDER`, one of `STATIC_MISSING|STATIC_BAD_IMAGE`, and `SEARCH_PATH` for the first-break import only.
- Verbose mode (`-v`/`--verbose`):
  - `RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`
  - `FIRST_BREAK` (only when early failure path taken)
  - `STATIC_START`, `STATIC_IMPORT`, `STATIC_FOUND`, `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `STATIC_END`
  - `SEARCH_ORDER`, `SEARCH_PATH`
  - `SUMMARY`
  - `NOTE` (rare, for not-modeled or loader-snaps setup/restore disclaimers)

### Canonical example
```text
RUN_START exe="C:\App\app.exe" cwd="C:\App" pid=1234
DEBUG_STRING pid=1234 tid=5678 source="OUTPUT_DEBUG_STRING_EVENT" text="LdrLoadDll, searching for foo.dll"
RUNTIME_LOADED pid=1234 dll="KERNEL32.dll" path="C:\Windows\System32\KERNEL32.dll" base=0x00007FFB12340000
RUNTIME_LOADED pid=1234 dll="VCRUNTIME140.dll" path="C:\Windows\System32\VCRUNTIME140.dll" base=0x...
RUN_END pid=1234 exit_kind="EXCEPTION" code=0xC0000135 note="STATUS_DLL_NOT_FOUND"

FIRST_BREAK observed_exit_kind="EXCEPTION" observed_code=0xC0000135 diagnosis="MISSING_STATIC_IMPORT" dll="foo.dll" confidence="HIGH"
STATIC_START module="C:\App\app.exe" scope="direct-imports"
STATIC_IMPORT module="app.exe" needs="foo.dll"
STATIC_MISSING module="app.exe" dll="foo.dll" reason="NOT_FOUND"
SEARCH_ORDER safedll=1
SEARCH_PATH dll="foo.dll" order=1 path="C:\App\foo.dll" result="MISS"
SEARCH_PATH dll="foo.dll" order=2 path="C:\Windows\System32\foo.dll" result="MISS"
SEARCH_PATH dll="foo.dll" order=3 path="C:\Windows\System\foo.dll" result="MISS"
SEARCH_PATH dll="foo.dll" order=4 path="C:\Windows\foo.dll" result="MISS"
SEARCH_PATH dll="foo.dll" order=5 path="C:\App\.\foo.dll" result="MISS"
SEARCH_PATH dll="foo.dll" order=6 path="C:\Tools\bin\foo.dll" result="MISS"
STATIC_END module="C:\App\app.exe"

SUMMARY first_break=true missing_static=1 runtime_loaded=2 com_issues=0
```

---

## 6) `imports` command (offline helper)

`imports` runs the same direct import scan against `<exe_or_dll>` and resolves each imported DLL using the same search order and SafeDllSearchMode rules, producing the same `STATIC_*` and `SEARCH_*` events.

---

## 7) `com` command (minimal)

`com` reports:
- `COM_PROGID progid=... clsid=...`
- `COM_SERVER clsid=... kind="InprocServer32|LocalServer32" path=... exists=true|false`
- Optional: `COM_TEST ... hresult=0x...`

No integration with `run` in v1.

---

## 8) Exit codes

- `0` = no issues detected (and process exited normally)
- `10` = static missing/bad image detected (from `run` early-fail diagnosis or `imports`)
- `12` = COM issue detected (`com`)
- `20` = usage error
- `21` = cannot debug/launch (includes loader-snaps setup failure when requested)
- `22` = unsupported architecture / WOW64 mismatch

`--strict`: warnings become nonzero (prefer `10` if static issues exist; else `21` if launch issues; else `0`).

---

## 9) Implementation constraints

- Single exe, x64-only
- Unicode APIs
- Errors include hex `GetLastError()` / `HRESULT`
- Never claim a specific missing DLL unless supported by the static import scan
