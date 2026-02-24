# loadwhat Roadmap (non-authoritative)

This file lists planned or previously-considered features that are not implemented in the current repo.

> Authoritative behavior is defined by `docs/loadwhat_spec_v1.md`.
> Do not implement roadmap items unless explicitly requested.

## Not implemented (candidate future work)

- `com progid <name>` and `com clsid <{CLSID}>` helpers
- Output/report file generation (`--report`)
- Environment injection (`--env KEY=VALUE`)
- Output mode flags (`--quiet`, `--strict`)

## Explicitly removed (do not implement)

- attach to an existing PID
- recursive import scanning
- JSON output
- custom search path knobs / custom search modes

## When an item graduates into the v1 spec

An item should only be added to `docs/loadwhat_spec_v1.md` after:

1. it is implemented in the main executable,
2. it has automated tests (integration and/or unit),
3. it has stable token output documented (fields + ordering).
