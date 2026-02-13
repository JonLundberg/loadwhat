# AGENTS.md — loadwhat

## What matters most
This repo builds **loadwhat**, a single-exe **Windows x64 Rust CLI** that diagnoses *process-startup DLL loading failures* using **Win32 Debug APIs directly** (no DbgEng).

## Source of truth
1) `docs/loadwhat_spec_v1.md` is **authoritative** for:
   - CLI surface (commands/options)
   - output contract (TOKEN key=value ...)
   - early-failure diagnosis rules
   - Windows default DLL search order modeling for v1

2) `docs/loadwhat_ai_agent_spec.md` is **secondary** and should only guide:
   - repo layout suggestions
   - Windows Dev Container scaffolding
   - general implementation hygiene
If there is any conflict, follow `loadwhat_spec_v1.md`.

## Non-negotiable constraints
- Windows-only, x64-only.
- One executable; no external runtime dependencies.
- Use Win32 Debug API directly:
  - CreateProcessW with DEBUG_ONLY_THIS_PROCESS
  - WaitForDebugEvent / ContinueDebugEvent loop
- Do NOT add features removed in v1 spec (attach/recursive/json/custom search modes).

## Output contract (must follow exactly)
- Line-oriented: `TOKEN key=value key=value ...`
- No freeform prose except `NOTE`.
- Always quote paths and strings.
- Deterministic ordering for static output (lexicographic import list).
- Required tokens and behavior are defined in `docs/loadwhat_spec_v1.md`.

## Implementation plan (high level)
Milestone 1 — scaffold + compile:
- Create Rust crate `loadwhat`.
- Implement CLI with subcommands: `run`, `imports`, `com progid`, `com clsid`.
- Ensure `cargo build -r` produces `target\release\loadwhat.exe`.

Milestone 2 — debug loop (Phase A):
- Implement debug loop for `run`:
  - Emit RUN_START, RUNTIME_LOADED, RUN_END
  - Best-effort module path retrieval (hFile -> GetFinalPathNameByHandleW; else ReadProcessMemory of lpImageName; else unknown)
  - Log EXCEPTION_DEBUG_EVENT but keep it minimal and continue unless it’s terminal.

Milestone 3 — static diagnosis (Phase B) and imports command:
- Parse PE import table (direct imports only).
- Implement v1 Windows search order enumeration with SafeDllSearchMode handling.
- Emit STATIC_* tokens + SEARCH_ORDER + SEARCH_PATH exactly as spec.
- Implement FIRST_BREAK selection logic and SUMMARY.

Milestone 4 — COM helpers:
- Registry lookups for ProgID/CLSID and InprocServer32/LocalServer32.
- Emit COM_* tokens per spec.

## Repo layout
- Put the two spec docs under `/docs/`.
- Keep code organized (cli.rs, debug_loop.rs, pe_imports.rs, search_order.rs, com.rs, report.rs).
- README must explain Windows container prerequisites and how to run.

## Testing expectations
- Include at least a basic smoke test script (PowerShell) that runs:
  - `loadwhat imports C:\Windows\System32\notepad.exe`
  - `loadwhat run C:\Windows\System32\notepad.exe`
and demonstrates stable output tokens.

## Safety / correctness posture
- Prefer “observed” over “inferred.” When inference is used, print `confidence=...` exactly as spec.
- Never claim a DLL is missing unless supported by the direct import scan + search result.
- If something is not modeled, emit a NOTE topic="search-order" line as described in spec.