# loadwhat AI Agent Guidance (non-authoritative)

This document is guidance for coding agents working in this repository.

> Authority: use `docs/loadwhat_spec_v1.md` first, then `AGENTS.md`.
> This file does not define features by itself.

## Repository scope (current)

`loadwhat` is a Windows x64 Rust CLI that uses Win32 Debug APIs directly.

Current supported commands:

```text
loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [--loader-snaps] [-v|--verbose] [-- <args...>]
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

## Output contract reminder

Output is tokenized and line-oriented:

```text
TOKEN key=value key=value ...
```

Agents should preserve token stability and deterministic ordering requirements defined in `docs/loadwhat_spec_v1.md`.

## Working rules for agents

- Implement only behavior required by the authoritative spec.
- Keep changes minimal and scoped.
- Avoid adding dependencies without strong justification.
- Prefer direct Win32 API usage and explicit error handling.
- Maintain deterministic output behavior.

## Not in current v1 scope

These are roadmap items, not current implementation requirements:

- attach-to-process workflows
- JSON output
- COM helper subcommands
- custom search modes and recursive import analysis

See `docs/roadmap.md` for planned/out-of-scope items.
