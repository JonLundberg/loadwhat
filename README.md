# loadwhat

`loadwhat` is a Windows x64 Rust CLI for diagnosing DLL loading failures using Win32 debug APIs directly.

## Current commands

```text
loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [--loader-snaps] [--trace|--summary] [-v|--verbose] [-- <args...>]
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

`run` Phase B performs direct import diagnosis and an always-on recursive missing-dependency walk (transitive missing detection).

With `--loader-snaps`, `loadwhat` can infer handled dynamic `LoadLibrary*` failures and emit `DYNAMIC_MISSING` with search candidates.
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
.\target\release\loadwhat.exe run C:\path\to\myapp.exe --trace
```

Run with loader-snaps and verbose trace output:

```powershell
.\target\release\loadwhat.exe run C:\Windows\System32\notepad.exe --loader-snaps -v
```

Scan imports offline (including recursive missing-dependency walk):

```powershell
.\target\release\loadwhat.exe imports C:\Windows\System32\notepad.exe
```

## Docs

- Authoritative behavior spec: `docs/loadwhat_spec_v1.md`
- Contribution/testing workflow: `CONTRIBUTING.md`, `docs/testing.md`
- Out-of-scope and planned features: `docs/roadmap.md`
