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

## Design principle: injectable platform interfaces

V1 already has a precedent for test seams. `safe_dll_search_mode()` in `win.rs` checks an environment variable (`LOADWHAT_TEST_SAFE_DLL_SEARCH_MODE`) in debug builds before falling through to the real `RegQueryValueExW` call. The COM feature should generalize this pattern into a proper trait-based abstraction so that the vast majority of COM logic can be tested without any registry access at all.

The key insight is that COM diagnosis has two distinct layers:

1. **Data acquisition:** reading registry keys, checking file existence, parsing PE headers, reading manifests
2. **Diagnostic logic:** resolving ProgID chains, detecting TreatAs loops, classifying errors, merging HKCU/HKLM results, selecting the right registry view, ranking findings

Layer 1 is platform-coupled. Layer 2 is pure logic. If the boundary between them is clean, layer 2 can be exhaustively tested with mock data and zero host interaction.

### Why a trait, not just env-var overrides

The v1 env-var pattern works for single boolean toggles like `SafeDllSearchMode`. COM resolution requires complex multi-key traversal: ProgID -> CurVer -> CLSID -> TreatAs -> InprocServer32, with HKCU/HKLM precedence at each step. Encoding all of that in environment variables would be fragile and unreadable. A trait gives tests full control over exactly what data each registry path returns, including partial failures, access denied on specific keys, and missing intermediate links in a chain.

## Injectable interfaces

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

The production implementation wraps the real FFI calls (`RegOpenKeyExW`, `RegQueryValueExW`, `RegEnumKeyExW`) with appropriate `KEY_WOW64_64KEY` / `KEY_WOW64_32KEY` flags based on the `RegLocation`.

### Mock registry implementation

```rust
/// In-memory registry mock for testing. Keys are stored as
/// (RegLocation, subkey, value_name) -> RegValue.
pub struct MockRegistry {
    values: HashMap<(RegLocation, String, String), RegValue>,
    subkeys: HashMap<(RegLocation, String), Vec<String>>,
    denied: HashSet<(RegLocation, String)>,
}

impl MockRegistry {
    pub fn new() -> Self {
        MockRegistry {
            values: HashMap::new(),
            subkeys: HashMap::new(),
            denied: HashSet::new(),
        }
    }

    /// Insert a value. subkey should use backslash separators.
    pub fn set(&mut self, loc: RegLocation, subkey: &str, name: &str, value: RegValue) {
        self.values.insert(
            (loc, subkey.to_string(), name.to_string()),
            value,
        );
        // Auto-populate parent subkey enumerations so key_exists
        // and enum_subkeys work without separate setup.
        if let Some((parent, child)) = subkey.rsplit_once('\\') {
            self.subkeys
                .entry((loc, parent.to_string()))
                .or_default()
                .push(child.to_string());
        }
    }

    /// Mark a specific key as access-denied (all reads under it fail).
    pub fn deny_access(&mut self, loc: RegLocation, subkey: &str) {
        self.denied.insert((loc, subkey.to_string()));
    }
}

impl ComRegistry for MockRegistry {
    fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue {
        // Check for access-denied on this key or any parent.
        if self.denied.contains(&(location, subkey.to_string())) {
            return RegValue::AccessDenied;
        }
        self.values
            .get(&(location, subkey.to_string(), name.to_string()))
            .cloned()
            .unwrap_or(RegValue::NotFound)
    }

    fn key_exists(&self, location: RegLocation, subkey: &str) -> bool {
        if self.denied.contains(&(location, subkey.to_string())) {
            return false; // treat denied as not visible
        }
        self.values.keys().any(|(l, k, _)| *l == location && k == subkey)
            || self.subkeys.contains_key(&(location, subkey.to_string()))
    }

    fn enum_subkeys(&self, location: RegLocation, subkey: &str) -> Result<Vec<String>, u32> {
        if self.denied.contains(&(location, subkey.to_string())) {
            return Err(5); // ERROR_ACCESS_DENIED
        }
        Ok(self.subkeys
            .get(&(location, subkey.to_string()))
            .cloned()
            .unwrap_or_default())
    }
}
```

This mock is purely in-memory. It touches nothing on the host.

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
    pub fn new() -> Self {
        MockFileSystem { files: HashMap::new() }
    }

    /// Add a file with synthetic PE content. Uses pe_builder to create
    /// a minimal PE with the given import table.
    pub fn add_pe(&mut self, path: &str, imports: &[&str]) {
        self.files.insert(
            path.to_lowercase(),
            pe_builder::build_import_test_pe(imports),
        );
    }

    /// Add a file with an explicit machine type for bitness testing.
    pub fn add_pe_with_machine(&mut self, path: &str, machine: u16, imports: &[&str]) {
        let mut pe = pe_builder::build_import_test_pe(imports);
        // Patch the Machine field at PE_OFFSET + 4
        let offset = 0x80 + 4; // PE_OFFSET + 4
        pe[offset..offset + 2].copy_from_slice(&machine.to_le_bytes());
        self.files.insert(path.to_lowercase(), pe);
    }

    /// Add a non-PE file (for bad-image testing).
    pub fn add_raw(&mut self, path: &str, content: Vec<u8>) {
        self.files.insert(path.to_lowercase(), content);
    }
}

impl ComFileSystem for MockFileSystem {
    fn file_exists(&self, path: &str) -> bool {
        self.files.contains_key(&path.to_lowercase())
    }

    fn read_file_header(&self, path: &str, max_bytes: usize) -> Option<Vec<u8>> {
        self.files.get(&path.to_lowercase()).map(|data| {
            data[..data.len().min(max_bytes)].to_vec()
        })
    }
}
```

### The resolver: pure logic on injectable interfaces

The central COM resolution engine takes `&dyn ComRegistry` and `&dyn ComFileSystem` and performs all the logic:

```rust
pub struct ComResolver<'a> {
    registry: &'a dyn ComRegistry,
    fs: &'a dyn ComFileSystem,
}

impl<'a> ComResolver<'a> {
    pub fn resolve_clsid(&self, clsid: &str, view: RegView) -> ComLookupResult { /* ... */ }
    pub fn resolve_progid(&self, progid: &str, view: RegView) -> ComLookupResult { /* ... */ }
    pub fn validate_server(&self, path: &str, views: &[RegView]) -> ComServerResult { /* ... */ }
    pub fn audit(
        &self, target_machine: MachineType, target_manifest: Option<&Manifest>,
        query: &str, query_kind: QueryKind,
    ) -> ComAuditResult { /* ... */ }
}
```

Because `ComResolver` only calls trait methods, every code path is testable with mocks.

## Testing tiers

### Tier 1: mock-based unit and resolver tests

Runs with:

```text
cargo test
```

This tier covers most COM logic without touching the host registry. It should be the default place for new COM tests.

Gated by: `#[cfg(test)]` — always runs.

#### Complete test case catalog

| Test case | What it validates | Mock setup |
|-----------|-------------------|------------|
| **ProgID: simple resolution** | ProgID -> CLSID lookup | MockRegistry: ProgID key with CLSID value |
| **ProgID: CurVer chain** | ProgID -> CurVer -> versioned ProgID -> CLSID | MockRegistry: ProgID, CurVer, versioned ProgID, CLSID keys |
| **ProgID: multi-hop CurVer** | CurVer chains more than one level deep | MockRegistry: 3+ CurVer hops |
| **ProgID: CurVer cycle** | Cyclic CurVer chain detected, returns PROGID_BROKEN | MockRegistry: CurVer A -> B -> A |
| **ProgID: missing CLSID** | ProgID exists but terminal CLSID key missing | MockRegistry: ProgID chain present, CLSID key absent |
| **ProgID: missing ProgID** | Input ProgID does not exist at all | MockRegistry: empty |
| **CLSID: basic InprocServer32** | CLSID -> InprocServer32 path extraction | MockRegistry: CLSID with InprocServer32 subkey |
| **CLSID: basic LocalServer32** | CLSID -> LocalServer32 command extraction | MockRegistry: CLSID with LocalServer32 subkey |
| **CLSID: TreatAs redirect** | CLSID A -> TreatAs -> CLSID B -> InprocServer32 | MockRegistry: two CLSIDs with TreatAs link |
| **CLSID: TreatAs cycle** | Cyclic TreatAs chain detected, returns TREATAS_BROKEN | MockRegistry: TreatAs A -> B -> A |
| **CLSID: TreatAs deep chain** | 3+ TreatAs hops resolve correctly | MockRegistry: chain of 3+ CLSIDs |
| **CLSID: not registered** | CLSID key does not exist | MockRegistry: empty |
| **CLSID: no server subkey** | CLSID exists but has no InprocServer32 or LocalServer32 | MockRegistry: CLSID key present, no server subkeys |
| **HKCU overrides HKLM** | Same CLSID in both hives, HKCU value wins | MockRegistry: different server paths in Hkcu64 vs Hklm64 |
| **HKCU absent, HKLM fallback** | CLSID only in HKLM, correctly found | MockRegistry: CLSID in Hklm64 only |
| **HKLM absent, HKCU present** | CLSID only in HKCU, correctly found | MockRegistry: CLSID in Hkcu64 only |
| **64-bit view selected** | `--view 64` reads from *64 locations | MockRegistry: different values in Hklm64 vs Hklm32 |
| **32-bit view selected** | `--view 32` reads from *32 locations | MockRegistry: different values in Hklm64 vs Hklm32 |
| **Server: file exists, valid PE** | Server path resolves, file exists, PE is valid x64 | MockFileSystem: add_pe with valid imports |
| **Server: file missing** | Server path resolves, file does not exist | MockFileSystem: empty |
| **Server: bad image** | Server path resolves, file exists but not a valid PE | MockFileSystem: add_raw with garbage bytes |
| **Server: wrong architecture** | InprocServer32 is x86 PE when view is 64-bit | MockFileSystem: add_pe_with_machine(0x014C) |
| **Server: transitive dep missing** | Server PE imports A, A imports B, B missing | MockFileSystem: chained PEs, B absent |
| **Server: transitive dep bad image** | Server PE imports A, A exists but is corrupt | MockFileSystem: server PE + bad-image file for A |
| **LocalServer32: quoted path** | `"C:\Program Files\app.exe" /Embedding` -> extracts exe path | MockRegistry: LocalServer32 with quoted command |
| **LocalServer32: unquoted path** | `C:\app.exe /flag` -> extracts exe path | MockRegistry: LocalServer32 with unquoted command |
| **LocalServer32: path only** | `C:\app.exe` with no arguments | MockRegistry: LocalServer32 with bare path |
| **Access denied: CLSID key** | Registry returns ACCESS_DENIED for CLSID read | MockRegistry: deny_access on CLSID subkey |
| **Access denied: partial chain** | ProgID resolves, but CLSID key is denied | MockRegistry: ProgID accessible, CLSID denied |
| **Manifest: basic comClass** | Manifest declares CLSID with server DLL | Direct string input to manifest parser |
| **Manifest: with progid** | Manifest includes progid attribute | Direct string input |
| **Manifest: with threadingModel** | ThreadingModel attribute extracted | Direct string input |
| **Manifest: multiple comClass** | Multiple classes in one manifest | Direct string input |
| **Manifest: no comClass** | Manifest exists but has no COM declarations | Direct string input |
| **Manifest: malformed XML** | Graceful handling of broken manifest | Direct string input |
| **Audit: manifest wins over registry** | Target manifest declares CLSID, registry also has it | MockRegistry + manifest data; verify source="manifest" |
| **Audit: registry fallback** | Target manifest has no match, registry does | MockRegistry populated, empty manifest |
| **Audit: bitness mismatch** | x86 target, x64 InprocServer32 | MockRegistry + MockFileSystem with x64 PE |
| **Audit: bitness match** | x64 target, x64 InprocServer32 | MockRegistry + MockFileSystem with x64 PE |
| **Audit: LocalServer32 no bitness check** | Per V2 spec, LocalServer32 does not flag BITNESS_MISMATCH | MockRegistry with LocalServer32 + mismatched PE |
| **Token: summary mode clsid** | Only COM_LOOKUP emitted | Run resolver, capture output in summary mode |
| **Token: summary mode server** | Only COM_SERVER emitted | Run resolver, capture output in summary mode |
| **Token: summary mode audit** | Only COM_AUDIT emitted | Run resolver, capture output in summary mode |
| **Token: trace mode includes deps** | COM_DEPENDENCY_STATUS, SEARCH_ORDER, SEARCH_PATH emitted | Run resolver, capture output in trace mode |
| **Token: field ordering** | Required fields present, optional fields conditional | Parse emitted tokens, validate structure |
| **Exit code: 0 for OK** | No issue -> exit 0 | Healthy mock state |
| **Exit code: 10 for issue** | Missing server -> exit 10 | Broken mock state |
| **Exit code: 21 for access denied** | ACCESS_DENIED -> exit 21 | Denied mock state |
| **Edge: empty CLSID string** | Graceful error, not a panic | Pass "" as clsid |
| **Edge: malformed GUID** | Missing braces, wrong length, non-hex chars | Various malformed inputs |
| **Edge: very long ProgID** | No buffer overflow or truncation | 1000+ char ProgID |
| **Edge: non-ASCII ProgID** | Unicode handling | ProgID with non-ASCII chars |
| **Edge: path with spaces** | Path normalization handles spaces | MockFileSystem with spaced path |

This tier should cover **80-90% of all COM diagnostic code paths**. The resolver logic, chain traversal, error classification, merge rules, conflict detection, and token emission are all exercised without touching the host.

#### Example test implementations

**ProgID chain resolution:**

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

    let fs = MockFileSystem::new(); // no file checks for this test
    let resolver = ComResolver::new(&reg, &fs);
    let result = resolver.resolve_progid("Vendor.Widget", RegView::V64);

    assert_eq!(result.status, LookupStatus::Registered);
    assert_eq!(result.clsid.as_deref(), Some("{AAAA-BBBB-CCCC-DDDD}"));
    assert_eq!(result.hive, Some(Hive::Hklm));
    assert_eq!(result.server_kind, Some(ServerKind::InprocServer32));
}
```

**TreatAs cycle detection:**

```rust
#[test]
fn treatas_cycle_returns_broken() {
    let mut reg = MockRegistry::new();

    // CLSID A -> TreatAs -> CLSID B
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{CYCLE-A}\TreatAs",
        "",
        RegValue::String("{CYCLE-B}".into()));
    // CLSID B -> TreatAs -> CLSID A  (cycle!)
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{CYCLE-B}\TreatAs",
        "",
        RegValue::String("{CYCLE-A}".into()));

    let fs = MockFileSystem::new();
    let resolver = ComResolver::new(&reg, &fs);
    let result = resolver.resolve_clsid("{CYCLE-A}", RegView::V64);

    assert_eq!(result.status, LookupStatus::TreatAsBroken);
}
```

**COM server with missing transitive dependency:**

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
    let result = resolver.resolve_clsid("{TEST-0001}", RegView::V64);

    assert_eq!(result.status, LookupStatus::Registered);
    assert_eq!(result.server_status, Some(ServerStatus::ServerDepsMissing));
}
```

**HKCU overriding HKLM:**

```rust
#[test]
fn hkcu_overrides_hklm_for_same_clsid() {
    let mut reg = MockRegistry::new();

    // HKLM has one server path
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{OVERRIDE-TEST}\InprocServer32",
        "",
        RegValue::String(r"C:\Old\server.dll".into()));

    // HKCU has a different server path -> should win
    reg.set(RegLocation::Hkcu64,
        r"Software\Classes\CLSID\{OVERRIDE-TEST}\InprocServer32",
        "",
        RegValue::String(r"C:\New\server.dll".into()));

    let fs = MockFileSystem::new();
    let resolver = ComResolver::new(&reg, &fs);
    let result = resolver.resolve_clsid("{OVERRIDE-TEST}", RegView::V64);

    assert_eq!(result.status, LookupStatus::Registered);
    assert_eq!(result.hive, Some(Hive::Hkcu));
    assert_eq!(result.server_path.as_deref(), Some(r"C:\New\server.dll"));
}
```

**Access denied on a specific key:**

```rust
#[test]
fn access_denied_on_clsid_key() {
    let mut reg = MockRegistry::new();

    // The key exists but is denied
    reg.set(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{DENIED-TEST}\InprocServer32",
        "",
        RegValue::String(r"C:\Vendor\server.dll".into()));
    reg.deny_access(RegLocation::Hklm64,
        r"Software\Classes\CLSID\{DENIED-TEST}");

    let fs = MockFileSystem::new();
    let resolver = ComResolver::new(&reg, &fs);
    let result = resolver.resolve_clsid("{DENIED-TEST}", RegView::V64);

    assert_eq!(result.status, LookupStatus::AccessDenied);
}
```

**Manifest parsing:**

```rust
#[test]
fn manifest_comclass_extraction() {
    let xml = r#"
    <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
      <file name="vendor.dll">
        <comClass clsid="{MANIFEST-001}"
                  progid="Vendor.Widget"
                  threadingModel="Both" />
      </file>
    </assembly>
    "#;

    let classes = parse_manifest_com_classes(xml);

    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].clsid, "{MANIFEST-001}");
    assert_eq!(classes[0].progid.as_deref(), Some("Vendor.Widget"));
    assert_eq!(classes[0].threading_model.as_deref(), Some("Both"));
    assert_eq!(classes[0].server_dll.as_deref(), Some("vendor.dll"));
}
```

### Tier 2: fixture-backed file tests

Runs with:

```text
cargo xtask test
```

This tier validates the parts that need real PE files on disk but do not require real registry mutation.

Gated by: `#[cfg(feature = "harness-tests")]` — same as existing v1 integration tests.

#### Scope

- `com server <path>` against real fixture DLLs and EXEs
- machine-type detection on real PE files
- bad-image handling on real corrupt files
- dependency-walk integration on real files with real import tables
- manifest parsing on fixture executables with embedded or sidecar manifests

#### Recommended fixture additions

The existing MSBuild fixture project should be extended with:

| Fixture | Purpose | Build notes |
|---------|---------|-------------|
| `lwtest_com_server.dll` | Valid x64 DLL suitable as InprocServer32 | Standard x64 DLL, exports `DllGetClassObject` stub |
| `lwtest_com_server_x86.dll` | Valid x86 DLL for bitness mismatch tests | Cross-compile as x86 |
| `lwtest_com_server_broken_dep.dll` | DLL that imports `lwtest_nonexistent.dll` | x64 DLL with broken import |
| `lwtest_com_localserver.exe` | Minimal EXE for LocalServer32 tests | x64 EXE, no special behavior |
| `lwtest_com_manifest.exe` | EXE with embedded manifest containing comClass | x64 EXE with RT_MANIFEST resource |
| `lwtest_com_manifest.exe.manifest` | Sidecar manifest with comClass | XML file adjacent to a fixture EXE |

What this tier intentionally does NOT do: create registry entries. All fixture-backed tests that need registry data should either use mock injection or be promoted to Tier 3.

### Extending pe_builder for COM fixtures

The existing `pe_builder::build_import_test_pe` creates minimal x64 PEs with import tables. For COM testing, it needs extensions:

#### Machine type control

Build x86 PEs by changing the machine field from `0x8664` to `0x014C`:

```rust
pub fn build_import_test_pe_with_machine(imports: &[&str], machine: u16) -> Vec<u8> {
    let mut pe = build_import_test_pe(imports);
    // Machine field is at PE_OFFSET + 4
    let offset = PE_OFFSET + 4;
    pe[offset..offset + 2].copy_from_slice(&machine.to_le_bytes());
    pe
}
```

#### DLL flag

Set the `IMAGE_FILE_DLL` characteristic so the PE is recognized as a DLL:

```rust
pub fn build_dll_test_pe(imports: &[&str]) -> Vec<u8> {
    let mut pe = build_import_test_pe(imports);
    // Characteristics field at PE_OFFSET + 22
    let offset = PE_OFFSET + 22;
    let chars = u16::from_le_bytes([pe[offset], pe[offset + 1]]);
    let chars = chars | 0x2000; // IMAGE_FILE_DLL
    pe[offset..offset + 2].copy_from_slice(&chars.to_le_bytes());
    pe
}
```

#### Manifest resource embedding

Add an `RT_MANIFEST` resource to a PE for registration-free COM testing. This is the most complex extension — it requires adding a resource section with a resource directory and a single manifest entry. A minimal implementation:

```rust
pub fn build_pe_with_manifest(imports: &[&str], manifest_xml: &str) -> Vec<u8> {
    let mut pe = build_import_test_pe(imports);
    // Append a .rsrc section containing the manifest
    // RT_MANIFEST (24), ID 1 (for EXE) or 2 (for DLL)
    // This requires updating:
    //   - NumberOfSections
    //   - section table (add .rsrc entry)
    //   - resource directory structure
    //   - data directory entry for resources
    // Implementation details deferred to build time.
    todo!("implement resource section builder")
}
```

An alternative to programmatic manifest embedding is building fixture EXEs via MSBuild with a manifest file in the project — this is simpler and more maintainable for the initial implementation.

### Tier 3: container-based system tests

Runs with:

```text
cargo xtask test-container
```

This tier exists for the Windows behaviors that mocks should not be trusted to simulate.

Gated by: `#[cfg(feature = "container-tests")]` or run externally via Docker.

#### What mocks cannot cover

| Behavior | Why mocks are insufficient |
|----------|---------------------------|
| **Real HKCR merge** | HKCR is a virtual key constructed by the kernel; mock doesn't replicate kernel merge timing or caching |
| **Real 32/64 registry views** | `KEY_WOW64_32KEY` / `KEY_WOW64_64KEY` redirect to different physical registry paths; mock can't test the FFI flag handling |
| **Exact Win32 error codes** | Real error codes for access-denied vs not-found vs other failures may differ from mock assumptions |
| **PE loading of COM servers** | Mock PE headers test parsing but not whether Windows actually considers the file loadable |
| **Loader-snaps + COM interaction** | When a COM in-proc server's dependency is missing, does the loader-snaps output match expected patterns? |
| **IFEO registry path** | Loader-snaps IFEO fallback writes to real HKLM; must not leak to host |
| **End-to-end CLI output** | Full pipeline from CLI args through registry reads through PE parsing through token emission |
| **Elevation scenarios** | Non-admin vs admin registry access differences |

#### Docker setup

```dockerfile
FROM mcr.microsoft.com/windows/servercore:ltsc2022

# Copy loadwhat and fixtures
COPY target/release/loadwhat.exe C:/tools/
COPY target/loadwhat-tests/fixtures/ C:/fixtures/

# Copy test scripts
COPY tests/com/container/ C:/tests/

# Default entrypoint runs setup then tests
ENTRYPOINT ["powershell", "-File", "C:/tests/run_container_tests.ps1"]
```

#### Feasibility notes

**What works well:**

- Registry isolation is real. Each Windows container has its own `HKLM` and `HKCU`. Tests can create, modify, and destroy COM registrations freely without affecting the host.
- PE execution works. The Windows kernel is shared (in process-isolation mode), so `CreateProcessW`, `WaitForDebugEvent`, `ReadProcessMemory`, and other Win32 debug APIs function normally.
- Loader-snaps should work. `NtGlobalFlag` manipulation targets the PEB of the debugged process, which is a per-process structure.
- IFEO registry writes are container-local, so the loader-snaps fallback path can be tested safely.

**What requires caution:**

- **Image size.** Windows Server Core images are 5+ GB. CI runners need to cache this aggressively.
- **Host OS version coupling.** Process-isolated Windows containers require the container OS version to match the host (or be within the same servicing window). Hyper-V isolation removes this constraint but adds overhead.
- **CI support.** GitHub Actions Windows runners support Windows containers with limitations. Self-hosted runners are more reliable.
- **Debug API permissions.** `SeDebugPrivilege` may need to be granted inside the container. Validate early.

**What does not work:**

- **Nano Server images** do not include the Win32 subsystem, COM runtime, or debug APIs.
- **Linux containers** are not an option for Win32 API testing.

#### Registry fixture script

```powershell
# tests/com/container/setup_fixtures.ps1

$TestPrefix = "DEADBEEF-TEST"

# --- Scenario: basic InprocServer32 registration ---
$clsid = "{$TestPrefix-0001-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid\InprocServer32" `
    -Name "ThreadingModel" -Value "Both"

# --- Scenario: ProgID -> CurVer -> CLSID chain ---
$clsid_progid = "{$TestPrefix-0002-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\LwTest.Widget" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\LwTest.Widget" `
    -Name "(Default)" -Value "LwTest.Widget"
New-Item -Path "HKLM:\Software\Classes\LwTest.Widget\CurVer" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\LwTest.Widget\CurVer" `
    -Name "(Default)" -Value "LwTest.Widget.2"
New-Item -Path "HKLM:\Software\Classes\LwTest.Widget.2\CLSID" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\LwTest.Widget.2\CLSID" `
    -Name "(Default)" -Value $clsid_progid
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_progid\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_progid\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"

# --- Scenario: HKCU overrides HKLM ---
$clsid_override = "{$TestPrefix-0003-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_override\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_override\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a_v1.dll"
New-Item -Path "HKCU:\Software\Classes\CLSID\$clsid_override\InprocServer32" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$clsid_override\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a_v2.dll"

# --- Scenario: 32-bit vs 64-bit view mismatch ---
$clsid_bitness = "{$TestPrefix-0004-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_bitness\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_bitness\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
# Write to 32-bit view
reg add "HKLM\Software\Classes\WOW6432Node\CLSID\$clsid_bitness\InprocServer32" `
    /ve /d "C:\fixtures\lwtest_com_server_x86.dll" /f

# --- Scenario: missing server file ---
$clsid_missing = "{$TestPrefix-0005-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_missing\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_missing\InprocServer32" `
    -Name "(Default)" -Value "C:\nonexistent\path\missing.dll"

# --- Scenario: server with broken imports ---
$clsid_broken = "{$TestPrefix-0006-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_broken\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_broken\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_com_server_broken_dep.dll"

# --- Scenario: LocalServer32 with quoted command line ---
$clsid_local = "{$TestPrefix-0007-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_local\LocalServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_local\LocalServer32" `
    -Name "(Default)" -Value '"C:\fixtures\lwtest_com_localserver.exe" /Embedding'

# --- Scenario: TreatAs redirect (2-hop) ---
$clsid_old = "{$TestPrefix-0008-0000-0000-000000000001}"
$clsid_new = "{$TestPrefix-0008-0000-0000-000000000002}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_old\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_old\TreatAs" `
    -Name "(Default)" -Value $clsid_new
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_new\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_new\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"

# --- Scenario: TreatAs cycle ---
$clsid_cycle_a = "{$TestPrefix-0009-0000-0000-000000000001}"
$clsid_cycle_b = "{$TestPrefix-0009-0000-0000-000000000002}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_a\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_a\TreatAs" `
    -Name "(Default)" -Value $clsid_cycle_b
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_b\TreatAs" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_cycle_b\TreatAs" `
    -Name "(Default)" -Value $clsid_cycle_a

# --- Scenario: access-denied key ---
$clsid_denied = "{$TestPrefix-000A-0000-0000-000000000001}"
New-Item -Path "HKLM:\Software\Classes\CLSID\$clsid_denied\InprocServer32" -Force
Set-ItemProperty -Path "HKLM:\Software\Classes\CLSID\$clsid_denied\InprocServer32" `
    -Name "(Default)" -Value "C:\fixtures\lwtest_a.dll"
$acl = Get-Acl "HKLM:\Software\Classes\CLSID\$clsid_denied"
$rule = New-Object System.Security.AccessControl.RegistryAccessRule(
    "BUILTIN\Users", "ReadKey", "Deny")
$acl.AddAccessRule($rule)
Set-Acl -Path "HKLM:\Software\Classes\CLSID\$clsid_denied" -AclObject $acl
```

#### Container test runner

```powershell
# tests/com/container/run_container_tests.ps1

# Setup fixture registrations
& C:\tests\setup_fixtures.ps1

$total = 0
$passed = 0
$failures = @()

function Run-Test {
    param([string]$Name, [string]$Args, [string]$ExpectPattern, [int]$ExpectExit = -1)
    $script:total++
    $output = & C:\tools\loadwhat.exe $Args.Split(' ') 2>&1
    $exitCode = $LASTEXITCODE
    $matched = $output | Select-String -Pattern $ExpectPattern -Quiet
    $exitOk = ($ExpectExit -eq -1) -or ($exitCode -eq $ExpectExit)

    if ($matched -and $exitOk) {
        $script:passed++
        Write-Host "  PASS: $Name"
    } else {
        $script:failures += $Name
        Write-Host "  FAIL: $Name"
        Write-Host "    output: $output"
        Write-Host "    exit:   $exitCode"
    }
}

Write-Host "Running container COM tests..."

Run-Test "basic_clsid_lookup" `
    'com clsid {DEADBEEF-TEST-0001-0000-0000-000000000001}' `
    'status="REGISTERED"' 0

Run-Test "progid_curver_chain" `
    'com progid LwTest.Widget' `
    'status="REGISTERED".*clsid=' 0

Run-Test "hkcu_overrides_hklm" `
    'com clsid {DEADBEEF-TEST-0003-0000-0000-000000000001}' `
    'hive="HKCU"' 0

Run-Test "missing_server_file" `
    'com clsid {DEADBEEF-TEST-0005-0000-0000-000000000001}' `
    'server_status="SERVER_MISSING"' 10

Run-Test "broken_server_imports" `
    'com clsid {DEADBEEF-TEST-0006-0000-0000-000000000001}' `
    'server_status="SERVER_DEPS_MISSING"' 10

Run-Test "localserver32_command" `
    'com clsid {DEADBEEF-TEST-0007-0000-0000-000000000001}' `
    'server_kind="LocalServer32"' 0

Run-Test "treatas_redirect" `
    'com clsid {DEADBEEF-TEST-0008-0000-0000-000000000001}' `
    'status="REGISTERED"' 0

Run-Test "treatas_cycle" `
    'com clsid {DEADBEEF-TEST-0009-0000-0000-000000000001}' `
    'status="TREATAS_BROKEN"' 10

Run-Test "clsid_not_registered" `
    'com clsid {00000000-0000-0000-0000-DOES-NOT-EXIST}' `
    'status="NOT_REGISTERED"' 10

# Access-denied test: run as restricted user
# (requires the container to have a non-admin user configured)
# Run-Test "access_denied" ...

Write-Host ""
Write-Host "$passed/$total tests passed."
if ($failures.Count -gt 0) {
    Write-Host "FAILED: $($failures -join ', ')"
    exit 1
}
exit 0
```

## Test placement decision tree

When adding a test for new COM behavior:

```
Does the test need real Windows registry behavior?
(HKCR merge, 32/64 view flags, ACL enforcement, real error codes)
  │
  ├─ YES ──> Tier 3 (container)
  │          Also write a Tier 1 mock test for the same logic path.
  │
  └─ NO
      │
      Does the test need real PE files on disk?
      (actual DLL loading, loader-snaps interaction, debug APIs)
        │
        ├─ YES ──> Tier 2 (fixture-backed)
        │          Also write a Tier 1 mock test for the parsing/logic.
        │
        └─ NO ──> Tier 1 (mock-based)
                   This is the default.
```

The goal is for every Tier 2 or Tier 3 test to have a corresponding Tier 1 test that covers the same logic path with mocks. Container tests validate platform integration. Mock tests prevent regressions and enable fast iteration.

## CI pipeline

```
┌────────────────────────────────┐
│  cargo test                    │  Tier 1: mock tests
│  (always, every PR)            │  ~seconds
└──────────────┬─────────────────┘
               │ pass
               v
┌────────────────────────────────┐
│  cargo xtask test              │  Tier 2: fixture-backed tests
│  (always, every PR)            │  ~minutes
└──────────────┬─────────────────┘
               │ pass
               v
┌────────────────────────────────┐
│  cargo xtask test-container    │  Tier 3: container tests
│  (nightly / pre-release only)  │  ~5-10 minutes
└────────────────────────────────┘
```

- **Every PR:** Tier 1 + Tier 2. Fast, no special infrastructure.
- **Nightly / pre-release:** Tier 1 + Tier 2 + Tier 3. Full validation including container tests.
- **Local dev:** `cargo test` for fast iteration. `cargo xtask test-container` when working on registry FFI.

Tier 1 and Tier 2 failures block merges. Tier 3 failures block releases but not day-to-day development.

## Fixture design principles

COM test fixtures should follow the same principles as v1 fixtures:

- **Deterministic:** every test creates exactly the state it needs, no dependence on ambient machine state
- **Isolated:** tests do not interfere with each other (use unique CLSIDs per test)
- **Self-contained:** fixture DLLs are minimal PEs built by the existing `pe_builder` or by MSBuild fixture projects
- **Documented:** each fixture registration documents what it's testing and why

Recommended fixture CLSID scheme: use a test-specific prefix like `{DEADBEEF-TEST-xxxx-xxxx-xxxxxxxxxxxx}` to avoid collisions with real COM classes. Each test scenario gets a unique 4th group (e.g., `0001` for basic lookup, `0002` for ProgID chains, etc.).

## Suggested boundaries

The testing plan should stay separate from the public spec for two reasons:

- the public contract needs to stay compact and stable
- test infrastructure will evolve faster than CLI behavior

If COM support expands beyond V2 into `run --com` or runtime tracing, add those tests here rather than growing the public spec document.
