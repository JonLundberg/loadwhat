# AGENTS.md — loadwhat (Codex agent rules)

This file defines **mandatory** behavioral rules for Codex when working in this repository.

## Primary objective
Build **loadwhat**, a single-executable **Windows x64 Rust CLI** that diagnoses *process-startup DLL loading failures* using the **Win32 Debug APIs directly** (no DbgEng).

---

## Authority hierarchy (STRICT ORDER)
If any instruction conflicts, follow the highest authority source:

1) `docs/loadwhat_spec_v1.md` (**ABSOLUTE AUTHORITY**)
2) `AGENTS.md` (this file)
3) `docs/loadwhat_ai_agent_spec.md`
4) `README.md`
5) Inline code comments

Codex **MUST NEVER** override or reinterpret `loadwhat_spec_v1.md`.
Implement **exactly** what the spec defines — no more, no less.

---

## Non-negotiable constraints
- **Windows-only**, **x64-only**.
- **One executable**; no external runtime dependencies.
- Must use Win32 Debug API directly:
  - `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`
  - `WaitForDebugEvent` / `ContinueDebugEvent` loop
- Do **NOT** add features explicitly removed in v1 spec (attach/recursive/json/custom search modes, etc.).
- Output is **line-oriented tokens** only (per spec). No freeform prose except spec-defined `NOTE`.

---

## Truthfulness / anti-hallucination requirements
Codex MUST NOT:
- Invent Win32 behavior
- Invent undocumented fields
- Guess undocumented API results
- Fabricate module paths
- Fabricate DLL load results
- Fabricate registry values

Codex MUST rely only on:
- Official Microsoft Win32 APIs
- Direct observation via the debug loop
- Direct PE parsing
- Direct registry inspection

If something cannot be determined:
- Emit `NOTE` tokens **exactly** as defined in the spec.
- When inference is used, include `confidence=...` exactly as defined in the spec.

---

## Output contract (MANDATORY)
- Format: `TOKEN key=value key=value ...` (per `loadwhat_spec_v1.md`)
- No extra commentary; no “pretty printing”; no JSON.
- Always quote paths/strings exactly as the spec requires.
- Required tokens, required keys, and required ordering are defined in `docs/loadwhat_spec_v1.md`.

---

## Determinism requirement (MANDATORY)
Codex MUST ensure deterministic output.

Codex MUST NOT rely on:
- HashMap iteration order
- Pointer values / memory addresses
- Timing differences
- Thread scheduling

Codex MUST:
- Sort all lists lexicographically where required
- Use `Vec` with explicit sorting for stable output
- Ensure identical output across repeated runs

Repeated execution MUST produce identical output for the same inputs.

---

## Error handling contract
Codex MUST NOT:
- `panic!`
- `unwrap()` in production paths
- Print Rust error dumps
- Print stack traces

Errors MUST be reported via **spec-defined TOKEN lines** only.
Internal errors MUST be converted into spec-compliant output.

---

## Implementation workflow (MANDATORY)
Codex MUST follow this workflow:

1) Read the relevant section of `docs/loadwhat_spec_v1.md`
2) Identify required tokens and behavior
3) Implement the **minimal compliant** code
4) Compile the project
5) Verify CLI executes
6) Verify output matches spec **exactly**
7) Only then proceed to the next feature

Codex MUST NOT implement multiple unrelated features simultaneously.

---

## Implementation plan (high level milestones)
**Milestone 1 — scaffold + compile**
- Create Rust crate `loadwhat`
- Implement CLI subcommands per spec (`run`, `imports`, `com progid`, `com clsid`)
- Ensure `cargo build --release` produces `target\release\loadwhat.exe`

**Milestone 2 — debug loop (Phase A)**
- Implement debug loop for `run`:
  - Emit required start/end tokens per spec
  - Handle `EXCEPTION_DEBUG_EVENT` minimally and continue unless terminal per spec
- Best-effort module path retrieval (when needed by spec), prefer this order:
  1) If you have a file handle: `GetFinalPathNameByHandleW`
  2) Else if available from debug info: read from process (`ReadProcessMemory`) per known structure the spec allows
  3) Else: report as unknown using spec-compliant tokens/NOTE

**Milestone 3 — static diagnosis + `imports`**
- Parse PE import table (direct imports only, unless spec says otherwise)
- Implement v1 Windows DLL search order enumeration as defined in spec
- Emit `STATIC_*`, `SEARCH_ORDER`, `SEARCH_PATH`, summary tokens exactly as spec
- Deterministic ordering (lexicographic import list, stable search path ordering)

**Milestone 4 — COM helpers**
- Registry lookups for ProgID/CLSID and servers (per spec)
- Must inspect only real registry values; never invent results
- Emit `COM_*` tokens per spec

---

## Repo layout expectations
- Specs live in `/docs/` (do not move them)
- Keep code organized into focused modules (example split):
  - `cli.rs`, `debug_loop.rs`, `pe_imports.rs`, `search_order.rs`, `com.rs`, `report.rs`
- README should explain prerequisites and how to run locally / in a Windows container (if present)

---

## Testing expectations
- Provide a basic smoke test script (PowerShell) that demonstrates stable output tokens, e.g.:
  - `loadwhat.exe imports C:\Windows\System32\notepad.exe`
  - `loadwhat.exe run C:\Windows\System32\notepad.exe`
- Tests must validate token shape and determinism (not brittle on irrelevant details).

---

## Completion checklist (MANDATORY)
Before declaring work complete, verify:

- `cargo build --release` succeeds
- `target\release\loadwhat.exe` exists
- `loadwhat.exe --help` executes
- `loadwhat.exe imports C:\Windows\System32\notepad.exe` executes
- `loadwhat.exe run C:\Windows\System32\notepad.exe` executes
- Output matches the spec exactly
- Output is deterministic across repeated runs
- No runtime dependencies exist beyond Windows
- Binary runs on a clean Windows system

---

## Final directive
When in doubt, **re-read `docs/loadwhat_spec_v1.md`** and implement only what it requires.