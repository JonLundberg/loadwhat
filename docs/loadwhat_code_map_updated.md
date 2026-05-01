# loadwhat — Code Map

## Purpose

`loadwhat` is a Windows-focused Rust CLI for diagnosing DLL loading failures. It runs or inspects a target executable/module, observes runtime loader behavior through Win32 debug events, parses PE import tables directly, reconstructs DLL search candidates, and emits deterministic line-oriented diagnostic tokens.

The current crate is primarily a binary crate. The high-level orchestration lives in `src/main.rs`; helper modules are declared from there under `#[cfg(windows)]`.

---

## Authority and orientation

Primary project references:

1. `docs/loadwhat_spec_v1.md` — behavior and public token contract
2. `AGENTS.md` — repository/agent rules
3. `docs/loadwhat_ai_agent_spec.md`
4. `README.md`

When this map conflicts with source or the v1 spec, treat the source/spec as authoritative and update this map.

---

## Top-level crate shape

```text
loadwhat/
├── Cargo.toml              # package metadata; workspace includes xtask
├── Cargo.lock
├── AGENTS.md
├── README.md
├── docs/
│   ├── loadwhat_spec_v1.md
│   ├── loadwhat_ai_agent_spec.md
│   └── roadmap.md
├── src/
│   ├── main.rs             # entry point and high-level run/imports orchestration
│   ├── cli.rs              # hand-written command-line parser
│   ├── debug_run.rs        # Win32 debug loop and runtime event collection
│   ├── emit.rs             # public token formatting helpers/constants
│   ├── loader_snaps.rs     # enables/restores loader snaps through PEB/IFEO paths
│   ├── pe.rs               # raw PE parsing and direct import extraction
│   ├── search.rs           # DLL search root construction and candidate classification
│   ├── test_util.rs        # unit-test environment variable guard
│   └── win.rs              # Win32 FFI types, constants, and helper functions
├── tests/
│   ├── integration.rs      # integration module registry, feature-gated by harness-tests
│   ├── harness/            # test harness support
│   └── integration/        # integration test modules
└── xtask/                  # test/build automation helper
```

There is no current `src/cmd.rs` and no current `src/lib.rs` in the main source layout. Command dispatch and most diagnosis orchestration are in `src/main.rs`.

---

## Entry point: `src/main.rs`

### Non-Windows behavior

On non-Windows platforms, `main()` prints:

```text
loadwhat currently supports Windows only.
```

and exits with code `22`.

### Windows behavior

On Windows, `main.rs` declares the internal modules and owns the top-level command dispatch.

High-level flow:

```text
main()
  ├── reject non-x64 target_pointer_width with exit 22
  ├── cli::parse()
  │    ├── parse error -> stderr + exit 20
  │    └── Command
  └── dispatch
       ├── Command::Run(opts)     -> run_command(opts)
       ├── Command::Imports(opts) -> imports_command(opts)
       └── Command::Help          -> print usage + exit 0
```

Important functions in `main.rs`:

- `run_command(opts: RunOptions) -> i32`
- `imports_command(opts: ImportsOptions) -> i32`
- `emit_run_events(exe_path, cwd, outcome)`
- `diagnose_static_imports(...) -> Result<StaticReport, String>`
- `detect_dynamic_missing_from_debug_strings(...)`
- `run_result_code(...) -> i32`
- path normalization helpers such as `normalize_existing_path(...)` and `normalize_existing_run_target(...)`

### `run_command` responsibilities

`run_command` owns the full `loadwhat run` pipeline:

1. Normalize the target executable path.
2. Resolve the working directory.
3. Run the target under the debug loop via `debug_run::run_target(...)`.
4. Enable loader snaps by default:
   - first tries PEB-based enabling through `debug_run::run_target(..., enable_loader_snaps_peb = true)`;
   - if PEB enabling fails in a recoverable way, enables IFEO loader snaps through `loader_snaps::LoaderSnapsGuard::enable_for_image(...)`, retries the run without PEB enable, then restores the guard.
5. Build runtime-loaded and runtime-observed module sets from `RunOutcome`.
6. Decide whether static import diagnosis is needed:
   - loader-related exception; or
   - heuristic early non-zero exit with few loaded modules.
7. Run recursive static diagnosis through `diagnose_static_imports(...)` when needed.
8. Detect dynamic `LoadLibrary`-style failures from loader-snaps debug strings when loader snaps are enabled and no static issue was already found.
9. Emit the selected public output tokens according to output mode.
10. Return the final process exit code.

`run_command` is the main integration point between runtime observations, static import resolution, dynamic-missing inference, output emission, and exit-code selection.

### `imports_command` responsibilities

`imports_command` handles `loadwhat imports <module> [--cwd DIR]`.

It normalizes the module path, resolves the working directory, calls `diagnose_static_imports(...)` in full static mode, emits a `SUMMARY`, and returns:

- `0` when no static missing/bad-image issues are found;
- `10` when static missing/bad-image issues are found;
- `20` for input/path errors;
- `21` for diagnosis errors.

Despite the command name, the current implementation performs recursive static diagnosis rather than only printing a direct import list.

---

## CLI parser: `src/cli.rs`

`cli.rs` is a hand-written parser using `std::env`, `OsString`, and `PathBuf`. It does not use `clap`.

### Public command model

```rust
pub enum Command {
    Run(RunOptions),
    Imports(ImportsOptions),
    Help,
}

pub struct RunOptions {
    pub exe_path: PathBuf,
    pub exe_args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: u32,
    pub loader_snaps: bool,
    pub trace: bool,
    pub verbose: bool,
}

pub struct ImportsOptions {
    pub module_path: PathBuf,
    pub cwd: Option<PathBuf>,
}
```

### Key functions

- `parse() -> Result<Command, String>`
- `parse_from<I, T>(args: I) -> Result<Command, String>`
- `usage() -> String`
- internal helpers:
  - `parse_run(...)`
  - `parse_imports(...)`
  - `run_usage()`
  - `looks_like_run_option(...)`

### `run` command parsing

Usage:

```text
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
```

All `loadwhat run` options must appear before `<TARGET>`. Once the parser reaches the first non-option token, that token becomes the target path and all following tokens become target arguments, even if they look like options.

Supported pre-target options:

- `--cwd <path>`
- `--timeout <ms>`
- `--timeout-ms <ms>`
- `--verbose`
- `-v`
- `--trace`
- `--summary`
- `--loader-snaps`
- `--no-loader-snaps`
- `--quiet`
- `--strict` currently accepted as a no-op

Defaults:

- `timeout_ms = 30_000`
- `loader_snaps = true`
- `trace = false`
- `verbose = false`

Mode interactions:

- `--verbose` / `-v` sets both `verbose = true` and `trace = true`.
- `--summary` sets `trace = false`.
- `--quiet` sets `verbose = false` but does not clear an explicitly enabled `trace`.

### `imports` command parsing

Usage:

```text
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

Supported post-module options:

- `--cwd <dir>`
- `--quiet`, `--verbose`, and `--strict` are accepted as no-ops.

Unknown options produce parse errors.

---

## Debug loop: `src/debug_run.rs`

`debug_run.rs` runs the target under the Windows debug APIs and converts raw debug events into structured runtime observations. It does not own static import diagnosis or final summary emission.

### Main API

```rust
pub fn run_target(
    exe_path: &Path,
    exe_args: &[OsString],
    cwd: Option<&Path>,
    timeout_ms: u32,
    enable_loader_snaps_peb: bool,
) -> Result<RunOutcome, RunError>
```

### Key types

```rust
pub struct LoadedModule {
    pub dll_name: String,
    pub path: Option<PathBuf>,
    pub base: usize,
}

pub struct DebugStringEvent {
    pub pid: u32,
    pub tid: u32,
    pub text: String,
}

pub enum RuntimeEvent {
    RuntimeLoaded(LoadedModule),
    DebugString(DebugStringEvent),
}

pub enum RunEndKind {
    ExitProcess,
    Exception,
    Timeout,
}

pub struct RunOutcome {
    pub pid: u32,
    pub runtime_events: Vec<RuntimeEvent>,
    pub loaded_modules: Vec<LoadedModule>,
    pub loader_snaps_peb: Option<loader_snaps::PebEnableInfo>,
    pub end_kind: RunEndKind,
    pub exit_code: Option<u32>,
    pub exception_code: Option<u32>,
    pub elapsed_ms: u128,
}

pub enum RunError {
    Message(String),
    PebLoaderSnapsEnableFailed(loader_snaps::PebEnableInfo, u32),
    UnsupportedWow64Target,
}
```

### Runtime flow

1. Validate that the target path exists.
2. Optionally check test override behavior for PEB loader-snaps enable.
3. Build a Windows command line from the executable path and target arguments.
4. Call `CreateProcessW` with `DEBUG_ONLY_THIS_PROCESS`.
5. If requested, call `loader_snaps::enable_via_peb(process_handle)` after process creation.
6. Enter a `WaitForDebugEvent` / `ContinueDebugEvent` loop.
7. Handle debug events:
   - `CREATE_PROCESS_DEBUG_EVENT`: close the file handle from the event payload.
   - `LOAD_DLL_DEBUG_EVENT`: capture module path/name/base; append `LoadedModule` and `RuntimeEvent::RuntimeLoaded`.
   - `OUTPUT_DEBUG_STRING_EVENT`: read the remote debug string with `ReadProcessMemory`; append `RuntimeEvent::DebugString`.
   - `EXCEPTION_DEBUG_EVENT`: continue breakpoints/single-step events; record terminal second-chance non-breakpoint exceptions.
   - `EXIT_PROCESS_DEBUG_EVENT`: capture exit code and end the loop.
8. Stop on exit, timeout, or unrecoverable debug API error.
9. Close process/thread handles.
10. Return `RunOutcome`.

### Important helpers

- `build_command_line(exe_path, exe_args) -> String`
- `quote_cmd_arg(arg) -> String`
- `read_output_debug_string(...) -> Option<String>`
- `debug_string_text(...) -> String`
- `read_remote_image_name(...) -> Option<String>`
- `read_remote_utf16(...) -> Option<String>`
- `read_remote_ansi(...) -> Option<String>`
- `determine_end_kind(...) -> RunEndKind`
- `loaded_module_name(path, base) -> String`

`debug_run.rs` collects data. Token emission for runtime events happens later in `main.rs` through `emit_run_events(...)`.

---

## Loader snaps: `src/loader_snaps.rs`

`loader_snaps.rs` enables and restores loader-snaps behavior. It does not enumerate loaded modules and does not implement a separate `infer_loader_snaps(...)` API.

### Main responsibilities

- Enable loader snaps in a newly created target process by writing `FLG_SHOW_LDR_SNAPS` into the target PEB `NtGlobalFlag`.
- Detect unsupported WOW64 targets for the current v1 flow.
- Fall back to IFEO registry-based loader-snaps enablement when `main.rs` chooses that path.
- Preserve and restore the previous IFEO `GlobalFlag` value through an RAII guard.

### Key constants/concepts

- `FLG_SHOW_LDR_SNAPS = 0x0000_0002`
- IFEO base: `SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options`
- IFEO value: `GlobalFlag`
- x64 PEB `NtGlobalFlag` offset currently selected as `0xBC`

### Key types and functions

```rust
pub struct PebEnableInfo {
    pub os_version: Option<win::OsVersion>,
    pub ntglobalflag_offset: usize,
}

pub enum PebEnableError {
    UnsupportedWow64,
    Win32 { code: u32, info: PebEnableInfo },
}

pub struct LoaderSnapsGuard { ... }

impl LoaderSnapsGuard {
    pub fn enable_for_image(image_name: &str) -> Result<Self, u32>;
    pub fn restore(&mut self) -> Result<(), u32>;
}

pub fn enable_via_peb(process: win::Handle) -> Result<PebEnableInfo, PebEnableError>;

pub(crate) fn test_peb_enable_override_result()
    -> Option<Result<PebEnableInfo, PebEnableError>>;
```

`LoaderSnapsGuard` restores the original IFEO registry value on explicit `restore()` or in `Drop`.

Dynamic missing DLL detection is performed in `main.rs` by analyzing loader-snaps `DEBUG_STRING` output captured by `debug_run.rs`.

---

## PE parsing: `src/pe.rs`

`pe.rs` parses PE files directly from raw bytes. It is intentionally narrow: it extracts direct import DLL names and performs lightweight PE validity checks for search candidate classification.

### Public API

```rust
pub fn direct_imports(module_path: &Path) -> Result<Vec<String>, String>;

pub fn is_probably_pe_file(module_path: &Path) -> bool;
```

### Internal flow

`direct_imports(...)`:

1. Reads the module bytes from disk.
2. Calls `direct_imports_from_bytes(...)`.
3. Parses the DOS header and PE header.
4. Supports PE32 (`0x010B`) and PE32+ (`0x020B`) optional headers.
5. Locates the import directory RVA from the optional header data directories.
6. Maps RVAs to file offsets through the section table.
7. Walks 20-byte import descriptors until the null descriptor.
8. Reads import DLL names.
9. Lowercases, deduplicates, and lexicographically sorts the names with `BTreeSet`.

`is_probably_pe_file(...)`:

1. Reads a file.
2. Calls the PE layout parser.
3. Returns `true` when the basic PE layout can be parsed.

### Representative parse errors

- `file too small for DOS header`
- `missing MZ header`
- `invalid PE header offset`
- `missing PE signature`
- `truncated optional header`
- `unsupported optional header format`
- `optional header missing data directories`
- `truncated section table`
- `invalid import table RVA`
- `truncated import descriptor table`
- `invalid import name RVA`
- `unterminated import string`
- `import name is not valid UTF-8`

Recursive dependency diagnosis is not implemented in `pe.rs`; it is implemented in `main.rs` using `pe::direct_imports(...)` plus `search::resolve_dll(...)`.

---

## DLL search: `src/search.rs`

`search.rs` constructs the effective DLL search roots and classifies candidate paths. It does not emit tokens directly; `main.rs` emits `SEARCH_ORDER` and `SEARCH_PATH` tokens from the returned search data.

### Key types

```rust
pub struct SearchContext {
    pub app_dir: PathBuf,
    pub cwd: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    pub safedll: bool,
    pub system_dir: PathBuf,
    pub windows_dir: PathBuf,
    pub system16_dir: Option<PathBuf>,
}

pub enum ResolutionKind {
    Found,
    Missing,
    BadImage,
}

pub struct CandidateResult {
    pub order: usize,
    pub path: PathBuf,
    pub result: &'static str,
}

pub struct Resolution {
    pub kind: ResolutionKind,
    pub chosen: Option<PathBuf>,
    pub candidates: Vec<CandidateResult>,
}
```

### Main API

```rust
impl SearchContext {
    pub fn from_environment(
        app_dir: &Path,
        cwd: &Path,
        path_env: Option<OsString>,
    ) -> Result<Self, String>;

    pub fn ordered_roots(&self) -> Vec<PathBuf>;
}

pub fn resolve_dll(dll_name: &str, context: &SearchContext) -> Resolution;
```

### Search root order

`SearchContext::ordered_roots()` builds roots as follows.

Always first:

1. application directory

When `SafeDllSearchMode` is enabled:

2. System32
3. optional Windows `System` directory, if present
4. Windows directory
5. current working directory, if different from app dir
6. PATH entries, in order

When `SafeDllSearchMode` is disabled:

2. current working directory, if different from app dir
3. System32
4. optional Windows `System` directory, if present
5. Windows directory
6. PATH entries, in order

After construction, roots are deduplicated case-insensitively while preserving the first occurrence.

### Candidate classification

`resolve_dll(...)` behavior:

- Absolute DLL path:
  - checks exactly that path as order `1`;
  - returns `Found`, `Missing`, or `BadImage`.
- Relative DLL name:
  - joins the name to each ordered root;
  - records each candidate with order, path, and result token:
    - `HIT`
    - `MISS`
    - `BAD_IMAGE`
  - stops at the first `HIT` or `BAD_IMAGE`;
  - returns `Missing` only after all roots miss.

A path is classified as:

- `Missing` if it does not exist;
- `Found` if `pe::is_probably_pe_file(path)` returns true;
- `BadImage` if it exists but does not look like a valid PE file.

---

## Token formatting: `src/emit.rs`

`emit.rs` owns the low-level formatting of public line-oriented tokens. It does not decide when a diagnostic should be emitted; callers in `main.rs` decide that.

### Token constants

```rust
TOKEN_DEBUG_STRING
TOKEN_DYNAMIC_MISSING
TOKEN_FIRST_BREAK
TOKEN_NOTE
TOKEN_RUN_END
TOKEN_RUN_START
TOKEN_RUNTIME_LOADED
TOKEN_SEARCH_ORDER
TOKEN_SEARCH_PATH
TOKEN_STATIC_BAD_IMAGE
TOKEN_STATIC_END
TOKEN_STATIC_FOUND
TOKEN_STATIC_IMPORT
TOKEN_STATIC_MISSING
TOKEN_STATIC_START
TOKEN_SUCCESS
TOKEN_SUMMARY
```

### Helpers

```rust
pub fn emit(token: &str, fields: &[(String, String)]);
pub fn field<K: Into<String>, V: Into<String>>(key: K, value: V) -> (String, String);
pub fn quote(value: &str) -> String;
pub fn hex_u32(value: u32) -> String;
pub fn hex_usize(value: usize) -> String;

pub struct SummaryCounts {
    pub static_missing: usize,
    pub static_bad_image: usize,
    pub dynamic_missing: usize,
    pub runtime_loaded: usize,
    pub com_issues: usize,
}

pub fn summary_fields(first_break: bool, counts: SummaryCounts) -> Vec<(String, String)>;
```

### Quoting behavior

`quote(...)` wraps values in double quotes and escapes:

- backslash as `\\`
- double quote as `\"`
- newline as `\n`
- carriage return as `\r`
- tab as `\t`

### Summary field order

`summary_fields(...)` emits fields in this order:

1. `first_break`
2. `static_missing`
3. `static_bad_image`
4. `dynamic_missing`
5. `runtime_loaded`
6. `com_issues`

---

## Win32 bindings: `src/win.rs`

`win.rs` contains the low-level Windows FFI surface used by the rest of the program.

Representative responsibilities:

- Win32 handle and integer type aliases.
- Debug event constants and structs.
- `CreateProcessW`, `WaitForDebugEvent`, `ContinueDebugEvent`, `ReadProcessMemory`, `WriteProcessMemory`, `TerminateProcess`, `CloseHandle`, and related kernel32 bindings.
- Registry APIs used for IFEO loader-snaps fallback.
- NTDLL APIs used for PEB and OS-version inspection.
- Helpers:
  - `to_wide(...)`
  - `safe_dll_search_mode() -> bool`
  - `get_system_directory() -> Result<PathBuf, String>`
  - `get_windows_directory() -> Result<PathBuf, String>`
  - `rtl_get_version() -> Option<OsVersion>`
  - `is_wow64_process_best_effort(...) -> Result<bool, u32>`
  - `final_path_from_handle(...) -> Option<PathBuf>`

`win.rs` also exposes a `TEST_ENV_LOCK` mutex used by tests that manipulate process-wide environment variables.

---

## Test utility: `src/test_util.rs`

`test_util.rs` currently contains `EnvVarGuard`, a small RAII helper for temporarily setting or removing environment variables in tests.

Key methods:

```rust
EnvVarGuard::set(name, value)
EnvVarGuard::set_os(name, value)
EnvVarGuard::remove(name)
```

On drop, it restores the previous value or removes the variable if it did not previously exist.

---

## Integration tests

`tests/integration.rs` is the integration-test module registry. Most integration tests are compiled only when both conditions hold:

```rust
#[cfg(all(windows, feature = "harness-tests"))]
```

The registered integration modules cover areas such as:

- CLI validation and run contract behavior
- run output modes
- timeout behavior
- unreadable debug strings
- loader-snaps contract and note behavior
- dynamic `LoadLibrary` by name and full path
- nested dynamic loads
- dynamic missing direct cases
- dynamic multiple candidate search behavior
- static missing direct/transitive cases
- static bad-image direct/transitive cases
- circular/deep/shared dependency graphs
- path search order and de-duplication
- imports mode edge cases and stability
- malformed PE handling
- real-world runtime scenarios
- post-init crash behavior
- verbose static/dynamic combined output

The harness has separate support code under `tests/harness/`.

---

## High-level data flow

### `loadwhat run`

```text
cli::parse()
  -> Command::Run(RunOptions)
     -> main.rs::run_command(opts)
        ├── normalize target and cwd
        ├── debug_run::run_target(...)
        │    ├── CreateProcessW(DEBUG_ONLY_THIS_PROCESS)
        │    ├── optional loader_snaps::enable_via_peb(...)
        │    ├── WaitForDebugEvent loop
        │    └── RunOutcome { runtime_events, loaded_modules, exit/exception info }
        ├── optional IFEO fallback through loader_snaps::LoaderSnapsGuard
        ├── build runtime-loaded/runtime-observed module sets
        ├── maybe diagnose_static_imports(...)
        │    ├── pe::direct_imports(...)
        │    ├── search::SearchContext::from_environment(...)
        │    ├── search::resolve_dll(...)
        │    └── StaticReport { counts, first_issue, safedll }
        ├── maybe detect_dynamic_missing_from_debug_strings(...)
        ├── emit selected output tokens
        └── return final exit code
```

### `loadwhat imports`

```text
cli::parse()
  -> Command::Imports(ImportsOptions)
     -> main.rs::imports_command(opts)
        ├── normalize module path and cwd
        ├── diagnose_static_imports(..., StaticEmitMode::Full)
        │    ├── pe::direct_imports(...)
        │    └── search::resolve_dll(...)
        ├── emit full static tokens
        ├── emit SUMMARY
        └── return 0 / 10 / 20 / 21
```

---

## Output modes and token behavior

The exact public token contract should be kept aligned with `docs/loadwhat_spec_v1.md` and source emission sites in `main.rs`.

### Default `run` mode

Command:

```text
loadwhat run <TARGET> [TARGET_ARGS...]
```

Default mode is summary-oriented and not verbose.

Typical successful output:

```text
SUCCESS status=0
```

When a first static issue is diagnosed, default mode emits a single issue token such as:

```text
STATIC_MISSING module="..." dll="..." reason="NOT_FOUND"
```

or:

```text
STATIC_BAD_IMAGE module="..." dll="..." reason="BAD_IMAGE"
```

When a dynamic missing DLL is detected from loader-snaps debug strings, default mode emits:

```text
DYNAMIC_MISSING dll="..." reason="..." [status=0x...]
```

Default mode does not normally emit the full runtime event timeline.

### Trace mode

Command:

```text
loadwhat run --trace <TARGET> [TARGET_ARGS...]
```

Trace mode emits diagnostic search details when a failure is diagnosed. Depending on the failure type and verbosity, relevant tokens can include:

- `SEARCH_ORDER safedll=0|1`
- `SEARCH_PATH dll="..." order=N path="..." result="MISS|HIT|BAD_IMAGE"`
- `STATIC_MISSING ...`
- `STATIC_BAD_IMAGE ...`
- `DYNAMIC_MISSING ...`

Non-verbose trace mode is still not the full runtime event timeline.

### Verbose mode

Command:

```text
loadwhat run -v <TARGET> [TARGET_ARGS...]
```

`-v` / `--verbose` implies trace and adds the runtime timeline emitted by `emit_run_events(...)`:

```text
RUN_START exe="..." cwd="..." pid=N
RUNTIME_LOADED pid=N dll="..." path="..." base=0x...
DEBUG_STRING pid=N tid=N source="OUTPUT_DEBUG_STRING_EVENT" text="..."
RUN_END pid=N exit_kind="EXIT_PROCESS|EXCEPTION|TIMEOUT" code=0x...
SUMMARY first_break=true|false static_missing=N static_bad_image=N dynamic_missing=N runtime_loaded=N com_issues=N
```

Verbose static diagnosis can also emit:

```text
STATIC_START module="..." scope="direct-and-recursive-imports"
SEARCH_ORDER safedll=0|1
STATIC_IMPORT module="..." needs="..."
SEARCH_PATH dll="..." order=N path="..." result="..."
STATIC_FOUND module="..." dll="..." path="..."
STATIC_FOUND module="..." dll="..." reason="RUNTIME_OBSERVED"
STATIC_MISSING module="..." dll="..." reason="NOT_FOUND" [via="..." depth=N]
STATIC_BAD_IMAGE module="..." dll="..." reason="BAD_IMAGE"
STATIC_END module="..."
```

### Imports mode

Command:

```text
loadwhat imports <EXE_OR_DLL> [--cwd DIR]
```

Current imports mode runs full recursive static diagnosis and emits static-analysis tokens, ending with:

```text
SUMMARY first_break=false static_missing=N static_bad_image=N dynamic_missing=0 runtime_loaded=0 com_issues=0
```

It returns `10` if the recursive static diagnosis found missing or bad-image DLLs.

---

## Exit-code summary

Observed from `main.rs` behavior:

- `0`: success/help, or acceptable timeout with runtime modules loaded.
- `10`: diagnosed missing/bad-image/dynamic DLL issue.
- `20`: command-line/input/path error.
- `21`: runtime/debug/diagnosis failure not classified as a dependency issue.
- `22`: unsupported platform/architecture/WOW64 target path.

Test-mode behavior can intentionally alter some return codes for harness scenarios.

---

## Common implementation notes

### Do not add logic to the wrong module

- Static recursive diagnosis currently lives in `main.rs`, not `pe.rs`.
- Dynamic missing detection from loader-snaps debug strings currently lives in `main.rs`, not `loader_snaps.rs`.
- Search candidate construction/classification lives in `search.rs`, but token emission for search results lives in `main.rs`.
- Runtime event collection lives in `debug_run.rs`, but public runtime token emission lives in `main.rs`.

### Preserve deterministic behavior

When adding or changing diagnosis output:

- avoid unordered map/set iteration for public output ordering;
- preserve search candidate order;
- preserve sorted PE import behavior;
- keep token field order stable;
- update tests when the public token contract changes.

### Be careful with Windows process-global state

Loader snaps fallback can modify IFEO registry state. Environment-variable test hooks are process-global. Tests that manipulate these use synchronization through `win::TEST_ENV_LOCK` and `EnvVarGuard`.

### Keep path behavior explicit

Path normalization and case-insensitive comparison are significant on Windows. Existing code normalizes module visit keys and de-duplicates search roots case-insensitively. Changes to path handling can affect integration tests, especially mapped-drive and app-dir-equals-cwd cases.

---

## Items that should not appear in this map unless added to the repo

These names have appeared in stale/generated maps but are not current source APIs:

- `src/cmd.rs`
- `cmd_run(...)`
- `cmd_imports(...)`
- `debug_run::run(...) -> SummaryCounts`
- `RuntimeState`
- `DllLoadRecord`
- `pe::static_diagnose(...)`
- `pe::print_imports(...)`
- `pe::parse_pe(...)`
- `pe::extract_imports(...)`
- `pe::recursive_walk(...)`
- `PeFile`
- `search::SearchStep`
- `search::SearchResult`
- `search::resolve_search_order(...)`
- `loader_snaps::infer_loader_snaps(...)`
- `LoaderSnapEntry`
- `src/lib.rs` module re-export layer
- `test_util::fixture_dir(...)`
- `test_util::run_loadwhat(...)`

If any of these are introduced later, update this section and the relevant module descriptions.
