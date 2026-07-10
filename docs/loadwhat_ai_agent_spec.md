# loadwhat AI Agent Guidance (non-authoritative)

This document is guidance for coding agents working in this repository.

> Authority: use `docs/loadwhat_spec_v2.md` first, then `docs/loadwhat_spec_v1.md` for `run` / `imports`, then `AGENTS.md`.
> This file does not define features by itself.

## Repository scope (current)

`loadwhat` is a Windows x64 Rust CLI that uses Win32 Debug APIs directly for `run`, performs static PE import analysis, and provides deterministic COM registration and activation-prerequisite diagnosis.

Current supported commands:

```text
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
loadwhat imports <exe_or_dll> [--cwd <dir>]
loadwhat com clsid [OPTIONS] <{CLSID}>
loadwhat com progid [OPTIONS] <PROGID>
loadwhat com server [OPTIONS] <PATH>
loadwhat com audit [OPTIONS] <TARGET> <{CLSID}|PROGID>
```

## Output contract reminder

Output is tokenized and line-oriented:

```text
TOKEN key=value key=value ...
```

Agents should preserve token stability and deterministic ordering requirements defined in `docs/loadwhat_spec_v2.md`.

## Working rules for agents

- Implement only behavior required by the authoritative spec.
- Keep changes minimal and scoped.
- Avoid adding dependencies without strong justification.
- Prefer direct Win32 API usage and explicit error handling.
- Maintain deterministic output behavior.

## Not in current v2 scope

These are roadmap items, not current implementation requirements:

- attach-to-process workflows
- JSON output
- `run --com` / `imports --com` enrichment
- custom search modes
- WOW64 target support (32-bit target process handling)

See `docs/roadmap.md` for planned/out-of-scope items.
