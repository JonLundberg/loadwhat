# loadwhat

`loadwhat` is a Windows x64 Rust CLI for diagnosing DLL loading failures in no-GUI environments.

It is designed to stay lightweight:
- single executable
- no external Rust crate dependencies
- direct Win32 API calls for debugger events

## Current commands

```text
loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [-- <args...>]
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

## Examples

Run a process under a minimal debug loop:

```powershell
.\target\release\loadwhat.exe run C:\Windows\System32\notepad.exe
```

Scan direct static imports and print ordered search candidates:

```powershell
.\target\release\loadwhat.exe imports C:\Windows\System32\notepad.exe
```

## Output style

Output is line-oriented and greppable:

```text
TOKEN key=value key=value ...
```

Typical tokens include:
- `RUN_START`
- `RUNTIME_LOADED`
- `RUN_END`
- `STATIC_START`
- `STATIC_IMPORT`
- `STATIC_FOUND`
- `STATIC_MISSING`
- `STATIC_BAD_IMAGE`
- `SEARCH_ORDER`
- `SEARCH_PATH`
- `FIRST_BREAK`
- `SUMMARY`

## Open source workflow

This repo includes:
- MIT license
- contribution guide
- GitHub Actions workflow for Windows build/test

## Project status

The current implementation focuses on:
- direct import diagnostics
- stable CLI output
- reliable baseline behavior without GUI dependencies

Follow-up milestones are tracked in `docs/loadwhat_spec_v1.md`.
