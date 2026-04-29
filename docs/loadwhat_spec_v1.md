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
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
```

- Run options must appear before `<TARGET>`.
- `<TARGET>` is the first positional argument after `run`.
- All arguments after `<TARGET>` are passed unchanged to the target process.
- Loader-snaps Phase C is enabled by default; use `--no-loader-snaps` to disable it.
- Options are applied left-to-right until `<TARGET>` is reached.
- Later flags win per dimension:
  - `--trace` vs `--summary`
  - `-v` / `--verbose` vs `--quiet`
  - `--loader-snaps` vs `--no-loader-snaps`
- `-v` / `--verbose` implies trace unless a later `--summary` switches back to summary mode.

### Helpers

```text
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

### Not in v1

Roadmap-only features are documented in `docs/roadmap.md` and are not part of this spec.

## 2) `run` behavior

### Output mode policy

- Default mode is summary mode.
- Summary mode emits exactly one token line for `run` when `loadwhat` reaches a public diagnosis or success-like completion:
  - `STATIC_MISSING ...`, `STATIC_BAD_IMAGE ...`, or `DYNAMIC_MISSING ...` when a first break is diagnosed
  - `SUCCESS status=0` when startup succeeds, or when a timeout occurs after runtime module-load progress, and no load issue is diagnosed
- A non-diagnostic failure such as a timeout before meaningful runtime progress may exit `21` with no public token output.
- Summary mode suppresses trace-style token lines (`SEARCH_ORDER`, `SEARCH_PATH`, `NOTE`, runtime timeline tokens).
- `--trace` enables detailed diagnostic trace output.
- `-v` or `--verbose` enables verbose runtime event output (`RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`) and extended static diagnosis output (`STATIC_*`, `SEARCH_*`, `FIRST_BREAK`, `SUMMARY`).
- If a later `--summary` appears after `-v` / `--verbose`, summary mode wins and trace output is suppressed for that invocation.
- Verbose `SUMMARY` fields use explicit diagnosis counters:
  - `run`: `SUMMARY first_break=true|false static_missing=N static_bad_image=N dynamic_missing=N runtime_loaded=N com_issues=0`
  - `imports`: `SUMMARY first_break=false static_missing=N static_bad_image=N dynamic_missing=0 runtime_loaded=0 com_issues=0`

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
- Uses the same fixed v1 search order and SafeDllSearchMode behavior (Â§4).
- Missing-focused: failures-only mode may stop as soon as the tool can report the first missing DLL.

Algorithm (deterministic BFS):

1. Start at root image (EXE for `run`, file for `imports`), `depth=0`.
2. Parse direct import table.
3. For each imported DLL name:
   - Ignore API sets: `api-ms-win-*`, `ext-ms-win-*`.
   - Resolve via Â§4 search order.
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

### Phase C: dynamic missing inference (loader-snaps; enabled by default)

When loader-snaps is enabled (the default for `run`; disable with `--no-loader-snaps`) and static diagnosis did not already report a missing/bad direct import, `loadwhat` may infer a dynamic `LoadLibrary*` failure from loader-snaps debug strings.

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

When `SEARCH_ORDER` / `SEARCH_PATH` are emitted for a dynamic failure, they are produced from the fixed v1 search model in Â§4. They are diagnostic reconstructions, not a claim that every Windows loader mode or `LoadLibraryEx` variant used that exact runtime search order.

#### Dynamic candidate selection rules

Phase C is heuristic and is based on loader-snaps debug strings captured through `OUTPUT_DEBUG_STRING_EVENT`; it does not observe `LoadLibrary*` return values directly.

v1 does not define a separate post-startup suppression boundary for Phase C. Dynamic load failures later in the observed run may still be selected if they remain the highest-ranked unresolved candidate under the rules below.

When multiple dynamic-failure candidates are present in one run, `loadwhat` selects a single `DYNAMIC_MISSING` result using these rules:

1. Discard any candidate for a DLL that is later observed to load successfully in the same run.
2. Use thread-local load context when a failure line omits DLL name; do not cross-correlate load attempts across threads.
3. Prefer higher-confidence terminal failure candidates (for example `Unable to load DLL`) over weaker contextual candidates.
4. Prefer app-local/target-initiated failures over later framework/UI/system noise when both are otherwise plausible.
5. For candidates still tied after the rules above, prefer the earliest remaining unresolved candidate.
6. Apply deterministic final tie-break rules if needed:
   - thread-correlated candidate over uncorrelated candidate
   - lexicographic DLL name as final tie-break

Purpose:

- report the most likely first unresolved handled dynamic load failure
- avoid replacing an earlier app-local failure with a later incidental framework load event
- emit at most one summary diagnosis, representing the highest-ranked unresolved dynamic failure candidate after Phase C filtering and selection

## 3) Loader Snaps mode (enabled by default for `run`)

When loader-snaps is enabled (the default for `run`; disable with `--no-loader-snaps`):

1. Preferred: enable `FLG_SHOW_LDR_SNAPS` (`0x00000002`) process-locally by setting `PEB->NtGlobalFlag |= 0x2`.
2. Fallback: set IFEO `GlobalFlag` under
   `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\<ImageName>\GlobalFlag`
   and restore original value afterward.

Restoration is best effort for all terminal paths.

### Determining `PEB->NtGlobalFlag` address (v1)

- Detect Windows `major.minor.build` using `RtlGetVersion` (not `GetVersionEx`).
- v1 is x64-only:
  - If the target is WOW64, treat as unsupported (exit code `22`) and emit a `NOTE` that WOW64 target support is roadmap-only.
  - `run --no-loader-snaps` is still x64-only and must reject x86/WOW64 targets before launch.
  - `imports` roots are also x64-only in v1.
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

- Summary mode omits loader-snaps setup and restore notes.
- Trace mode may emit terminal setup/restore diagnostics such as `enable-failed`, `restore-failed`, and `wow64-target-unsupported`.
- Verbose mode may additionally emit fallback-detail notes such as `peb-enable-failed`.

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

## 4) DLL search order (fixed model in v1)

Scope:
- v1 models a deterministic subset of Windows DLL resolution for classic unpackaged desktop processes.
- This model is used for:
  - static import diagnosis in `run`
  - the recursive missing-dependency walk in `run` and `imports`
  - `SEARCH_ORDER` / `SEARCH_PATH` reconstruction for `DYNAMIC_MISSING` when Phase C can build search context
- The v1 model applies to:
  - non-absolute DLL basenames
  - dependent DLL resolution for a module that was itself loaded by absolute path
- If a root module is loaded by absolute path, its dependent DLLs are still resolved by basename using this same fixed v1 order.

For non-absolute DLL basenames, evaluate candidates in this order:
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

Additional v1 notes:
- PATH evaluation uses the process PATH entries in order.
- The per-application App Paths registry key is not part of DLL search resolution in v1.
- If the computed 16-bit system directory path does not exist, skip it; do not fabricate a result for it.
- Recursive static scanning ignores import names matching `api-ms-win-*` and `ext-ms-win-*` rather than reporting them as missing.

Not modeled in v1:
- DLL redirection
- Loaded-module list reuse
- KnownDLLs
- SxS / manifest redirection
- packaged app search rules
- package dependency graph search
- `LoadLibraryEx(..., LOAD_WITH_ALTERED_SEARCH_PATH)`
- `SetDllDirectory`
- `SetDefaultDllDirectories`
- `AddDllDirectory`
- `LOAD_LIBRARY_SEARCH_*` flag-based search order
- relative-path `LoadLibrary*` semantics beyond the distinction between absolute-path inputs and basename inputs

When one of the unmodeled behaviors above is likely relevant, emit:
```text
NOTE detail="KnownDLLs/SxS/SetDllDirectory/AddDllDirectory/alternate loader search not modeled in v1"
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

`imports` runs direct import scanning for `<exe_or_dll>` and also performs the recursive missing-dependency walk described in Â§2, resolving imports with the same fixed search order and SafeDllSearchMode behavior and emitting static/search tokens. The `imports` command uses the same fixed v1 model from Â§4 and does not attempt to emulate alternate Windows loader search modes.

## 7) Exit codes

- `0` = no issues detected, including the current success-like timeout path after runtime module-load progress
- `10` = missing/bad image issue detected (`run` static/dynamic diagnosis or `imports`)
- `20` = usage error
- `21` = cannot launch/debug target, or non-diagnostic failure without a public diagnosis token (including loader-snaps setup failure and timeout before meaningful runtime progress)
- `22` = unsupported architecture, including x86/WOW64 roots in v1

## 8) Constraints

- Windows-only, x64-only
- single executable
- direct Win32 debug APIs
- no fabricated diagnostics (DLL names/paths/results must come from direct observation or deterministic scan/inference rules above)

## 9) Pre-v2 architecture hardening

V1 may use internal PE architecture detection to enforce the x64-only contract and prepare for planned v2 x86/WOW64 support.

Internal classification:

- x64 = `IMAGE_FILE_MACHINE_AMD64` (`0x8664`) with PE32+ optional header (`0x020B`)
- x86 = `IMAGE_FILE_MACHINE_I386` (`0x014C`) with PE32 optional header (`0x010B`)
- any mismatched machine/magic pair is not a compatible x64 image

Behavior:

- x86/WOW64 roots are rejected in v1 with exit `22`.
- When static search finds an x86 DLL in an x64 dependency chain, report it as `STATIC_BAD_IMAGE`, not `STATIC_FOUND`.
- No new public token family is introduced for this hardening.
