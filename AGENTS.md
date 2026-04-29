# AGENTS.md - loadwhat (Codex agent rules)

This file defines mandatory behavioral rules for Codex when working in this repository.

## Primary objective

Current v1 objective: build **loadwhat**, a single-executable Windows x64 Rust CLI that diagnoses process-startup DLL loading failures using Win32 Debug APIs directly (no DbgEng).

Future v2 objective: add x86/WOW64 target support with feature parity for the existing DLL-loading mission. Do not implement v2 behavior until the v2 spec exists and authorizes it.

## Authority hierarchy (strict order)

If instructions conflict, follow this order:

1. `docs/loadwhat_spec_v1.md` (absolute authority)
2. `AGENTS.md` (this file)
3. `docs/loadwhat_ai_agent_spec.md`
4. `README.md`
5. Inline code comments

Never override or reinterpret the authoritative v1 spec.

## Roadmap

Planned and out-of-scope features live in `docs/roadmap.md`.
Do not implement roadmap items unless explicitly requested.

## Version planning

- v1 is the current active contract and remains Windows-only, x64-only.
- `docs/loadwhat_spec_v1.md` remains the absolute authority for current behavior.
- Agents must not implement x86 target support under v1 unless the v1 spec is updated first.
- v2 is planned to add x86/WOW64 support with parity for:
  - `run`
  - `imports`
  - recursive static diagnosis
  - loader-snaps dynamic inference
  - output modes
  - deterministic token behavior
- COM is no longer planned as v2. Preserve COM planning as future v3-oriented work.
- If present, move the existing COM draft from `docs/loadwhat_spec_v2.md` to `docs/COM_Plan.md` before creating the new x86/WOW64 v2 spec.

Allowed pre-v2 hardening work:

- update CI to run `cargo xtask test`
- add PE architecture detection as internal plumbing
- classify wrong-architecture DLLs in x64 dependency chains as bad image
- reject x86 targets consistently until v2 support is specified and implemented

V2-only work:

- allowing x86 targets
- WOW64 runtime/debug support
- PEB32 loader-snaps support
- x86 fixture expansion and full parity tests

## Non-negotiable constraints

- Windows-only.
- Current v1 is x64-only for supported targets.
- Until v2 is specified and implemented, x86/WOW64 targets must be rejected consistently per v1 behavior.
- Internal PE architecture detection is allowed in v1 when used to enforce x64-only behavior or diagnose wrong-architecture dependencies.
- One executable; no external runtime dependencies.
- Use Win32 Debug APIs directly:
  - `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`
  - `WaitForDebugEvent` / `ContinueDebugEvent`
- Do not add removed features (attach/json/custom search modes).
- Recursive missing-dependency walk is part of v1 spec; implement deterministically per spec.
- Output must be line-oriented tokens per spec (`TOKEN key=value ...`).

## Output contract discipline

- Preserve the documented public token families and output modes.
- `run` summary mode emits exactly one line:
  - `STATIC_MISSING`
  - `STATIC_BAD_IMAGE`
  - `DYNAMIC_MISSING`
  - or `SUCCESS status=0`
- `--trace` emits search/diagnosis trace tokens.
- `--verbose` adds runtime timeline tokens such as:
  - `RUN_START`
  - `RUNTIME_LOADED`
  - `DEBUG_STRING`
  - `RUN_END`
- Do not replace `RUNTIME_LOADED` with a new `LOAD` token.
- Do not introduce new public token families unless `docs/loadwhat_spec_v1.md` is updated first.

## Truthfulness requirements

Do not:

- invent Win32 behavior or undocumented fields
- fabricate DLL names, paths, or load results
- fabricate registry values

Use only:

- documented Win32 APIs
- direct debug-loop observation
- direct PE parsing
- direct registry inspection when required

When something cannot be determined, emit spec-defined `NOTE` tokens.

## Determinism requirements

Must be deterministic across repeated runs for same inputs.

Do not rely on:

- map iteration order
- pointer values
- timing races
- thread scheduling

Use explicit sorting where required by spec.

## File hygiene requirements

- Preserve existing file line-ending style; do not introduce mixed line endings.
- For markdown and docs in this repository, keep LF line endings to avoid noisy diffs.

## Error handling contract

Do not `panic!`/`unwrap()` in production paths.
Report errors via spec-defined token output (or documented exit code behavior), not stack dumps.

## Implementation workflow

1. Read relevant section of `docs/loadwhat_spec_v1.md`.
2. Identify required tokens and behavior.
3. Implement minimal compliant code.
4. Build.
5. Run/verify CLI behavior.
6. Verify output contract and determinism.

Do not mix unrelated feature work in one change.

## Implementation milestones (current repo scope)

### Milestone 1 - scaffold + compile

- crate builds on Windows x64
- CLI supports `run` and `imports` per v1 spec
- release build produces `target\release\loadwhat.exe`

### Milestone 2 - debug loop + loader-snaps capture

- runtime debug loop for `run`
- capture `LOAD_DLL_DEBUG_EVENT` and `OUTPUT_DEBUG_STRING_EVENT`
- handle process termination/exception paths per spec

### Milestone 3 - static diagnosis + imports

- direct import parsing
- recursive missing-dependency walk
- fixed v1 search order resolution
- emit `STATIC_*`, `SEARCH_ORDER`, `SEARCH_PATH`, `SUMMARY` deterministically

### Milestone 4 - dynamic inference + test harness

- loader-snaps dynamic missing inference (`DYNAMIC_MISSING`) per spec
- fixture-driven integration coverage
- deterministic test harness workflow (`cargo xtask test`)

## Repo layout expectations

- Specs/docs stay under `/docs/`.
- Keep code in focused modules, for example:
  - `cli.rs`, `debug_run.rs`, `pe.rs`, `search.rs`, `loader_snaps.rs`, `win.rs`, `emit.rs`

## Testing expectations

- Primary workflow: `cargo xtask test` (Windows).
- Ensure tests validate token shape/contract and deterministic behavior.
- Keep fixture-based tests isolated and reproducible.
- Preserve harness behavior under `target/loadwhat-tests/`.
- Treat `LWTEST:*` lines as internal harness output, not part of the public token contract.

## Change discipline

When changing behavior:

1. update code
2. update tests
3. update spec/examples if the public contract changed

Public behavior changes must update the active spec before or with the implementation. Do not rely on roadmap or planning docs to redefine current behavior.

Do not silently change code behavior while leaving the docs or test contract behind.

## Completion checklist

Before declaring work complete, verify:

- `cargo build --release` succeeds
- `target\release\loadwhat.exe` exists
- `loadwhat.exe --help` executes
- `loadwhat.exe imports C:\Windows\System32\notepad.exe` executes
- `loadwhat.exe run C:\Windows\System32\notepad.exe` executes
- output contract matches `docs/loadwhat_spec_v1.md`
- behavior is deterministic across repeated runs

## Final directive

When in doubt, re-read `docs/loadwhat_spec_v1.md` and implement only what it defines.
