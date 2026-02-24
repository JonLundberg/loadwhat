# AGENTS.md - loadwhat (Codex agent rules)

This file defines mandatory behavioral rules for Codex when working in this repository.

## Primary objective

Build **loadwhat**, a single-executable Windows x64 Rust CLI that diagnoses process-startup DLL loading failures using Win32 Debug APIs directly (no DbgEng).

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

## Non-negotiable constraints

- Windows-only, x64-only.
- One executable; no external runtime dependencies.
- Use Win32 Debug APIs directly:
  - `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`
  - `WaitForDebugEvent` / `ContinueDebugEvent`
- Do not add removed features (attach/recursive/json/custom search modes).
- Output must be line-oriented tokens per spec (`TOKEN key=value ...`).

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
