# loadwhat Architecture Overview

`loadwhat` is a **Windows x64 DLL loading diagnostics CLI** — it answers "what DLL broke process startup, and why?" using Win32 debug APIs directly (no DbgEng dependency).

---

## Project Structure

```
loadwhat/
├── src/
│   ├── main.rs          # Core orchestration & phase logic (~21K lines)
│   ├── cli.rs           # CLI argument parsing
│   ├── debug_run.rs     # Windows debug loop (Phase A)
│   ├── loader_snaps.rs  # PEB/IFEO loader-snaps config
│   ├── search.rs        # DLL search order resolution
│   ├── pe.rs            # PE import table parsing
│   ├── emit.rs          # Token-based output formatting
│   └── win.rs           # Win32/NT FFI declarations
├── tests/
│   ├── harness/         # Test infrastructure & PE fixture builders
│   └── integration/     # 21 integration test suites
└── xtask/               # Build/test orchestration
```

---

## Three-Phase Execution Model

Entry: `loadwhat run <target.exe>` → `run_command()` in `src/main.rs`

### Phase A — Runtime Observation

- Spawns the target with `CreateProcessW(..., DEBUG_ONLY_THIS_PROCESS)`
- Runs a `WaitForDebugEvent` / `ContinueDebugEvent` debug loop (30s timeout)
- Captures `LOAD_DLL_DEBUG_EVENT` → `LoadedModule` structs (dll_name, path, base)
- Captures `OUTPUT_DEBUG_STRING_EVENT` → loader-snaps strings (if enabled)
- Returns a `RunOutcome` with all events, loaded modules, exit kind, elapsed ms

**Loader-snaps setup** (done before Phase A, if enabled):
1. Try writing `FLG_SHOW_LDR_SNAPS` to offset `0xBC` of the child PEB via `NtQueryInformationProcess`
2. Fallback: set `GlobalFlag` DWORD in the IFEO registry key for the image
3. Restore original state on exit

### Phase B — Static Import Diagnosis

**Triggered when:** loader exception code (e.g. `0xC0000135` = DLL not found) **or** early-exit heuristic (exit≠0, elapsed < 1.5s, ≤6 modules loaded)

- Parses PE import tables starting from the target EXE
- **Deterministic BFS** over the full import graph using a fixed v1 search order:
  1. App directory
  2. `System32`
  3. 16-bit system dir
  4. Windows dir
  5. CWD (position depends on `SafeDllSearchMode`)
  6. `%PATH%` entries
- Records missing/bad-image DLLs with `depth`, `via` (importing module), and all tried `CandidateResult` paths
- Selects `first_issue` by lowest depth, tie-broken lexicographically
- Returns a `StaticReport`

### Phase C — Dynamic Missing Inference (Loader-Snaps)

**Triggered when:** Phase B found nothing AND loader-snaps were captured

- Scans the captured debug strings for failure patterns
- Classifies candidates by kind + score (0–100):
  - `UnableToLoadDll` (95–100) — terminal failure
  - `InitializeProcessFailure` (85–92) — process init failed
  - `LoadDllFailed` (80) — generic `LdrLoadDll` failure
  - `SearchPathFailure` (70) — path resolution failure
- Ranks by: kind → score → app-local path hint → non-framework hint → thread correlation → event index → dll name
- Discards candidates whose DLL was later loaded successfully
- Emits a single `DYNAMIC_MISSING` token for the top candidate

---

## Output Format

Line-oriented, greppable tokens — e.g.:

```
STATIC_MISSING module=foo.exe dll=bar.dll reason=NOT_FOUND via=baz.dll depth=1
FIRST_BREAK observed_exit_kind=Exception observed_code=0xC0000135 diagnosis=dll-not-found dll=bar.dll confidence=HIGH
SUMMARY first_break=true static_missing=1 static_bad_image=0 dynamic_missing=0 runtime_loaded=4
```

**Exit codes:** `0` = success, `10` = diagnosis found, `21` = internal error, `22` = unsupported platform

---

## Full Flow Summary

```
CLI parse → run_command()
  └─ Setup loader-snaps (PEB write → IFEO fallback)
  └─ Phase A: Debug loop → RunOutcome
  └─ Restore loader-snaps
  └─ Early failure? → Phase B: BFS PE import graph → StaticReport
  └─ Still nothing? → Phase C: Heuristic debug-string scan → DynamicCandidate
  └─ Emit tokens → return exit code
```

The three phases form a **progressive fallback**: Phase A observes reality, Phase B diagnoses statically with high confidence, Phase C infers heuristically from loader-snaps output when static analysis comes up empty.

---

## Key Data Structures

### `RunOutcome` (Phase A result)

```rust
pub struct RunOutcome {
    pub pid: u32,
    pub runtime_events: Vec<RuntimeEvent>,   // All observed events
    pub loaded_modules: Vec<LoadedModule>,   // Successfully loaded DLLs
    pub loader_snaps_peb: Option<PebEnableInfo>,
    pub end_kind: RunEndKind,                // ExitProcess/Exception/Timeout
    pub exit_code: Option<u32>,
    pub exception_code: Option<u32>,
    pub elapsed_ms: u128,
}
```

### `StaticReport` (Phase B result)

```rust
struct StaticReport {
    missing_count: usize,
    bad_image_count: usize,
    first_issue: Option<FirstIssue>,  // Lowest depth by (depth, via, dll)
    safedll: bool,
}

struct FirstIssue {
    module: String,        // Root module name
    via: String,           // Importing module name
    depth: u32,            // Transitive depth
    dll: String,           // Missing DLL name
    diagnosis: &'static str,
    kind: ResolutionKind,
    candidates: Vec<CandidateResult>,
}
```

### `DynamicCandidate` (Phase C ranking)

```rust
struct DynamicCandidate {
    event_idx: usize,
    tid: u32,
    dll: String,
    status: Option<u32>,
    reason: &'static str,           // NOT_FOUND, BAD_IMAGE, OTHER
    score: i32,                     // 0–100
    kind: DynamicCandidateKind,
    app_local_hint: bool,
    framework_or_os_hint: bool,
    later_loaded: bool,
    thread_correlated: bool,
}
```
