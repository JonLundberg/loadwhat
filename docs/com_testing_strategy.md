# COM Testing Strategy

This document describes the recommended testing strategy for planned COM support.

It is implementation guidance, not part of the public CLI contract. The draft behavior contract lives in [docs/loadwhat_spec_v2.md](./loadwhat_spec_v2.md). The design rationale lives in [docs/com_feature_analysis.md](./com_feature_analysis.md).

## Goals

The COM test strategy should preserve the same qualities as the existing v1 harness:

- deterministic
- isolated
- reproducible
- fast on the common path
- able to validate real Windows registry behavior when necessary

## Testing tiers

### Tier 1: mock-based unit and resolver tests

Runs with:

```text
cargo test
```

This tier should cover most COM logic without touching the host registry.

Recommended scope:

- HKCU/HKLM merge behavior
- 32-bit versus 64-bit view selection
- ProgID and `CurVer` resolution
- `TreatAs` resolution and loop detection
- supported server-kind extraction
- `LocalServer32` executable parsing
- path normalization
- manifest parsing for target-scoped `com audit`
- token emission and output-mode suppression
- status selection and exit-code mapping

Recommended implementation approach:

- define injectable registry, filesystem, and manifest-reader interfaces
- keep the main resolver logic independent from raw Win32 calls
- treat FFI layers as adapters over the pure resolver

Tier 1 should be the default place for new COM tests.

## Tier 2: fixture-backed file tests

Runs with:

```text
cargo xtask test
```

This tier validates the parts that need real PE files on disk but do not require real registry mutation.

Recommended scope:

- `com server <path>` against real fixture DLLs and EXEs
- machine-type detection
- bad-image handling
- dependency-walk integration on real files
- manifest parsing on fixture executables with embedded or sidecar manifests

Recommended fixture scenarios:

- valid x64 DLL
- valid x86 DLL
- DLL with missing transitive dependency
- bad-image DLL
- EXE suitable for `LocalServer32`
- target EXE with embedded registration-free COM manifest

## Tier 3: container-based system tests

Runs with:

```text
cargo xtask test-container
```

This tier exists for the Windows behaviors that mocks should not be trusted to simulate.

Recommended scope:

- real HKCU/HKLM merge behavior
- real `KEY_WOW64_32KEY` / `KEY_WOW64_64KEY` handling
- access-denied behavior and exact Win32 error mapping
- end-to-end CLI output against disposable registry state

Windows containers are the safest place to create and destroy COM registrations because:

- `HKLM` and `HKCU` are isolated from the host
- fixture registrations can be created freely
- IFEO writes remain container-local

## Representative container scenarios

The container suite should cover at least:

- basic `InprocServer32` registration
- ProgID with `CurVer`
- HKCU overriding HKLM
- 64-bit and 32-bit view mismatch
- missing server file
- broken server imports
- `LocalServer32` with quoted executable and arguments
- `TreatAs` redirect
- `TreatAs` cycle
- access-denied key

## CI guidance

Recommended CI split:

- every PR: Tier 1 plus Tier 2
- nightly or pre-release: Tier 1, Tier 2, and Tier 3

Tier 3 should not be the only place a logic path is tested. If a container test exposes a bug, add a Tier 1 test for the same resolver path so the regression becomes cheap to catch.

## Fixture design principles

COM fixtures should be:

- deterministic
- isolated
- self-contained
- documented

Use unique synthetic CLSIDs for every scenario to avoid accidental collisions with real system registrations.

## Suggested boundaries

The testing plan should stay separate from the public spec for two reasons:

- the public contract needs to stay compact and stable
- test infrastructure will evolve faster than CLI behavior

If COM support expands beyond V2 into `run --com` or runtime tracing, add those tests here rather than growing the public spec document.
