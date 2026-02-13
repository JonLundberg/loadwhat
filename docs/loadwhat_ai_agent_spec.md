# Windows Rust Debug-API Utility — AI Agent Setup Spec

> **Purpose:** This document is written to be pasted into an AI coding agent (Codex/Cursor/etc.) to generate a repo that builds a **Windows-only Rust CLI** using the **Win32 Debug API directly**, with a **Windows Dev Container** workflow in VS Code.

---

## 1) What to build

Create a **Windows x64 Rust CLI utility** (single `.exe`) focused on diagnosing **DLL loading problems** by running (or attaching to) a target process under a minimal debugger loop.

### Core constraints

- **Windows-only**, **x64 only**
- **No runtime dependencies** beyond the OS:
  - No .NET, no Python, no Java, no separate helper EXEs
  - No requiring dbgeng/windbg/cdb
- Use **Win32 Debug API directly**:
  - `CreateProcessW` with debug flags and/or `DebugActiveProcess`
  - `WaitForDebugEvent`, `ContinueDebugEvent`
  - Handle debug events for module loads/unloads and process exit

### Initial focus

Deliver a first milestone that provides **good observability**:
- Print module load events (DLL name/path when possible)
- Highlight suspected missing DLLs / load failures with best-effort inference
- Support “run under debug” at minimum

---

## 2) Repo goal: microsoft/edit-like structure, but Windows containers

We want a repo structure inspired by `microsoft/edit` (containerized build environment in-repo), but **not Linux**.

A developer should be able to:

1. Clone the repo
2. Open the folder in **VS Code**
3. Choose **Dev Containers: Reopen in Container**
4. Build inside the Windows container (Debug/Release) and get a single `.exe`

---

## 3) Target platforms & environment

- **Developer host:** Windows 10/11
- **Container:** Windows container image (Server Core recommended)
- **Output:** Windows 10/11 compatible x64 `.exe`

---

## 4) CLI requirements

Implement at least:

- `loadwhat run <path-to-exe> [-- <args...>]`
  - Launch target under debug and report loader issues
- Optional (nice-to-have for v1):
  - `loadwhat attach <pid>`

### Output requirements (v1)

- Human-readable stdout logging:
  - process start, PID
  - `LOAD_DLL` events: best-effort module path + base address
  - process exit code
- Optional flag:
  - `--json` (can be a follow-up milestone)

---

## 5) Debug loop behavior (minimum viable)

Implement a standard Windows debug loop:

Handle at least these events:

- `CREATE_PROCESS_DEBUG_EVENT`
- `LOAD_DLL_DEBUG_EVENT`
- `UNLOAD_DLL_DEBUG_EVENT`
- `EXIT_PROCESS_DEBUG_EVENT`
- `EXCEPTION_DEBUG_EVENT` (log basic info; keep simple)

### Notes for module path retrieval

`LOAD_DLL_DEBUG_EVENT` often includes:
- `hFile` (file handle to the DLL)
- `lpImageName` (pointer in debuggee memory to name, may be Unicode/ANSI)
- `fUnicode` (name encoding)

Best-effort approaches (in descending reliability):
1. If `hFile` is valid, use `GetFinalPathNameByHandleW` to get full path.
2. If `lpImageName` is present, read remote memory with `ReadProcessMemory`.
3. Fallback: print base address only and mark path as unknown.

---

## 6) Rust implementation constraints

- Rust **stable**
- Prefer minimal dependencies.
- Suggested crates:
  - Prefer **`windows-sys`** for low-level bindings (lightweight).
  - Optionally `clap` for CLI parsing (if you choose it, keep it minimal).
- Do **not** use dbgeng / DIA / external debuggers.

---

## 7) Deliverables

### A) Working Rust project

- `cargo build -r` builds a single exe (e.g., `target\release\loadwhat.exe`)
- Running `loadwhat run C:\Windows\System32\notepad.exe` prints some DLL loads and exits cleanly.

### B) Windows Dev Container setup (in repo)

Include:

- `.devcontainer/devcontainer.json`
- `.devcontainer/Dockerfile`
- `.vscode/tasks.json`

So a developer can build using VS Code tasks inside the container.

### C) README onboarding

README must cover:

- Docker Desktop prerequisites and **Windows container mode**
- How to open in VS Code Dev Containers
- How to build (Debug/Release)
- Example usage

---

## 8) Suggested repository layout

```
/ (repo root)
  README.md
  Cargo.toml
  Cargo.lock

  /src
    main.rs
    cli.rs
    debug_loop.rs
    report.rs
    win32.rs

  /.devcontainer
    devcontainer.json
    Dockerfile
    setup.ps1

  /.vscode
    tasks.json
    launch.json          (optional)

  /scripts
    build.ps1
    test-run.ps1         (optional)

  /tests
    /fixtures            (optional)
```

---

## 9) Dev Container requirements (Windows containers)

### Key requirement

The container must be Windows-based (e.g., `mcr.microsoft.com/windows/servercore:ltsc2022` or a compatible variant), because we need a Windows toolchain and want to validate Windows debug APIs.

### The container image must provide

- Visual Studio Build Tools (MSVC x64)
- Windows SDK
- Rust toolchain (rustup + stable)
- Git + PowerShell

### VS Code integration

- `devcontainer.json` should:
  - build from `.devcontainer/Dockerfile`
  - mount the workspace
  - run a post-create step to ensure Rust toolchain is installed

### Build tasks

- `.vscode/tasks.json` should provide:
  - `Build (Debug)` → `cargo build`
  - `Build (Release)` → `cargo build -r`

---

## 10) Acceptance criteria

- On Windows 10/11 host:
  - Opening repo in VS Code → **Reopen in Container** succeeds.
  - Running build tasks works inside container.
  - Produces a single `.exe` in `target\release`.
- Running:
  - `loadwhat run C:\Windows\System32\notepad.exe`
  - prints at least module load events and exits cleanly.

---

## 11) Implementation guidance (agent notes)

- The v1 goal is **observability** and a clean debug loop, not perfect diagnosis.
- Keep code readable and separated:
  - CLI parsing separate from debug loop
  - reporting separate from event handling
- Prefer explicit error handling with clear messages.

---

## 12) README template (agent should generate)

Include a README that roughly contains:

- **Prereqs**
  - Windows 10/11
  - Docker Desktop
  - Switch to **Windows containers**
  - VS Code + Dev Containers extension
- **Open in container**
  - “Dev Containers: Reopen in Container”
- **Build**
  - `cargo build`
  - `cargo build -r`
- **Run**
  - `.\target\release\loadwhat.exe run C:\Windows\System32\notepad.exe`

---

## 13) Non-goals (v1)

- No interactive debugger UI
- No TUI required
- No symbol server / PDB parsing
- No hooking/injection

---

## 14) Naming

Tool name can be `loadwhat` (placeholder). Use consistent naming across:
- crate name
- exe name
- README examples

---

**End of spec.**
