# Codex Handoff Prompt (repo-aligned)

Use this prompt when handing the current repo to Codex.

---

You are updating the Windows Rust CLI tool `loadwhat`.

Read these files first, in order:

1. `docs/loadwhat_spec_v1.md`
2. `AGENTS.md`
3. `docs/loadwhat_ai_agent_spec.md`
4. `README.md`
5. `docs/testing.md`
6. `CONTRIBUTING.md`

Then inspect the current source tree, especially:

- `src/cli.rs`
- `src/debug_run.rs`
- `src/emit.rs`
- `src/loader_snaps.rs`
- `src/pe.rs`
- `src/search.rs`
- `src/win.rs`
- `tests/integration.rs`
- `tests/harness/*`
- `tests/integration/*`
- `xtask/*`

Requirements:

- language: Rust
- platform: Windows x64
- use Win32 APIs directly
- preserve deterministic output
- preserve current public token names and output modes
- preserve `cargo xtask test` as the primary integration workflow

Important current contract details:

- default `run` summary mode emits exactly one line:
  - `STATIC_MISSING`
  - `STATIC_BAD_IMAGE`
  - `DYNAMIC_MISSING`
  - or `SUCCESS status=0`
- `--trace` emits search/diagnosis trace tokens
- `--verbose` adds runtime timeline tokens:
  - `RUN_START`
  - `RUNTIME_LOADED`
  - `DEBUG_STRING`
  - `RUN_END`

Do not:

- invent new token families casually
- replace `RUNTIME_LOADED` with `LOAD`
- change exit codes without updating docs and tests
- add roadmap-only features unless explicitly requested
- perform unrelated refactors

Recommended workflow:

1. summarize the current relevant code paths
2. list the exact files you will change
3. implement the smallest coherent change
4. run the documented test workflow
5. verify token/output behavior against the spec
