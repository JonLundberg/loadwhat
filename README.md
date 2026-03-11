# loadwhat

`loadwhat` is a Windows x64 Rust CLI for diagnosing DLL loading failures using Win32 debug APIs directly.

## Current commands

```text
loadwhat run [--cwd <dir>] [--timeout <n>] [--no-loader-snaps] [--trace|--summary] [-v|--verbose] <exe_path> [args...]
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

## Build

```powershell
cargo build
cargo build --release
```

Release output:

```text
target\release\loadwhat.exe
```

## Output modes

- Default `run` mode is summary:
  - emits exactly one line for first-break diagnosis (`STATIC_MISSING`, `STATIC_BAD_IMAGE`, or `DYNAMIC_MISSING`)
  - emits `SUCCESS status=0` when startup succeeds without a diagnosed load issue
- `--trace` enables detailed diagnostic trace output (`SEARCH_ORDER`, `SEARCH_PATH`, and related diagnosis lines).
- `-v`/`--verbose` implies `--trace` and adds runtime timeline tokens:
  - `RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`
  - plus full static/search/summary tokens
  - verbose `SUMMARY` uses explicit counters: `first_break`, `static_missing`, `static_bad_image`, `dynamic_missing`, `runtime_loaded`, `com_issues`

`run` Phase B performs direct import diagnosis and an always-on recursive missing-dependency walk (transitive missing detection).

By default, `loadwhat` enables loader-snaps Phase C and can heuristically infer handled dynamic `LoadLibrary*` failures from loader-snaps debug strings and emit `DYNAMIC_MISSING`. Use `--no-loader-snaps` to disable that phase. When multiple dynamic-failure candidates are observed in one run, `loadwhat` prefers the earliest unresolved app-relevant failure and ignores candidates for DLLs that later load successfully. See `docs/loadwhat_spec_v1.md` for the authoritative selection rules.
Loader-snaps setup uses best-effort `PEB->NtGlobalFlag` enable with Windows version/build detection to pick the x64 offset.

## Token style

Output is line-oriented and greppable:

```text
TOKEN key=value key=value ...
```

Common token families:

- `RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`
- `STATIC_*` (`STATIC_IMPORT`, `STATIC_MISSING`, `STATIC_BAD_IMAGE`, ...)
- `SEARCH_ORDER`, `SEARCH_PATH`
- `FIRST_BREAK`, `SUMMARY`, `NOTE`
- `DYNAMIC_MISSING` (loader-snaps dynamic inference)

Transitive missing reports may include optional fields on `STATIC_MISSING`, for example:

```text
STATIC_MISSING dll="lwtest_b.dll" via="lwtest_a.dll" depth=2
```

## Examples

Run with default summary output:

```powershell
.\target\release\loadwhat.exe run C:\path\to\myapp.exe
# example output:
# DYNAMIC_MISSING dll="b.dll" reason="NOT_FOUND"
```

Run with full trace output:

```powershell
.\target\release\loadwhat.exe run --trace C:\path\to\myapp.exe
```

Run with verbose trace output:

```powershell
.\target\release\loadwhat.exe run -v C:\Windows\System32\notepad.exe
```

Run with target arguments:

```powershell
.\target\release\loadwhat.exe run --verbose C:\path\to\myapp.exe --mode test --threads 4
```

Disable loader-snaps Phase C:

```powershell
.\target\release\loadwhat.exe run --no-loader-snaps C:\path\to\myapp.exe
```

Scan imports offline (including recursive missing-dependency walk):

```powershell
.\target\release\loadwhat.exe imports C:\Windows\System32\notepad.exe
```

## Docs

- Authoritative behavior spec: `docs/loadwhat_spec_v1.md`
- Contribution/testing workflow: `CONTRIBUTING.md`, `docs/testing.md`
- Out-of-scope and planned features: `docs/roadmap.md`
