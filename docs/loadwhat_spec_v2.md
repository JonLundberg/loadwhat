# loadwhat v2 Spec Draft

Status: draft for planned x86/WOW64 support.

This document is not the source of truth for current implemented behavior. Current behavior remains defined by [docs/loadwhat_spec_v1.md](./loadwhat_spec_v1.md).

Purpose: extend `loadwhat` from an x64-target-only diagnostic tool to a single Windows x64 executable that diagnoses DLL loading failures for both x64 and x86 targets.

COM is not part of v2. Future COM planning lives in [docs/COM_Plan.md](./COM_Plan.md).

Output remains line-oriented and greppable:

```text
TOKEN key=value key=value ...
```

## 1) Scope

V2 keeps the existing commands:

```text
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

V2 supports:

- one x64 `loadwhat.exe`
- x64 target processes
- x86 target processes under WOW64
- x64 and x86 roots for `imports`
- the v1 fixed DLL search model unless explicitly extended by a later spec

V2 does not support:

- ARM/ARM64 target diagnosis
- attach-to-process workflows
- JSON output
- custom search modes
- COM commands

## 2) PE architecture model

PE images are classified by COFF `Machine` and optional-header magic:

- `x64`: `IMAGE_FILE_MACHINE_AMD64` (`0x8664`) with PE32+ optional header (`0x020B`)
- `x86`: `IMAGE_FILE_MACHINE_I386` (`0x014C`) with PE32 optional header (`0x010B`)
- `other`: any other machine, unsupported machine, or mismatched machine/magic pair

Unsupported root images exit `22`.

Malformed or unreadable PE roots exit `21` unless a command-specific diagnosis is already defined.

## 3) Static diagnosis

`imports` and `run` Phase B use the root image machine type as the compatibility target.

Rules:

1. Parse direct imports for the root image.
2. Walk transitive imports with the same deterministic BFS rules from v1.
3. Resolve DLL basenames with the fixed v1 search order.
4. Treat a found dependency as compatible only when its architecture matches the root architecture.
5. Treat a found dependency with incompatible architecture as `STATIC_BAD_IMAGE`.
6. Treat malformed found dependency files as `STATIC_BAD_IMAGE`.
7. Preserve first-issue selection by lowest depth, then `via` lexicographic, then DLL name lexicographic.

No new public static token family is introduced. `STATIC_BAD_IMAGE` remains the public result for wrong-architecture dependencies.

## 4) `run` behavior

`run` keeps the v1 phase order:

1. runtime observation through Win32 debug APIs
2. direct and recursive static import diagnosis
3. loader-snaps dynamic inference when enabled

Runtime observation continues to use:

- `CreateProcessW(..., DEBUG_ONLY_THIS_PROCESS, ...)`
- `WaitForDebugEvent`
- `ContinueDebugEvent`

For x86 WOW64 targets, `LOAD_DLL_DEBUG_EVENT`, `OUTPUT_DEBUG_STRING_EVENT`, exit, timeout, and exception handling remain required.

Summary mode still emits exactly one line when a public diagnosis or success-like completion is reached:

- `STATIC_MISSING`
- `STATIC_BAD_IMAGE`
- `DYNAMIC_MISSING`
- `SUCCESS status=0`

## 5) Loader-snaps

Loader-snaps remains enabled by default for `run`.

For x64 targets:

- keep the existing v1 PEB `NtGlobalFlag` enable path
- use x64 `NtGlobalFlag` offset `0xBC` for Windows 10/11 family

For x86 WOW64 targets:

- detect WOW64 target process
- locate PEB32 using `NtQueryInformationProcess(ProcessWow64Information)`
- set `PEB32->NtGlobalFlag |= FLG_SHOW_LDR_SNAPS`
- use PEB32 `NtGlobalFlag` offset `0x68` for Windows 10/11 family

If PEB enable fails for either architecture, use the existing IFEO `GlobalFlag` fallback and best-effort restore behavior.

Trace/verbose `NOTE` output may distinguish PEB64 and PEB32 setup details, but must not introduce a new public token family.

## 6) Dynamic inference

Dynamic `LoadLibrary*` inference remains based on loader-snaps `OUTPUT_DEBUG_STRING_EVENT` text.

Candidate selection keeps the v1 rules:

- discard candidates later loaded successfully
- correlate unnamed failure lines by thread-local context
- prefer higher-confidence terminal failures
- prefer app-local/target-initiated failures over later framework or OS noise
- choose deterministically when tied

For x86 targets, reconstructed `SEARCH_ORDER` and `SEARCH_PATH` lines use the same fixed search model unless a later v2 revision defines architecture-specific differences.

## 7) Output contract

V2 preserves v1 token families:

- `STATIC_MISSING`
- `STATIC_BAD_IMAGE`
- `DYNAMIC_MISSING`
- `SUCCESS`
- `RUN_START`
- `RUNTIME_LOADED`
- `DEBUG_STRING`
- `RUN_END`
- `FIRST_BREAK`
- `STATIC_START`
- `STATIC_IMPORT`
- `STATIC_FOUND`
- `STATIC_END`
- `SEARCH_ORDER`
- `SEARCH_PATH`
- `SUMMARY`
- `NOTE`

V2 may add optional `machine="x64|x86|other"` fields only where the implementation needs trace or verbose diagnostics. Summary mode should not add machine fields unless a later draft explicitly requires them.

## 8) Exit codes

- `0` = no issue detected, including success-like timeout after runtime module-load progress
- `10` = missing or bad-image issue detected
- `20` = usage error
- `21` = cannot launch/debug target, malformed root image, or non-diagnostic failure without public diagnosis
- `22` = unsupported root architecture

## 9) Testing requirements

V2 must add fixture-backed integration coverage for:

- x86 `imports` success
- x86 `imports` direct and transitive missing
- x86 `imports` wrong-architecture dependency
- x86 `run` success
- x86 `run` static missing
- x86 `run` static bad image
- x86 `run` dynamic missing through loader-snaps
- x86 loader-snaps PEB32 setup and IFEO fallback notes
- mixed x64-root/x86-dependency and x86-root/x64-dependency chains

The primary validation workflow remains:

```text
cargo xtask test
```
