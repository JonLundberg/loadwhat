# loadwhat Roadmap (non-authoritative)

This file lists planned or previously-considered features that are not part of current v1 behavior.

Authoritative current behavior is defined by `docs/loadwhat_spec_v1.md`.

## Not implemented (candidate future work)

- `com progid <name>` and `com clsid <{CLSID}>` helpers
- output/report file option (`--report`)
- environment injection option (`--env KEY=VALUE`)
- stricter/warning policy mode (`--strict`)
- quiet output mode (`--quiet`)
- WOW64 target support (32-bit process on 64-bit Windows):
  - parse x86 PE imports and resolve with the same v1 search order
  - support loader-snaps enable via PEB32 and/or correct IFEO handling
  - use `NtQueryInformationProcess(ProcessWow64Information)` and `PEB32->NtGlobalFlag` offset `0x68`

## Explicitly removed / out of scope

- attach to existing process
- JSON output mode
- custom search path knobs or custom search modes

## Graduation policy

A roadmap item should only move into `docs/loadwhat_spec_v1.md` after:

1. implementation exists in the main executable,
2. automated tests cover behavior and tokens,
3. token contract (fields and ordering) is documented.
