# loadwhat v2 x86/WOW64 Plan

Status: planning document. The v2 behavior contract lives in `docs/loadwhat_spec_v2.md`.

Purpose: add x86/WOW64 target support with feature parity for the existing DLL-loading mission while preserving the current v1 token discipline and deterministic behavior.

## Goals

- Keep one Windows x64 `loadwhat.exe`.
- Support both x64 and x86 target images for `run`.
- Support both x64 and x86 root images for `imports`.
- Preserve the current `run` and `imports` command shapes.
- Preserve summary-mode one-line behavior and existing public token families unless the v2 spec explicitly extends them.

## Phase 0 - v1 cleanup already completed

- CI runs the fixture-backed workflow through `cargo xtask test`.
- PE parsing exposes internal architecture detection.
- v1 rejects x86 roots consistently until v2 enables them.
- v1 reports wrong-architecture DLLs in x64 dependency chains as `STATIC_BAD_IMAGE`.

## Phase 1 - shared architecture model

- Keep `pe::ImageArchitecture` as the single source for PE machine classification.
- Treat `IMAGE_FILE_MACHINE_AMD64` + PE32+ as x64.
- Treat `IMAGE_FILE_MACHINE_I386` + PE32 as x86.
- Treat mismatched machine/magic pairs and other machines as invalid or unsupported.
- Add shared compatibility helpers so static diagnosis can compare root and dependency architecture without duplicating policy in `search.rs` and `main.rs`.

## Phase 2 - static diagnosis parity

- Allow `imports` on x86 roots.
- Resolve x86 imports with the same fixed v1 search order.
- Classify x86 dependency images as compatible for x86 roots.
- Classify x64 dependency images as `STATIC_BAD_IMAGE` for x86 roots.
- Keep x64 behavior unchanged: x64 roots require x64 dependencies.
- Preserve deterministic first-issue selection by depth, `via`, then DLL name.

## Phase 3 - runtime observation parity

- Allow `run` on x86 targets under WOW64.
- Continue using `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`, `WaitForDebugEvent`, and `ContinueDebugEvent`.
- Continue capturing `LOAD_DLL_DEBUG_EVENT` and `OUTPUT_DEBUG_STRING_EVENT`.
- Use target machine type when interpreting static and dynamic results.
- Preserve x64 `run` behavior and output shape.

## Phase 4 - x86 loader-snaps

- Keep the existing x64 PEB `NtGlobalFlag` path for x64 targets.
- For x86 WOW64 targets, locate PEB32 through `NtQueryInformationProcess(ProcessWow64Information)`.
- Write `PEB32->NtGlobalFlag` at offset `0x68` for Windows 10/11 family unless the v2 spec later narrows this rule.
- If PEB32 enable fails, use the existing IFEO `GlobalFlag` fallback and best-effort restore behavior.
- Trace/verbose notes should distinguish PEB32 setup from PEB64 setup.

## Phase 5 - tests and fixtures

- Extend the PE test builder and MSVC fixture build to produce x86 fixtures.
- Add x86 `imports` tests for success, missing, bad image, and transitive missing.
- Add x86 `run` tests for success, static missing, static bad image, dynamic missing, and loader-snaps note behavior.
- Add mixed-architecture tests in both directions:
  - x64 root with x86 dependency
  - x86 root with x64 dependency
- Keep CI on `cargo xtask test`.

## Out of scope for v2

- COM commands and COM audit behavior. COM planning lives in `docs/COM_Plan.md`.
- JSON output.
- attach-to-process workflows.
- custom search modes.
