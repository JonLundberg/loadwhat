# loadwhat v1 Spec (authoritative)

Purpose: a single-exe, x64 Rust CLI that answers "what broke first during process startup?" for DLL loading (static first), using Win32 debug APIs directly (no DbgEng).

Output is line-oriented and greppable:

```text
TOKEN key=value key=value ...
```

This document is the source of truth for current implemented behavior.

## 1) CLI

### Primary workflow

```text
loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [--loader-snaps] [--trace|--summary] [-v|--verbose] [-- <args...>]
```

### Helpers

```text
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

### Not in v1

Roadmap-only features are documented in `docs/roadmap.md` and are not part of this spec.

## 2) `run` behavior

### Output mode policy

- Default mode is summary mode.
- Summary mode emits exactly one token line for `run`:
  - `STATIC_MISSING ...`, `STATIC_BAD_IMAGE ...`, or `DYNAMIC_MISSING ...` when a first break is diagnosed
  - `SUCCESS status=0` when startup succeeds and no load issue is diagnosed
- Summary mode suppresses trace-style token lines (`SEARCH_ORDER`, `SEARCH_PATH`, `NOTE`, runtime timeline tokens).
- `--trace` enables detailed diagnostic trace output.
- `-v` or `--verbose` implies `--trace` and additionally emits runtime event output (`RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`) and extended static diagnosis output (`STATIC_*`, `SEARCH_*`, `FIRST_BREAK`, `SUMMARY`).

### Phase A: runtime observation

- Launch with `CreateProcessW(..., DEBUG_ONLY_THIS_PROCESS, ...)`.
- Run debug loop with `WaitForDebugEvent` and `ContinueDebugEvent`.
- Capture `LOAD_DLL_DEBUG_EVENT` and `OUTPUT_DEBUG_STRING_EVENT`.
- Track termination via `EXIT_PROCESS_DEBUG_EVENT` and terminal `EXCEPTION_DEBUG_EVENT`.

### Phase B: direct static import diagnosis

Static diagnosis is attempted only when startup appears to have failed early, based on:

- loader-related exception code (for example `0xC0000135`, `0xC0000139`, `0xC000007B`), or
- early process exit heuristics.

Behavior:

- Parse direct imports of the target image. Additionally, perform an always-on recursive missing-dependency walk over the import graph of any found modules (see "Recursive missing-dependency walk (v1)" below).
- Compare imports with modules observed in Phase A.
- Resolve missing candidates with the fixed v1 search order.
- If static missing or bad image is diagnosed:
  - summary mode: emit exactly one line, `STATIC_MISSING` or `STATIC_BAD_IMAGE`.
  - trace mode: emit `SEARCH_ORDER`, one `STATIC_MISSING` or `STATIC_BAD_IMAGE`, and `SEARCH_PATH` for that DLL.
  - verbose trace mode: emit full `STATIC_*` and `SEARCH_*` events and `FIRST_BREAK`.

#### Recursive missing-dependency walk (v1)

Purpose:

- Detect missing transitive dependencies without relying on loader-snaps.

Rules:

- Always on during `run` Phase B and in `imports`.
- Uses the same fixed v1 search order and SafeDllSearchMode behavior (§4).
- Missing-focused: failures-only mode may stop as soon as the tool can report the first missing DLL.

Algorithm (deterministic BFS):

1. Start at root image (EXE for `run`, file for `imports`), `depth=0`.
2. Parse direct import table.
3. For each imported DLL name:
   - Ignore API sets: `api-ms-win-*`, `ext-ms-win-*`.
   - Resolve via §4 search order.
   - If not found: record missing `{ dll, depth=parent+1, via=parent module }`.
   - If found: enqueue resolved module for scanning at `depth=parent+1`.
4. Maintain `visited` set keyed by normalized absolute path.
5. Stop conditions:
   - failures-only: stop once first missing is known.
   - verbose: continue walking until queue is empty.

First-missing selection:

- Lowest `depth`, tie-break by:
  1. `via` lexicographic
  2. `dll` lexicographic

Output contract change:

- Keep existing token families.
- For transitive missing, allow optional fields on `STATIC_MISSING`:
  - `via="parent.dll"` and `depth=<n>`.

### Phase C: dynamic missing inference (`--loader-snaps`)

When `--loader-snaps` is enabled and static diagnosis did not already report a missing/bad direct import, `loadwhat` may infer a dynamic `LoadLibrary*` failure from loader-snaps debug strings.

When inference succeeds:

- summary mode: emit only `DYNAMIC_MISSING dll="name.dll" reason="NOT_FOUND|BAD_IMAGE|OTHER" [status=0x........]`
- trace mode:
  1. `SEARCH_ORDER safedll=...` (if search context is available)
  2. `DYNAMIC_MISSING dll="name.dll" reason="NOT_FOUND|BAD_IMAGE|OTHER" [status=0x........]`
  3. `SEARCH_PATH` lines for that DLL in evaluated order (if search context is available)

`DYNAMIC_MISSING` fields:

- `dll` (required): inferred DLL basename.
- `reason` (required): `NOT_FOUND`, `BAD_IMAGE`, or `OTHER`.
- `status` (optional): parsed status value when available.

If search context cannot be built, emit only `DYNAMIC_MISSING`.

## 3) Loader Snaps mode (`run --loader-snaps`)

When `--loader-snaps` is present:

1. Preferred: enable `FLG_SHOW_LDR_SNAPS` (`0x00000002`) process-locally by setting `PEB->NtGlobalFlag |= 0x2`.
2. Fallback: set IFEO `GlobalFlag` under
   `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\<ImageName>\GlobalFlag`
   and restore original value afterward.

Restoration is best effort for all terminal paths.

### Determining `PEB->NtGlobalFlag` address (v1)

- Detect Windows `major.minor.build` using `RtlGetVersion` (not `GetVersionEx`).
- v1 is x64-only:
  - If the target is WOW64, treat as unsupported (exit code `22`) and emit a `NOTE` that WOW64 target support is roadmap-only.
- For x64 target:
  - Read `PebBaseAddress` from `NtQueryInformationProcess(ProcessBasicInformation)`.
  - Select `NtGlobalFlagOffset` using detected OS version where possible.
  - Best-effort rule: if OS detection fails or is unknown, still attempt using the default x64 offset.
- Offset statement for x64 v1:
  - `NtGlobalFlagOffset = 0xBC` for Windows 10/11 family (`major >= 10`).
  - If OS version is unknown, still attempt `0xBC`.
- Attempt PEB write first; if it fails, fall back to IFEO as specified above.
- Recommended verbose note example:
  - `NOTE topic="loader-snaps" detail="peb-ntglobalflag" os="major.minor.build|unknown" ntglobalflag_offset=0xBC`

Failure behavior:

- If both PEB enable and IFEO fallback fail:

```text
NOTE topic="loader-snaps" detail="enable-failed" code=0x...
```

and exit code `21`.

- If PEB enable fails but IFEO fallback succeeds, emit in verbose mode:

```text
NOTE topic="loader-snaps" detail="peb-enable-failed" code=0x...
```

- If restore fails:

```text
NOTE topic="loader-snaps" detail="restore-failed" code=0x...
```

`OUTPUT_DEBUG_STRING_EVENT` requirements:

- Read event text from target memory using event metadata.
- Honor Unicode/ANSI flag.
- In verbose mode, emit `DEBUG_STRING`.
- If unreadable, emit `DEBUG_STRING ... text="UNREADABLE"` and continue.
- Do not introduce a `SNAPS_*` token family.

## 4) DLL search order (fixed in v1)

For non-absolute DLL names, evaluate candidates in this order:

1. Application directory (target EXE directory)
2. System directory (`GetSystemDirectoryW`)
3. 16-bit system directory (`GetWindowsDirectoryW` + `\System`, if present)
4. Windows directory (`GetWindowsDirectoryW`)
5. Current directory (position depends on Safe DLL Search Mode)
6. PATH directories (in order)

Safe DLL Search Mode:

- Read `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\SafeDllSearchMode`.
- Missing value is treated as enabled.
- `safedll=1`: CWD after Windows directory.
- `safedll=0`: CWD after application directory.

Not modeled in v1:

- KnownDLLs
- SxS/loader redirection
- `SetDefaultDllDirectories` / `AddDllDirectory`
- packaged app search rules

When relevant, emit:

```text
NOTE detail="KnownDLLs/SxS/AddDllDirectory not modeled in v1"
```

## 5) Output contract

Line format:

```text
TOKEN key=value key=value ...
```

Rules:

- Keys use `key=value` format only.
- Paths and strings are quoted.
- Preserve runtime event order for `RUNTIME_LOADED` and `DEBUG_STRING`.
- `SEARCH_PATH` order matches evaluated candidate order.
- Static import iteration order is deterministic (lexicographic).

Required token families in v1:

- Summary/default: `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `DYNAMIC_MISSING`, `SUCCESS`
- Runtime/verbose: `RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`
- Static diagnosis: `FIRST_BREAK`, `STATIC_START`, `STATIC_IMPORT`, `STATIC_FOUND`, `STATIC_MISSING`, `STATIC_BAD_IMAGE`, `STATIC_END`
- Search: `SEARCH_ORDER`, `SEARCH_PATH`
- Dynamic loader-snaps inference: `DYNAMIC_MISSING`
- Meta: `SUMMARY`, `NOTE`

## 6) `imports` behavior

`imports` runs direct import scanning for `<exe_or_dll>` and also performs the recursive missing-dependency walk described in §2, resolving imports with the same fixed search order and SafeDllSearchMode behavior and emitting static/search tokens.

## 7) Exit codes

- `0` = no issues detected
- `10` = missing/bad image issue detected (`run` static/dynamic diagnosis or `imports`)
- `20` = usage error
- `21` = cannot launch/debug target (including loader-snaps setup failure)
- `22` = unsupported architecture

## 8) Constraints

- Windows-only, x64-only
- single executable
- direct Win32 debug APIs
- no fabricated diagnostics (DLL names/paths/results must come from direct observation or deterministic scan/inference rules above)
