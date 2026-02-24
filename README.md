# loadwhat

`loadwhat` is a Windows x64 Rust CLI for diagnosing DLL loading failures using Win32 debug APIs directly.

## Current commands

```text
loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [--loader-snaps] [-v|--verbose] [-- <args...>]
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

- Default mode is failures-only:
  - success: no output lines
  - diagnosed load issue: emits only relevant diagnosis/search tokens
- Verbose mode (`-v`, `--verbose`) includes runtime timeline tokens:
  - `RUN_START`, `RUNTIME_LOADED`, `DEBUG_STRING`, `RUN_END`
  - plus static/search/summary tokens

With `--loader-snaps`, `loadwhat` can infer handled dynamic `LoadLibrary*` failures and emit `DYNAMIC_MISSING` with search candidates.

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

## Examples

Run with default failures-only output:

```powershell
.\target\release\loadwhat.exe run C:\Windows\System32\notepad.exe
```

Run with loader-snaps and verbose runtime output:

```powershell
.\target\release\loadwhat.exe run C:\Windows\System32\notepad.exe --loader-snaps -v
```

Scan direct imports offline:

```powershell
.\target\release\loadwhat.exe imports C:\Windows\System32\notepad.exe
```

## Docs

- Authoritative behavior spec: `docs/loadwhat_spec_v1.md`
- Contribution/testing workflow: `CONTRIBUTING.md`, `docs/testing.md`
- Out-of-scope and planned features: `docs/roadmap.md`
