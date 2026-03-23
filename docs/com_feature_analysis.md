# COM Feature Analysis for loadwhat

This document analyzes how `loadwhat` could be extended to help diagnose COM activation problems without drifting away from its current mission.

This is a non-authoritative design note and feasibility analysis. It is not a spec. Current behavior is still defined by [docs/loadwhat_spec_v1.md](./loadwhat_spec_v1.md). When COM features are implemented, precise behavioral contracts should be written into a separate `loadwhat_spec_v2.md` that inherits the v1 spec's structure and precision.

Today, `loadwhat` only supports:

```text
loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]
loadwhat imports <exe_or_dll> [--cwd <dir>]
```

Roadmap-only COM helpers are listed in [docs/roadmap.md](./roadmap.md).

## Recommended mission framing

`loadwhat` should not try to become a general-purpose COM debugger.

The best fit is narrower:

- keep the primary mission: explain what broke first during process startup for DLL loading
- extend that mission to cover COM activation prerequisites that are observable and deterministic
- focus on factual answers such as:
  - what COM server is registered for this CLSID or ProgID
  - which registry view and hive supplied that registration
  - whether the registered server path or command is valid
  - whether the in-proc server is the right machine type for the current `loadwhat` build
  - whether the registered server itself has missing or bad-image dependencies

That keeps the feature aligned with `loadwhat`'s existing strengths: direct registry inspection, PE parsing, and deterministic dependency diagnosis.

## Evidence tiers

Any COM feature should separate diagnostics by evidence strength.

### Tier A: deterministic offline facts

These fit `loadwhat` well:

- registry lookups for `CLSID`, `ProgID`, `CurVer`, `TreatAs`, `AppID`, and `TypeLib`
- inspection of `InprocServer32`, `LocalServer32`, `LocalService`, and `DllSurrogate`
- validation that referenced files exist
- PE parsing for machine type and basic image validity
- recursive dependency diagnosis of in-proc COM servers using existing import walking
- manifest parsing for registration-free COM declarations

### Tier B: deterministic runtime observation

This is possible only where Windows exposes direct evidence:

- a COM server DLL load that surfaces as normal module loads or loader-snaps debug strings
- a COM server process launch that can be tied to a registered `LocalServer32` command

This tier is narrower than "observe every COM failure."

### Tier C: heuristic inference

These are possible, but weaker:

- inferring COM activity from loader-snaps alone
- scanning binaries for embedded CLSIDs
- guessing expected version requirements from surrounding files

If `loadwhat` ever emits heuristic COM output, it should be clearly distinguished from deterministic findings.

## What loadwhat already has that helps

- basic registry FFI in `win.rs`
- PE parsing infrastructure
- deterministic import walking and DLL resolution modeling
- runtime debug loop for process startup observation
- loader-snaps capture for some DLL load failures

These are a strong foundation for offline COM server validation.

They are not, by themselves, enough for full COM activation diagnosis. Important missing pieces include:

- merged `HKCR` semantics and explicit `HKCU` vs `HKLM` precedence
- registry-view handling for 64-bit vs 32-bit COM registration
- COM-specific key traversal and normalization
- version resource extraction
- manifest and SxS parsing (including registration-free COM)
- any direct runtime observation of `CoCreateInstance`-style APIs

### Engineering risk: COM registry subsystem

The current `win.rs` has basic registry FFI. COM registry traversal requires a significant new subsystem:

- HKCU/HKLM merge with correct precedence rules
- 32-bit and 64-bit registry view selection (`KEY_WOW64_32KEY` / `KEY_WOW64_64KEY`)
- ProgID chain resolution including `CurVer` versioning
- `TreatAs` redirection (potentially cyclic; needs loop detection)
- `AppID` indirection with `LocalService`, `DllSurrogate`, and `RemoteServerName`

This is the largest single engineering effort in the COM feature set and should be designed, tested, and stabilized before building higher-level features on top of it.

## COM model details the feature must respect

A useful COM diagnostic needs a more complete model than "read `HKCR\CLSID\{...}\InprocServer32`."

At minimum it should account for:

- `HKCR` being a merged view of `HKCU\Software\Classes` and `HKLM\Software\Classes`
- separate 64-bit and 32-bit registry views on x64 Windows
- `ProgID -> CLSID` resolution, including `CurVer`
- `TreatAs` redirection
- `InprocServer32` versus `LocalServer32`
- `LocalServer32` values being command lines, not just plain executable paths
- `AppID` indirection, including `LocalService` and `DllSurrogate`
- `ThreadingModel`
- `TypeLib` references and versioned type library registration
- registration-free COM via application manifests (see "Manifest and registration-free COM" below)

Without these, the tool risks reporting incomplete or misleading results.

## Registry access and permissions

Real-world COM debugging often happens in environments with restricted registry access: non-admin users, AppContainer processes, per-user vs machine-wide registration conflicts.

### Access failure handling

COM registry reads are broader than anything in v1. The tool must distinguish between:

- `NOT_REGISTERED`: the key does not exist in any accessible hive
- `ACCESS_DENIED`: the key may exist but the current user cannot read it

These are materially different findings. "Not registered" is a definitive diagnosis. "Access denied" means the answer is unknown and should be reported as such.

### Elevation awareness

`loadwhat` should detect whether it is running elevated and report this when results may be incomplete:

- non-elevated: HKLM reads may partially fail; per-user HKCU reads will reflect the current user only
- elevated: full HKLM access, but HKCU reads reflect the admin user, not necessarily the user who would run the target application

When running non-elevated and an HKLM read fails with access denied:

```text
NOTE topic="com" detail="registry-access-denied" hive="HKLM" key="CLSID\\{...}\\InprocServer32"
```

When elevation status may affect result completeness:

```text
NOTE topic="com" detail="non-elevated" message="some HKLM keys may be inaccessible"
```

## COM error taxonomy

V1 has clear failure modes for DLL diagnosis: `NOT_FOUND` and `BAD_IMAGE`. COM diagnosis needs its own taxonomy to classify the specific failure point in the activation chain:

| Status | Meaning |
|--------|---------|
| `NOT_REGISTERED` | No CLSID found in any accessible hive/view |
| `SERVER_MISSING` | Registered but referenced file does not exist |
| `SERVER_BAD_IMAGE` | File exists but is corrupt or not a valid PE |
| `SERVER_DEPS_MISSING` | Server exists but has broken transitive imports |
| `BITNESS_MISMATCH` | Server architecture does not match expected caller architecture |
| `SURROGATE_MISSING` | `DllSurrogate` specified but surrogate host not found |
| `SERVICE_MISSING` | `LocalService` specified but service not installed |
| `PROGID_BROKEN` | ProgID exists but does not resolve to a valid CLSID |
| `TREATAS_BROKEN` | `TreatAs` chain does not resolve to a usable CLSID |
| `ACCESS_DENIED` | Registry key exists but is not readable |
| `REGISTERED` | Registration found and server appears valid |

These statuses should appear in the `status` field of `COM_LOOKUP` tokens.

## Feasible feature directions

### 1. Standalone lookup helpers

These are the foundational building blocks for all other COM features.

Possible future commands:

```text
loadwhat com clsid <{CLSID}>
loadwhat com progid <Name.Object>
loadwhat com server <path-to-dll-or-exe>
```

These would be roadmap features, not current v1 behavior.

#### `com clsid` and `com progid`

A strong `com clsid` or `com progid` result should answer:

- what CLSID was resolved (and the resolution chain for ProgID: `ProgID -> CurVer -> CLSID`)
- whether resolution came from `HKCU` or `HKLM`
- whether the 64-bit or 32-bit registry view was used
- what server model is registered (`InprocServer32`, `LocalServer32`, `LocalService`)
- the raw registration string
- the expanded path or parsed command line
- whether the file exists
- whether the file is a valid PE image
- the server machine type
- whether the server's imports are loadable (reusing existing recursive walk)
- any relevant `AppID`, `TypeLib`, or `ThreadingModel` values
- if not found in registry: whether the CLSID is declared in any manifest in the target's directory (registration-free COM fallback)

This is high-value because many real COM failures reduce to bad registration, missing files, wrong bitness, or a broken in-proc server image.

#### `com server <path>`

This command validates a COM server binary and reports all COM registrations that reference it. Given a DLL or EXE path, it should:

- verify the file exists and is a valid PE image
- report machine type and whether it matches the current platform
- scan the registry for all CLSIDs whose `InprocServer32` or `LocalServer32` points to this path
- for each found registration:
  - report CLSID, hive, view, `ThreadingModel`, `AppID`
  - report any ProgIDs that map to this CLSID
- run the existing recursive import walk on the server binary
- report any missing transitive dependencies

This is the reverse lookup of `com clsid`: instead of "what server does this CLSID point to?" it answers "what CLSIDs point to this server, and is the server healthy?"

Example output:

```text
COM_SERVER path="C:\Vendor\foo.dll" exists=true machine="x64" valid_pe=true
COM_REGISTRATION clsid="{ABC...}" hive="HKLM" view="64" server_kind="InprocServer32" threading_model="Both"
COM_REGISTRATION clsid="{DEF...}" hive="HKCU" view="64" server_kind="InprocServer32" threading_model="Apartment"
COM_PROGID clsid="{ABC...}" progid="Vendor.Foo" curver="Vendor.Foo.2"
STATIC_MISSING dll="bar.dll" via="foo.dll" depth=1
```

### 2. Integration with `run` command

The most natural V2 extension is not just standalone COM commands but making `run` smarter about *why* a DLL was being loaded.

#### Automatic COM context enrichment

When `run` diagnoses a `STATIC_MISSING` or `DYNAMIC_MISSING` DLL, it should check whether that DLL is a registered COM in-proc server. If so, the output should include COM context:

```text
STATIC_MISSING dll="vendor_com.dll" via="target.exe" depth=1
COM_CONTEXT dll="vendor_com.dll" clsid="{...}" progid="Vendor.Object" server_kind="InprocServer32"
```

This does not change the diagnosis — the DLL is still missing — but it tells the user *why* the DLL was being loaded, which is often the key to fixing the problem.

#### Optional `--com` flag on `run`

A `--com` flag on `run` could enable richer COM-aware diagnosis:

- after Phase B/C diagnosis, check whether any missing DLL is a COM server
- if `ole32.dll` or `combase.dll` loads are observed followed by a DLL failure, attempt COM correlation
- emit `COM_CONTEXT` tokens alongside existing diagnosis tokens

This should be opt-in initially to avoid performance overhead on every `run` invocation. A future version might enable it by default if the overhead proves negligible.

### 3. COM activation prerequisite audit

This is the highest-value standalone COM feature. While lookup helpers answer "what is registered?", the audit answers the question users actually have: "my app failed with a COM error, what's wrong?"

Possible future command:

```text
loadwhat com audit <target-exe> <{CLSID}|ProgID>
```

This would answer a practical question:

> Could this target process plausibly activate this COM class on this machine?

Checks should include:

- target architecture versus server architecture
- which registry view the target would see (based on target bitness)
- whether the resolved COM server registration is machine-consistent
- whether the server binary exists and is loadable
- whether a `LocalServer32` command resolves to a real executable
- whether service-backed activation references a real `LocalService`
- whether the in-proc server has missing transitive DLL dependencies
- whether the target's manifest declares registration-free COM for this CLSID
- elevation requirements (if `RunAs` is specified in the `AppID`)

This extends `loadwhat`'s mission in a focused way: from "what DLL load broke first?" to "are the prerequisites for this COM activation even satisfiable?"

The lookup helpers (`com clsid`, `com progid`, `com server`) are building blocks for this command. They should be designed with `com audit` as the primary consumer.

### 4. Integration with `imports` command

When `imports` is analyzing a DLL that is itself a COM server, it may be useful to report its COM registrations as part of the output. This is a lightweight extension:

- after the normal import walk, check whether the target DLL has any COM registrations
- if so, emit `COM_REGISTRATION` tokens as supplementary context
- this helps users understand whether a DLL they're analyzing is a COM server and how it's registered

This should be opt-in (a `--com` flag on `imports`) to avoid adding overhead to the default path.

### 5. Limited runtime COM correlation

This is possible, but should be treated as advanced future work.

Loader-snaps can already help in some COM-related cases:

- a registered COM in-proc server DLL may fail to load because it is missing or has a broken dependency
- that failure may already surface as normal `DYNAMIC_MISSING` evidence during `run`

What loader-snaps generally does not tell us on its own:

- which CLSID the application tried to activate
- which HRESULT was returned by COM APIs
- whether the failure was registration-related versus policy-related

If runtime COM diagnosis is added later, it likely needs dedicated runtime signal beyond the current loader-snaps path, such as ETW-based COM activation tracing. That is a materially larger feature than standalone helpers.

## The "wrong version" problem

The desire is often:

> the wrong version is registered at X; the application wanted Y

This is only partially solvable.

What a future COM feature can determine with high confidence:

- which server is currently registered
- file version and product version of that server
- whether the file's architecture matches expectations
- which `TypeLib` version is registered
- whether registration points at an older or obviously incorrect location

What it usually cannot determine from registration alone:

- the exact version the application expected internally
- whether the application would have accepted a newer compatible server
- whether failure is due to interface contract drift rather than registration

## Manifest and registration-free COM

Registration-free COM is increasingly common in modern Windows applications. An application manifest can declare COM classes with `<comClass>` elements, bypassing the registry entirely.

This matters for `loadwhat` in two ways:

1. **Lookup fallback**: when `com clsid` finds nothing in the registry, the CLSID may be declared in a manifest. Reporting "not registered" when the class is actually available via manifest is misleading.

2. **Conflict detection**: when both a manifest declaration and a registry entry exist for the same CLSID, the manifest wins at runtime. Reporting only the registry entry would be incomplete.

### Implementation approach

Manifest parsing should be part of the lookup path from the start, not deferred to a later phase:

- when resolving a CLSID, check the registry first, then check the target application's manifest
- when both sources exist, report both and note which one Windows would use at runtime
- for `com audit`, compare the manifest declaration against the actual file on disk

Manifest parsing scope should be limited to:

- embedded manifests in the target PE (`RT_MANIFEST` resource)
- external `.manifest` files adjacent to the target
- `<comClass>` elements with `clsid`, `progid`, and `threadingModel` attributes
- `<file>` elements that identify the server DLL

Full SxS assembly resolution and publisher policy chains are out of scope for near-term work.

### Manifest-aware output

```text
COM_MANIFEST source="embedded" file="target.exe" clsid="{...}" progid="Vendor.Object" server="vendor.dll" threading_model="Both"
```

When manifest and registry conflict:

```text
COM_LOOKUP clsid="{...}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_MANIFEST source="embedded" file="target.exe" clsid="{...}" server="vendor_v2.dll"
NOTE topic="com" detail="manifest-overrides-registry" clsid="{...}" message="runtime activation will use manifest declaration"
```

## WOW64 and bitness considerations

COM bitness mismatches are among the most common real-world COM failures. Although `loadwhat` v1 is x64-only and does not support debugging WOW64 (32-bit) target processes, the COM feature can still provide valuable bitness-related diagnosis.

### What loadwhat can do without WOW64 debug support

- Read both 64-bit and 32-bit registry views using `KEY_WOW64_64KEY` / `KEY_WOW64_32KEY` flags
- Report when a CLSID is registered in one view but not the other
- Report when a 32-bit in-proc server is registered but the caller is 64-bit (or vice versa)
- Report when `InprocServer32` entries differ between 32-bit and 64-bit views

### Bitness-aware output

```text
COM_LOOKUP clsid="{...}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_LOOKUP clsid="{...}" status="not_registered" hive="HKLM" view="32"
COM_SERVER path="C:\Vendor\foo.dll" machine="x64"
NOTE topic="com" detail="bitness" message="registered in 64-bit view only; 32-bit callers will not find this class"
```

For `com audit`, the target's architecture determines which registry view to check, and the result should flag any mismatch:

```text
COM_AUDIT target="app.exe" target_machine="x64" clsid="{...}" server_machine="x86" status="BITNESS_MISMATCH"
```

## Recommended output philosophy

Any future COM public output should stay consistent with `loadwhat`'s token model:

```text
TOKEN key=value key=value ...
```

The tool should prefer factual lines over prescriptive fixes.

Good:

```text
COM_LOOKUP clsid="{...}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_SERVER path="C:\Program Files\Vendor\foo.dll" exists=true machine="x64" threading_model="Both"
COM_DEPENDENCY_STATUS status="missing" dll="bar.dll" via="foo.dll" depth=1
```

Risky unless strongly justified:

```text
COM_SUGGESTED_FIX message="run regsvr32 foo.dll"
```

`regsvr32` is only appropriate for some self-registering in-proc servers and is not a general COM repair action. `loadwhat` should bias toward reporting facts, not assuming the right remediation.

## Token contract evolution

V1 has a precise token contract. Adding COM features means new token families that must coexist cleanly with existing tokens.

### New token families

| Token | Purpose | Emitted by |
|-------|---------|------------|
| `COM_LOOKUP` | CLSID/ProgID resolution result | `com clsid`, `com progid`, `com audit` |
| `COM_SERVER` | Server binary validation | `com clsid`, `com progid`, `com server`, `com audit` |
| `COM_REGISTRATION` | Reverse lookup: registrations pointing to a server | `com server` |
| `COM_PROGID` | ProgID associated with a CLSID | `com server`, `com clsid` |
| `COM_MANIFEST` | Registration-free COM declaration | `com clsid`, `com audit` |
| `COM_CONTEXT` | COM context enrichment for DLL failures | `run` (with `--com`) |
| `COM_AUDIT` | Activation prerequisite check result | `com audit` |
| `COM_DEPENDENCY_STATUS` | Server dependency walk result | `com clsid`, `com server`, `com audit` |

### Interaction with existing tokens

- `COM_DEPENDENCY_STATUS` reuses the same recursive import walk as `STATIC_MISSING` / `STATIC_BAD_IMAGE`, but is scoped to a COM server rather than the target executable. It should be a distinct token to avoid confusion about what is being diagnosed.
- `COM_CONTEXT` appears *after* a `STATIC_MISSING` or `DYNAMIC_MISSING` line in `run` output. It annotates the existing diagnosis with COM context; it does not replace it.
- The `SUMMARY` token already has a `com_issues=0` placeholder field. When COM features are active, this field should count the number of COM-specific findings.

### Output ordering in verbose mode

For `run` with `--com`:

```text
RUN_START ...
RUNTIME_LOADED ...
...
RUN_END ...
STATIC_START ...
STATIC_MISSING dll="vendor_com.dll" via="target.exe" depth=1
COM_CONTEXT dll="vendor_com.dll" clsid="{...}" server_kind="InprocServer32"
STATIC_END ...
FIRST_BREAK ...
SUMMARY ... com_issues=1
```

For `com clsid`:

```text
COM_LOOKUP clsid="{...}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_SERVER path="C:\Vendor\foo.dll" exists=true machine="x64" threading_model="Both"
COM_DEPENDENCY_STATUS status="ok"
```

For `com audit`:

```text
COM_AUDIT target="app.exe" target_machine="x64" clsid="{...}" status="SERVER_DEPS_MISSING"
COM_LOOKUP clsid="{...}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_SERVER path="C:\Vendor\foo.dll" exists=true machine="x64" threading_model="Both"
COM_DEPENDENCY_STATUS status="missing" dll="bar.dll" via="foo.dll" depth=1
SEARCH_ORDER safedll=true
SEARCH_PATH dll="bar.dll" ...
```

### Summary mode behavior

Summary mode for COM commands should follow the same philosophy as v1: emit exactly one line for the primary finding.

- `com clsid` / `com progid`: emit `COM_LOOKUP` only (the resolution result)
- `com server`: emit `COM_SERVER` only (the validation result)
- `com audit`: emit `COM_AUDIT` only (the overall pass/fail verdict)
- `run --com`: existing summary behavior unchanged; `COM_CONTEXT` is suppressed in summary mode

## Example user stories

### Story 1: "My app fails with 'Class not registered'"

```console
> loadwhat com clsid "{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}"
COM_LOOKUP clsid="{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}" status="not_registered"
```

The CLSID simply is not in any registry hive. The user needs to install or register the component.

### Story 2: "COM server is registered but activation fails"

```console
> loadwhat com clsid "{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}"
COM_LOOKUP clsid="{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}" status="server_deps_missing" hive="HKLM" view="64" server_kind="InprocServer32"
COM_SERVER path="C:\Vendor\foo.dll" exists=true machine="x64" threading_model="Both"
COM_DEPENDENCY_STATUS status="missing" dll="bar.dll" via="foo.dll" depth=1
```

The COM server is registered and present, but it has a broken dependency. This is the most common "hidden" COM failure.

### Story 3: "32-bit app can't activate a 64-bit COM server"

```console
> loadwhat com audit my32bitapp.exe "{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}"
COM_AUDIT target="my32bitapp.exe" target_machine="x86" clsid="{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}" status="BITNESS_MISMATCH"
COM_LOOKUP clsid="{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}" status="registered" hive="HKLM" view="64" server_kind="InprocServer32"
COM_SERVER path="C:\Vendor\foo.dll" machine="x64"
NOTE topic="com" detail="bitness" message="server is x64 but target is x86; in-proc activation will fail"
```

### Story 4: "`run` discovers a COM-related DLL failure"

```console
> loadwhat run --com myapp.exe
STATIC_MISSING dll="vendor_ocx.dll" via="myapp.exe" depth=1
COM_CONTEXT dll="vendor_ocx.dll" clsid="{DEADBEEF-...}" progid="Vendor.Control" server_kind="InprocServer32"
```

The user now knows that `vendor_ocx.dll` wasn't just a random import — it was a COM in-proc server. That context guides them toward re-registering the component rather than just copying the DLL.

## Testability strategy

COM features interact with machine-global state (the Windows registry) in ways that v1's DLL-focused tests do not. The existing test suite uses MSVC-built fixture EXEs and DLLs in isolated directories. COM testing requires fixture *registrations* — synthetic registry entries that point to fixture DLLs — and those entries must not leak into or depend on the host machine's real COM state.

The goal is to maximize test coverage that runs locally without containers, without Docker, and without touching the host registry — while still having a container-based tier for full end-to-end validation.

### Design principle: injectable platform interfaces

V1 already has a precedent for test seams. `safe_dll_search_mode()` in `win.rs` checks an environment variable (`LOADWHAT_TEST_SAFE_DLL_SEARCH_MODE`) in debug builds before falling through to the real `RegQueryValueExW` call. The COM feature should generalize this pattern into a proper trait-based abstraction so that the vast majority of COM logic can be tested without any registry access at all.

The key insight is that COM diagnosis has two distinct layers:

1. **Data acquisition:** reading registry keys, checking file existence, parsing PE headers, reading manifests
2. **Diagnostic logic:** resolving ProgID chains, detecting TreatAs loops, classifying errors, merging HKCU/HKLM results, selecting the right registry view, ranking findings

Layer 1 is platform-coupled. Layer 2 is pure logic. If the boundary between them is clean, layer 2 can be exhaustively tested with mock data and zero host interaction.

### The registry trait

The COM registry subsystem should be built around an injectable interface:

```rust
/// Represents the result of reading a single registry value.
pub enum RegValue {
    String(String),
    Dword(u32),
    Binary(Vec<u8>),
    NotFound,
    AccessDenied,
    Error(u32),
}

/// Represents a registry hive + view combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegLocation {
    Hklm64,
    Hklm32,
    Hkcu64,
    Hkcu32,
}

/// Abstraction over registry reads. Production code uses the real
/// Windows registry. Tests inject a mock with controlled data.
pub trait ComRegistry {
    /// Read a string or DWORD value from a specific hive/view.
    fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue;

    /// Check whether a subkey exists (without reading a value).
    fn key_exists(&self, location: RegLocation, subkey: &str) -> bool;

    /// Enumerate subkey names under a given key.
    fn enum_subkeys(&self, location: RegLocation, subkey: &str) -> Result<Vec<String>, u32>;
}
```

Production implementation wraps the real FFI calls (`RegOpenKeyExW`, `RegQueryValueExW`, `RegEnumKeyExW`) with appropriate `KEY_WOW64_64KEY` / `KEY_WOW64_32KEY` flags based on the `RegLocation`.

#### Why a trait, not just env-var overrides

The v1 env-var pattern works for single boolean toggles like `SafeDllSearchMode`. COM resolution requires complex multi-key traversal: ProgID -> CurVer -> CLSID -> TreatAs -> InprocServer32, with HKCU/HKLM precedence at each step. Encoding all of that in environment variables would be fragile and unreadable. A trait gives tests full control over exactly what data each registry path returns, including partial failures, access denied on specific keys, and missing intermediate links in a chain.

#### Mock implementation

```rust
/// In-memory registry mock for testing. Keys are stored as
/// (RegLocation, subkey, value_name) -> RegValue.
pub struct MockRegistry {
    values: HashMap<(RegLocation, String, String), RegValue>,
    subkeys: HashMap<(RegLocation, String), Vec<String>>,
}

impl MockRegistry {
    pub fn new() -> Self { /* ... */ }

    /// Insert a value. subkey should use backslash separators.
    pub fn set(&mut self, loc: RegLocation, subkey: &str, name: &str, value: RegValue) {
        // Also auto-populate parent subkey enumerations
        /* ... */
    }

    /// Mark a specific key as access-denied (all reads under it fail).
    pub fn deny_access(&mut self, loc: RegLocation, subkey: &str) { /* ... */ }
}

impl ComRegistry for MockRegistry {
    fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue {
        self.values
            .get(&(location, subkey.to_string(), name.to_string()))
            .cloned()
            .unwrap_or(RegValue::NotFound)
    }
    // ...
}
```

This mock is purely in-memory. It touches nothing on the host. Tests construct the exact registry state they need:

```rust
#[test]
fn progid_resolves_through_curver_to_clsid() {
    let mut reg = MockRegistry::new();

    // ProgID -> CurVer
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\Vendor.Widget",
        "",
        RegValue::String("Vendor.Widget".into()));
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\Vendor.Widget\CurVer",
        "",
        RegValue::String("Vendor.Widget.3".into()));

    // Versioned ProgID -> CLSID
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\Vendor.Widget.3\CLSID",
        "",
        RegValue::String("{AAAA-BBBB-CCCC-DDDD}".into()));

    // CLSID -> InprocServer32
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{AAAA-BBBB-CCCC-DDDD}\InprocServer32",
        "",
        RegValue::String(r"C:\Vendor\widget.dll".into()));
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{AAAA-BBBB-CCCC-DDDD}\InprocServer32",
        "ThreadingModel",
        RegValue::String("Both".into()));

    let resolver = ComResolver::new(&reg, &real_fs);
    let result = resolver.resolve_progid("Vendor.Widget");

    assert_eq!(result.clsid, Some("{AAAA-BBBB-CCCC-DDDD}".into()));
    assert_eq!(result.hive, Some(RegLocation::Hklm64));
    assert_eq!(result.server_kind, Some(ServerKind::InprocServer32));
    assert_eq!(result.server_path, Some(r"C:\Vendor\widget.dll".into()));
    assert_eq!(result.threading_model, Some("Both".into()));
}
```

### The file-system trait

COM diagnosis also checks whether server files exist, and feeds them into the PE parser. This should also be injectable:

```rust
/// Abstraction over file-system checks needed by COM diagnosis.
pub trait ComFileSystem {
    /// Check whether a file exists at the given path.
    fn file_exists(&self, path: &str) -> bool;

    /// Read the file contents (or enough of the header) for PE validation.
    /// Returns None if the file does not exist or is unreadable.
    fn read_file_header(&self, path: &str, max_bytes: usize) -> Option<Vec<u8>>;
}
```

The production implementation calls `std::fs::metadata` and `std::fs::read`. The mock implementation uses an in-memory map:

```rust
pub struct MockFileSystem {
    files: HashMap<String, Vec<u8>>,
}

impl MockFileSystem {
    pub fn new() -> Self { /* ... */ }

    /// Add a file. Use pe_builder to create synthetic PE content.
    pub fn add_pe(&mut self, path: &str, imports: &[&str]) {
        self.files.insert(
            path.to_lowercase(),
            pe_builder::build_import_test_pe(imports),
        );
    }

    /// Add a non-PE file (for bad-image testing).
    pub fn add_raw(&mut self, path: &str, content: Vec<u8>) {
        self.files.insert(path.to_lowercase(), content);
    }
}
```

This means tests can set up a complete COM scenario — registry entries pointing to fixture DLLs with controlled import tables — entirely in memory:

```rust
#[test]
fn com_server_with_missing_transitive_dependency() {
    let mut reg = MockRegistry::new();
    let mut fs = MockFileSystem::new();

    // Register a COM server
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{TEST-0001}\InprocServer32",
        "",
        RegValue::String(r"C:\Vendor\server.dll".into()));

    // server.dll exists and imports helper.dll
    fs.add_pe(r"C:\Vendor\server.dll", &["helper.dll"]);
    // helper.dll does NOT exist -> missing transitive dep

    let resolver = ComResolver::new(&reg, &fs);
    let result = resolver.resolve_clsid("{TEST-0001}");

    assert_eq!(result.status, ComStatus::ServerDepsMissing);
    assert_eq!(result.missing_dep, Some("helper.dll".into()));
}
```

### The manifest reader

Manifest parsing (for registration-free COM) is inherently file-based, not registry-based. It reads XML from embedded PE resources or adjacent `.manifest` files. This is already compatible with the `ComFileSystem` trait — the manifest reader takes file content as input and returns parsed `<comClass>` declarations:

```rust
pub struct ManifestComClass {
    pub clsid: String,
    pub progid: Option<String>,
    pub threading_model: Option<String>,
    pub server_dll: Option<String>,  // from the parent <file> element
}

/// Parse COM class declarations from manifest XML content.
pub fn parse_manifest_com_classes(xml: &str) -> Vec<ManifestComClass> {
    // Minimal XML parsing — no need for a full XML library.
    // Manifests have a well-defined schema.
    /* ... */
}
```

Tests can call `parse_manifest_com_classes` directly with string literals. No files, no registry, no host interaction.

### The resolver: pure logic on top of injectable interfaces

The central COM resolution engine takes `&dyn ComRegistry` and `&dyn ComFileSystem` and performs all the logic:

```rust
pub struct ComResolver<'a> {
    registry: &'a dyn ComRegistry,
    fs: &'a dyn ComFileSystem,
}

impl<'a> ComResolver<'a> {
    pub fn resolve_clsid(&self, clsid: &str) -> ComLookupResult { /* ... */ }
    pub fn resolve_progid(&self, progid: &str) -> ComLookupResult { /* ... */ }
    pub fn validate_server(&self, path: &str) -> ComServerResult { /* ... */ }
    pub fn audit(&self, target: &PeInfo, clsid_or_progid: &str) -> ComAuditResult { /* ... */ }
}
```

Because `ComResolver` only calls trait methods, every code path in the resolver is testable with mocks. This includes:

- ProgID -> CurVer -> CLSID chain resolution (any depth)
- TreatAs redirection with loop detection
- HKCU vs HKLM precedence (mock returns different values for each)
- 64-bit vs 32-bit view selection
- Server file existence and PE validation
- Transitive dependency walking of COM server imports
- Error classification into the taxonomy statuses
- Access-denied on specific keys
- Manifest fallback when registry has no entry
- Manifest-vs-registry conflict detection
- AppID indirection (LocalService, DllSurrogate)
- LocalServer32 command-line parsing

### What mock tests cover (Tier 1: no host interaction)

These tests run with `cargo test` on any machine, in any CI environment, with no special privileges and no cleanup:

| Test category | What it validates | Mock setup |
|---------------|-------------------|------------|
| **ProgID resolution** | CurVer chain traversal, versioned ProgID -> CLSID | MockRegistry with ProgID, CurVer, CLSID keys |
| **TreatAs handling** | Normal redirect, cyclic redirect (loop detection), missing target | MockRegistry with TreatAs entries forming chains/cycles |
| **HKCU/HKLM merge precedence** | HKCU wins over HKLM; HKLM used when HKCU absent | MockRegistry with different values in Hkcu64 vs Hklm64 |
| **32/64 view selection** | Correct view chosen for caller bitness; cross-view reporting | MockRegistry with entries in Hklm64 but not Hklm32 (or vice versa) |
| **Server kind classification** | InprocServer32 vs LocalServer32 vs LocalService routing | MockRegistry with different server subkeys |
| **LocalServer32 command parsing** | Quoted paths, arguments, environment variables | MockRegistry with various command-line formats |
| **File existence checks** | Server exists, missing, access denied | MockFileSystem with/without the path |
| **PE validation** | Valid PE, wrong architecture, corrupt file, non-PE file | MockFileSystem with pe_builder output or raw bytes |
| **Transitive dependency walk** | Server imports DLL A which imports DLL B which is missing | MockFileSystem with chained pe_builder PEs |
| **Error classification** | Each taxonomy status is reachable and correctly assigned | Various mock setups targeting each status code |
| **Access-denied handling** | Graceful degradation when specific keys are unreadable | MockRegistry with deny_access on specific subkeys |
| **Manifest parsing** | comClass extraction, progid, threadingModel, file association | Direct string input to parse_manifest_com_classes |
| **Manifest fallback** | Registry miss -> manifest hit -> correct result | MockRegistry empty + MockFileSystem with manifest content |
| **Manifest/registry conflict** | Both sources present -> both reported, manifest-wins noted | MockRegistry populated + MockFileSystem with manifest |
| **AppID indirection** | DllSurrogate path validation, LocalService name lookup | MockRegistry with AppID keys |
| **Threading model reporting** | Correct value extracted and reported | MockRegistry with ThreadingModel values |
| **TypeLib reference** | Version and path extracted from TypeLib registration | MockRegistry with TypeLib keys |
| **Token emission** | Correct token format for every output path | Run resolver, capture emitted tokens as strings |
| **Summary vs trace vs verbose** | Mode-specific suppression/inclusion of tokens | Run full output pipeline in each mode |
| **Edge cases** | Empty CLSID, malformed GUID, non-ASCII ProgID, very long paths | MockRegistry/MockFileSystem with edge-case data |

This tier should cover **80–90% of all COM diagnostic code paths**. The resolver logic, chain traversal, error classification, merge rules, conflict detection, and token emission are all exercised without touching the host.

### What needs real platform interaction (Tier 2: container tests)

Some behaviors cannot be fully validated with mocks because they depend on Windows kernel or registry semantics that are difficult to simulate accurately:

| Test category | Why mocks are insufficient | Container requirement |
|---------------|---------------------------|---------------------|
| **Real HKCR merge behavior** | HKCR is a virtual key constructed by the kernel; mock doesn't replicate kernel merge timing or caching | Populate both HKCU and HKLM, read via HKCR, verify merge |
| **Real 32/64 registry views** | `KEY_WOW64_32KEY` / `KEY_WOW64_64KEY` redirect to different physical registry paths; mock can't test the FFI flag handling | Write to both views, read back with each flag, verify isolation |
| **RegOpenKeyExW error codes** | Real error codes for access-denied vs not-found vs other failures may differ from mock assumptions | Set ACLs on test keys, verify exact error code propagation |
| **PE loading of COM servers** | Mock PE headers test parsing but not whether Windows actually considers the file loadable | Place real fixture DLLs in container, run loadwhat, verify output |
| **Loader-snaps + COM interaction** | When a COM in-proc server's dependency is missing, does the loader-snaps output match expected patterns? | Register a COM DLL with missing deps, run a host that activates it, capture output |
| **IFEO registry path** | Loader-snaps IFEO fallback writes to real HKLM; must not leak to host | Container-local HKLM is safe to write |
| **End-to-end CLI output** | Full pipeline from CLI args through registry reads through PE parsing through token emission | Run loadwhat binary against fixture registrations in container |
| **Elevation scenarios** | Non-admin vs admin registry access differences | Run as restricted user inside container |

### Container infrastructure design

#### Docker setup

Windows Server Core containers provide full registry isolation, Win32 API support, and debug API access:

```dockerfile
FROM mcr.microsoft.com/windows/servercore:ltsc2022

# Copy loadwhat and fixtures
COPY target/release/loadwhat.exe C:/tools/
COPY target/loadwhat-tests/fixtures/ C:/fixtures/

# Copy test scripts and the test runner
COPY tests/com/container/ C:/tests/

# Default entrypoint runs all container-tier tests
ENTRYPOINT ["powershell", "-File", "C:/tests/run_container_tests.ps1"]
```

#### Registry fixture script

A PowerShell script creates all fixture registrations before tests run:

```powershell
# tests/com/container/setup_fixtures.ps1

# Unique CLSIDs for each test scenario - prefixed to avoid collisions
$TestPrefix = "DEADBEEF-TEST"

# Scenario: basic CLSID lookup (InprocServer32)
$clsid = "{$TestPrefix-0001-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" `
    -Name "ThreadingModel" -Value "Both"

# Scenario: ProgID -> CurVer -> CLSID chain
New-Item -Path "HKLM:\Software\Classes\LwTest.Widget" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\LwTest.Widget" `
    -Name "(Default)" -Value "LwTest.Widget"
New-Item -Path "HKLM:\Software\Classes\LwTest.Widget\CurVer" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\LwTest.Widget\CurVer" `
    -Name "(Default)" -Value "LwTest.Widget.2"
# ... (versioned ProgID -> CLSID -> InprocServer32)

# Scenario: 32-bit vs 64-bit view mismatch
# Write to 64-bit view (default on x64)
$clsid_bitness = "{$TestPrefix-0003-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_bitness\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_bitness\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
# Write to 32-bit view using reg.exe (PowerShell doesn't natively support WOW6432Node targeting)
reg add "HKLM\Software\Classes\WOW6432Node\CLSID\$clsid_bitness\InprocServer32" `
    /ve /d "C:\fixtures\lwtest_a_x86.dll" /f

# Scenario: HKCU overrides HKLM
$clsid_override = "{$TestPrefix-0004-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_override\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_override\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a_v1.dll"
New-Item -Path "HKCU:\Software\Classes\CLSID\$clsid_override\InprocServer32" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$clsid_override\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a_v2.dll"

# Scenario: missing server file
$clsid_missing = "{$TestPrefix-0005-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_missing\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_missing\InprocServer32" `
    -Name "(Default)" -Value "C:\nonexistent\path\missing.dll"

# Scenario: server with broken imports (uses fixture DLL that imports nonexistent dep)
$clsid_broken = "{$TestPrefix-0006-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_broken\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_broken\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_missing_dep.dll"

# Scenario: LocalServer32 with command-line arguments
$clsid_local = "{$TestPrefix-0007-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_local\LocalServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_local\LocalServer32" `
    -Name "(Default)" -Value '"C:\fixtures\lwtest_server.exe" /Embedding'

# Scenario: TreatAs redirection (2-hop)
$clsid_old = "{$TestPrefix-0008-0000-0000-000000000001}"
$clsid_new = "{$TestPrefix-0008-0000-0000-000000000002}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_old\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_old\TreatAs" `
    -Name "(Default)" -Value $clsid_new
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_new\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_new\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"

# Scenario: TreatAs cycle (for loop detection)
$clsid_cycle_a = "{$TestPrefix-0009-0000-0000-000000000001}"
$clsid_cycle_b = "{$TestPrefix-0009-0000-0000-000000000002}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_a\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_a\TreatAs" `
    -Name "(Default)" -Value $clsid_cycle_b
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_b\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_b\TreatAs" `
    -Name "(Default)" -Value $clsid_cycle_a

# Scenario: access-denied (create key, then restrict ACL)
$clsid_denied = "{$TestPrefix-000A-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_denied\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_denied\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
# Restrict read access for non-admin users
$acl = Get-Acl "HKLM:\Software\Classes\CLSID\$clsid_denied"
$rule = New-Object System.Security.AccessControl.RegistryAccessRule(
    "BUILTIN\Users", "ReadKey", "Deny")
$acl.AddAccessRule($rule)
Set-Acl -Path "HKLM:\Software\Classes\CLSID\$clsid_denied" -AclObject $acl
```

#### Container test runner

```powershell
# tests/com/container/run_container_tests.ps1

# Setup fixtures
& C:\tests\setup_fixtures.ps1

$failures = @()

# Test: basic CLSID lookup
$output = & C:\tools\loadwhat.exe com clsid "{DEADBEEF-TEST-0001-0000-0000-000000000001}" 2>&1
if ($output -notmatch 'COM_LOOKUP.*status="registered"') {
    $failures += "basic_clsid_lookup"
}

# Test: ProgID chain resolution
$output = & C:\tools\loadwhat.exe com progid "LwTest.Widget" 2>&1
if ($output -notmatch 'COM_LOOKUP.*clsid=') {
    $failures += "progid_chain"
}

# Test: missing server file
$output = & C:\tools\loadwhat.exe com clsid "{DEADBEEF-TEST-0005-0000-0000-000000000001}" 2>&1
if ($output -notmatch 'status="server_missing"') {
    $failures += "missing_server"
}

# ... additional tests ...

# Report results
if ($failures.Count -eq 0) {
    Write-Host "All container tests passed."
    exit 0
} else {
    Write-Host "FAILED: $($failures -join ', ')"
    exit 1
}
```

#### Feasibility notes for containers

**What works well:**

- Registry isolation is real. Each Windows container has its own `HKLM` and `HKCU`. Tests can create, modify, and destroy COM registrations freely without affecting the host or other tests.
- PE execution works. The Windows kernel is shared (in process-isolation mode), so `CreateProcessW`, `WaitForDebugEvent`, `ReadProcessMemory`, and other Win32 debug APIs function normally.
- Loader-snaps should work. `NtGlobalFlag` manipulation targets the PEB of the debugged process, which is a per-process structure. The kernel is shared, so ntdll behavior is identical to the host.
- Fixture DLLs can be copied into the container image or mounted as volumes.
- IFEO registry writes are container-local, so the loader-snaps fallback path can be tested safely.

**What requires caution:**

- **Image size.** Windows Server Core images are 5+ GB. `mcr.microsoft.com/windows/servercore:ltsc2022` is typical. CI runners need to cache this aggressively or build times will be painful.
- **Host OS version coupling.** Process-isolated Windows containers require the container OS version to match the host OS version (or be within the same servicing window). This limits portability across different Windows 10/11 builds. Hyper-V isolation removes this constraint but adds overhead.
- **CI support.** GitHub Actions Windows runners support Windows containers but with limitations. Self-hosted runners are more reliable for this. Azure DevOps has better native support.
- **Debug API permissions.** `SeDebugPrivilege` may need to be granted inside the container. Process-isolation mode generally inherits kernel-level debug capabilities, but this should be validated early.

**What does not work:**

- **Nano Server images** are smaller but do not include the Win32 subsystem, COM runtime, or debug APIs. They are not usable for `loadwhat` testing.
- **Linux containers** are obviously not an option for Win32 API testing.

### Hybrid testing strategy (recommended)

Use three tiers, each building on the one below:

#### Tier 1: mock-based unit and integration tests

- **Runs with:** `cargo test` (no special flags, no container, no elevation)
- **Scope:** all COM resolution logic, chain traversal, error classification, merge rules, token emission, manifest parsing, command-line parsing, edge cases
- **Host interaction:** none — purely in-memory mocks
- **Speed:** milliseconds per test
- **Coverage target:** 80–90% of COM code paths
- **Gated by:** `#[cfg(test)]` — always runs

This tier is the workhorse. Every new COM logic path should have mock tests before anything else. If a bug is found in a container test, the fix should include a mock test that reproduces the issue without containers, so it can never regress silently.

#### Tier 2: fixture-backed integration tests (existing harness, extended)

- **Runs with:** `cargo xtask test` (builds MSVC fixtures, sets harness env vars)
- **Scope:** PE parsing of real fixture DLLs, import walk over real files, basic `com server` validation against fixture DLLs on disk
- **Host interaction:** reads fixture files from `target/loadwhat-tests/fixtures/`; no registry writes
- **Speed:** seconds (fixture build + test execution)
- **Coverage target:** PE-level COM server validation, end-to-end `com server <path>` command
- **Gated by:** `#[cfg(feature = "harness-tests")]` — same as existing v1 integration tests

This tier extends the existing test harness. New fixture DLLs (e.g., a minimal DLL with a known import table that acts as a COM server) can be added to the MSBuild fixture project. The `pe_builder` can also generate synthetic COM server DLLs programmatically.

What this tier intentionally does NOT do: create registry entries. All fixture-backed tests that need registry data should either use mock injection or be promoted to Tier 3.

#### Tier 3: container-based system tests

- **Runs with:** `cargo xtask test-container` (builds Docker image, runs tests inside container)
- **Scope:** real registry reads with fixture registrations, real HKCR merge, real 32/64 views, access-denied scenarios, loader-snaps + COM interaction, full end-to-end CLI
- **Host interaction:** none — everything runs inside the disposable container
- **Speed:** minutes (container build + startup + test execution)
- **Coverage target:** FFI correctness, kernel-level registry semantics, real-world integration
- **Gated by:** `#[cfg(feature = "container-tests")]` or run externally via Docker

This tier validates the things mocks cannot: that the real `RegOpenKeyExW` with `KEY_WOW64_32KEY` actually reads from the 32-bit view, that HKCR merge behaves as expected, that access-denied returns the right error code.

Container tests should be runnable independently of Tier 1 and Tier 2. They test the same code paths but through the real platform, so failures here indicate FFI bugs or incorrect assumptions about Windows behavior — not logic errors (which Tier 1 catches).

### CI pipeline integration

```
┌────────────────────────────┐
│  cargo test                │  Tier 1: mock tests (always, every PR)
│  (mock-based, fast)        │  ~seconds
└────────────┬───────────────┘
             │ pass
             v
┌────────────────────────────┐
│  cargo xtask test          │  Tier 2: fixture-backed (always, every PR)
│  (MSVC fixtures, no reg)   │  ~minutes
└────────────┬───────────────┘
             │ pass
             v
┌────────────────────────────┐
│  cargo xtask test-container│  Tier 3: container tests (nightly / pre-release)
│  (Docker, real registry)   │  ~5-10 minutes
└────────────────────────────┘
```

- **Every PR:** Tier 1 + Tier 2. Fast, no special infrastructure.
- **Nightly / pre-release:** Tier 1 + Tier 2 + Tier 3. Full validation including container tests.
- **Local dev:** `cargo test` for fast iteration. `cargo xtask test-container` when working on registry FFI.

Container tests run as a separate CI job that can be skipped for quick iteration or when Docker is unavailable. Tier 1 and Tier 2 failures block merges. Tier 3 failures block releases but not day-to-day development.

### Guidelines for where to put a new test

When adding a test for new COM behavior, use this decision tree:

1. **Does the test need real Windows registry behavior?** (HKCR merge, 32/64 view flags, ACL enforcement, real error codes)
   - Yes -> Tier 3 (container). Also write a Tier 1 mock test for the logic path.
   - No -> continue to 2.

2. **Does the test need real PE files on disk?** (actual DLL loading, loader-snaps interaction, debug APIs)
   - Yes -> Tier 2 (fixture-backed). Also write a Tier 1 mock test for the parsing/logic.
   - No -> continue to 3.

3. **Everything else** -> Tier 1 (mock-based). This is the default.

The goal is for every Tier 2 or Tier 3 test to have a corresponding Tier 1 test that covers the same logic path with mocks. Container tests validate platform integration. Mock tests prevent regressions and enable fast iteration.

### Test fixture design for COM

COM test fixtures should follow the same principles as v1 fixtures:

- **Deterministic:** every test creates exactly the state it needs, no dependence on ambient machine state
- **Isolated:** tests do not interfere with each other (use unique CLSIDs per test)
- **Self-contained:** fixture DLLs are minimal PEs built by the existing `pe_builder` or by MSBuild fixture projects
- **Documented:** each fixture registration documents what it's testing and why

Recommended fixture CLSID scheme: use a test-specific prefix like `{DEADBEEF-TEST-xxxx-xxxx-xxxxxxxxxxxx}` to avoid collisions with real COM classes. Each test scenario gets a unique 4th group (e.g., `0001` for basic lookup, `0002` for ProgID chains, etc.).

### Extending pe_builder for COM fixtures

The existing `pe_builder::build_import_test_pe` creates minimal x64 PEs with import tables. For COM testing, it may need extensions:

- **Machine type control:** build x86 PEs (change the machine field from `0x8664` to `0x014C`) for bitness mismatch tests
- **Manifest resource embedding:** add an `RT_MANIFEST` resource with `<comClass>` declarations for registration-free COM tests
- **DLL flag:** set the `IMAGE_FILE_DLL` characteristic so the PE is recognized as a DLL, not an EXE

These are small changes to the existing builder. The manifest embedding is the most complex (it requires adding a resource section to the PE), but a minimal implementation that just appends a resource directory with a single manifest entry is feasible.

## Proposed roadmap

### Phase 1: COM registry subsystem and lookup helpers

Focus:

- COM registry reader with HKCU/HKLM merge, 32/64 view, ProgID chains, TreatAs, AppID
- `com clsid` and `com progid` commands
- `com server <path>` reverse-lookup command
- manifest parsing for registration-free COM as a lookup fallback
- registry-source reporting
- server-kind reporting
- file existence and PE validation
- architecture checks
- import/dependency diagnosis for in-proc servers
- error taxonomy implementation (all statuses from the taxonomy table)
- registry access-denied handling and elevation awareness
- injectable registry interface for unit testing
- Windows container test infrastructure for system tests

Why first:

- the registry subsystem is the engineering foundation for everything else
- lookup helpers are immediately useful on their own
- building the test infrastructure early prevents regression as features grow
- manifest parsing in the lookup path catches registration-free COM from day one

### Phase 2: `run` integration and COM activation audit

Focus:

- `com audit` command: evaluate whether a target can activate a COM class
- `--com` flag on `run`: enrich DLL failure output with COM context
- target/view/bitness-aware reasoning
- `LocalServer32`, `LocalService`, and surrogate-aware validation
- `COM_CONTEXT` token emission in `run`
- WOW64 registry view cross-checking (read both views, report mismatches)

Why next:

- `com audit` is the highest-value user-facing feature
- `run` integration connects COM diagnosis to the existing workflow
- the registry subsystem from Phase 1 makes this mostly a composition exercise

### Phase 3: runtime COM tracing

Focus:

- dedicated COM runtime observation if a trustworthy signal source is chosen

Likely requirements:

- ETW or similarly direct runtime evidence
- explicit separation between observed facts and heuristics
- careful token-contract design

Why last:

- highest complexity
- least aligned with `loadwhat`'s current architecture
- easiest area to over-promise and under-deliver

## Feasibility summary

| Feature | Feasible? | Effort | Value | Notes |
|---------|-----------|--------|-------|-------|
| COM registry subsystem (HKCU/HKLM merge, views) | Yes | High | Critical | Foundation for all COM features; main engineering risk |
| `com clsid` / `com progid` helpers | Yes | Medium | High | Best fit for current architecture |
| `com server <path>` validation | Yes | Low-Medium | High | Reuses existing PE/import logic |
| Manifest parsing (registration-free COM) | Yes | Medium | High | Increasingly common; should be in Phase 1 |
| COM activation prerequisite audit | Yes | Medium-High | High | Strongest user-facing feature |
| `run --com` integration | Yes | Medium | High | Connects COM to existing workflow |
| `imports --com` integration | Yes | Low | Medium | Lightweight extension |
| Bitness cross-checking (read both views) | Yes | Low | High | Common failure mode; no WOW64 debug needed |
| Registry access-denied handling | Yes | Low | Medium | Important for non-admin users |
| Windows container test infrastructure | Yes | Medium | High | Enables reliable COM testing |
| Runtime CLSID capture during `run` | Complex | High | Medium | Likely requires ETW or new tracing |
| Exact "you need version X" diagnosis | Limited | Medium | Medium | Only possible in specific cases |

## Conclusion

The strongest COM direction for `loadwhat` is not "debug all COM failures at runtime."

It is:

- deterministic COM registration lookup with correct HKCU/HKLM merge and 32/64 view semantics
- server validation including reverse-lookup from server path to registrations
- dependency diagnosis of COM servers reusing the existing import walk
- manifest-aware lookup that catches registration-free COM
- bitness- and registry-view-aware activation prerequisite auditing
- integration with `run` to explain *why* a missing DLL was being loaded
- a clear error taxonomy that distinguishes between "not registered," "registered but broken," and "cannot determine"

That extends the mission cleanly. It keeps `loadwhat` centered on first-order loadability and machine configuration facts, which is where the tool can be both truthful and useful.

This document is the design rationale. When implementation begins, precise behavioral contracts (exact CLI syntax, token fields, phase triggering conditions, algorithm descriptions) should be written into `loadwhat_spec_v2.md`.
