# COM Feature Analysis for loadwhat

This document is the design rationale for planned COM support in `loadwhat`.

It is intentionally not the public contract. The draft behavior contract lives in [docs/loadwhat_spec_v2.md](./loadwhat_spec_v2.md). The COM-specific testing approach lives in [docs/com_testing_strategy.md](./com_testing_strategy.md).

Current implemented behavior remains defined by [docs/loadwhat_spec_v1.md](./loadwhat_spec_v1.md).

## Mission framing

`loadwhat` should not try to become a general-purpose COM debugger.

The best extension is narrower:

- keep the core mission: explain first-order loadability and machine-configuration failures
- extend that mission to cover COM activation prerequisites that are observable and deterministic
- prefer factual answers over repair advice

That means COM support should answer questions like:

- what server is registered for this CLSID or ProgID
- which hive and registry view supplied that registration
- whether the registered server file exists and is a valid image
- whether that server matches the caller's architecture
- whether the server itself has broken transitive DLL dependencies

## Recommended V2 scope

The draft V2 spec narrows the initial COM surface to standalone commands:

```text
loadwhat com clsid <{CLSID}>
loadwhat com progid <PROGID>
loadwhat com server <PATH>
loadwhat com audit <TARGET> <{CLSID}|PROGID>
```

V2 should cover:

- registry-backed `InprocServer32` and `LocalServer32` analysis
- HKCU/HKLM merge behavior within a chosen registry view
- 32-bit versus 64-bit registry-view handling
- ProgID and `CurVer` resolution
- `TreatAs` redirection with loop detection
- target-scoped registration-free COM manifest handling in `com audit`
- server validation and transitive dependency diagnosis

V2 should not try to fold COM into every existing command immediately.

## Why this scope is reasonable

This V2 scope aligns with `loadwhat`'s existing strengths:

- direct registry access
- deterministic PE parsing
- deterministic DLL dependency diagnosis
- careful, line-oriented public output

It avoids the largest scope explosions:

- generic COM runtime tracing
- remote COM / DCOM
- COM security and launch-permission debugging
- speculative "what version did the app want?" claims

## Evidence model

COM support should keep the same evidence discipline as the rest of the tool.

### Deterministic offline facts

These are the strongest fit for `loadwhat`:

- registry lookups for CLSID and ProgID
- `CurVer` and `TreatAs` traversal
- server path extraction from `InprocServer32` or `LocalServer32`
- file existence and PE validation
- dependency diagnosis of the resolved server binary
- target-scoped manifest parsing in `com audit`

### Deterministic target-scoped facts

`com audit` can add one more strong layer:

- determine target machine type from the PE header
- choose the registry view the target would use
- inspect the target manifest for registration-free COM declarations
- compare target context against the resolved server

### Heuristic or deferred facts

These should stay out of V2:

- runtime correlation inside `run`
- ETW-based COM tracing
- reverse-inference from loader-snaps to CLSID
- guessing app intent from nearby files or version strings

## Key engineering risks

### COM registry subsystem

The core engineering problem is not token emission. It is building a correct COM registry reader.

That subsystem must handle:

- HKCU/HKLM merge semantics
- separate 32-bit and 64-bit registry views
- `ProgID -> CurVer -> CLSID` resolution
- `TreatAs` redirection
- loop detection
- supported server-kind extraction

Without that, all higher-level COM features will be built on an unreliable foundation.

### Path normalization

Reverse lookup and server validation require precise normalization rules.

At minimum the implementation needs deterministic handling for:

- case-insensitive path comparison
- absolute path normalization
- quoted `LocalServer32` command lines
- executable extraction from `LocalServer32`
- environment expansion where applicable

This is one of the most important places to be explicit in the public spec, because otherwise output will drift between implementations.

### Status modeling

The document previously overloaded `COM_LOOKUP` so heavily that resolution state and server health blurred together.

The draft V2 spec fixes that by separating:

- lookup status: was the CLSID or ProgID resolved
- server status: is the resolved server healthy
- audit status: would activation plausibly work for this target

That makes summary output much easier to reason about.

## Registration-free COM

Registration-free COM should be target-scoped in V2.

That means:

- `com clsid` and `com progid` stay registry-based
- `com audit` is the command that may consult a target manifest
- manifest parsing should not be attempted without an explicit target context

This avoids a common conceptual bug: a global CLSID lookup cannot know which application's manifest, if any, should override registry-backed activation.

## What is explicitly deferred beyond V2

These are important areas, but they should not be in the initial V2 contract:

- `run --com` output enrichment
- `imports --com` output enrichment
- ETW-backed COM runtime tracing
- `AppID`-driven `LocalService` and `DllSurrogate` diagnosis
- `RemoteServerName` and remote COM/DCOM
- COM launch permissions, activation permissions, and COM security policy
- cross-user HKCU inspection
- full SxS assembly resolution and publisher-policy chains

Some of these may become V3 or post-V2 work, but they should not block a useful first COM release.

## Relationship between the COM docs

The COM planning docs now have distinct roles:

- [docs/loadwhat_spec_v2.md](./loadwhat_spec_v2.md): draft public behavior contract for planned V2 COM commands
- [docs/com_feature_analysis.md](./com_feature_analysis.md): design rationale and scoping decisions
- [docs/com_testing_strategy.md](./com_testing_strategy.md): implementation-facing testing and CI strategy

That split is deliberate. `loadwhat` works best when the public contract stays compact and precise, and the engineering notes stay separate from it.
