# COM Feature Analysis for loadwhat

This document analyzes the technical feasibility of adding COM (Component Object Model) diagnostics to loadwhat.

## Technical Feasibility: Yes, but with constraints

### What loadwhat already has that helps

- **Registry FFI** in `win.rs` (RegOpenKeyExW, RegQueryValueExW, etc.) - needed for CLSID lookups
- **PE parsing** infrastructure - can verify COM server DLLs are valid
- **DLL search resolution** - can verify registered paths exist and are loadable
- **Debug loop** - could capture COM-related exceptions
- **Loader-snaps** - sometimes shows COM server DLL loading failures

---

## COM Feature Design - Two Tiers

### Tier 1: Standalone helpers (easier, high value)

```
loadwhat com clsid {GUID}
loadwhat com progid Name.Object
```

These would:

1. Look up CLSID in registry (`HKCR\CLSID\{...}\InprocServer32` or `LocalServer32`)
2. Report what DLL/EXE is registered (or if not registered at all)
3. Verify the registered path exists and is a valid PE
4. Extract version info from the DLL's VERSION_INFO resource
5. If it's an in-proc server, run it through the existing DLL diagnosis (check its dependencies)

**Example output:**

```
COM_LOOKUP clsid={00000000-0000-0000-0000-000000000000} status=REGISTERED
COM_SERVER type=InprocServer32 path="C:\Windows\System32\foo.dll"
COM_SERVER_VERSION file_version=1.2.3.4 product_version=1.2.3.4
COM_SERVER_CHECK status=VALID dependencies_ok=true
```

### Tier 2: Runtime COM failure detection (harder)

```
loadwhat run exe_with_com_problem
```

**Challenges:**

1. **Detection**: How do we know *which* CLSID the app tried to activate?
   - COM errors like `REGDB_E_CLASSNOTREG` (0x80040154) don't crash the process - they return HRESULTs
   - We'd need to intercept `CoCreateInstance` or use ETW tracing

2. **Possible approaches:**
   - **Loader-snaps**: Already captures DLL load failures - so if the COM server DLL is missing/broken, we'd see it
   - **ETW tracing**: Windows has COM ETW providers (`Microsoft-Windows-COM`) that emit activation events with CLSIDs and results
   - **Exception scanning**: Some apps throw on COM failures - we could pattern-match exception data
   - **Import scanning**: Find CLSIDs hardcoded in the binary (heuristic - many false positives)

---

## The "Wrong Version" Problem

The desired scenario:

> "wrong version registered at ____, you want _____"

This is **partially solvable**:

| What we CAN do | What we CAN'T easily do |
|----------------|-------------------------|
| Report what version IS registered | Know what version the app EXPECTED |
| Compare TypeLib versions in registry | Read app's internal version requirements |
| Check manifest-declared dependencies | Handle runtime-determined CLSIDs |
| Verify DLL has expected exports | Know if it's a "wrong version" vs "broken registration" |

### Manifest-based detection (promising approach)

- Side-by-side (SxS) manifests can declare COM dependencies with version requirements
- We could parse the app's manifest and compare against registry
- This gives us the "expected vs actual" comparison for apps that use manifests

---

## Proposed Implementation Roadmap

### Phase 1: `com clsid` and `com progid` helpers

- Registry lookup + validation
- Version info extraction from PE resources
- Dependency check on in-proc servers
- Estimated: ~400-600 lines of new code

### Phase 2: Manifest-aware COM checking

- Parse app manifest for `comClass` / `comInterfaceProxyStub` elements
- Compare declared versions against registered versions
- Emit `COM_VERSION_MISMATCH` tokens

### Phase 3: Runtime COM failure detection

- ETW-based tracing (more complex, needs new infrastructure)
- Or heuristic loader-snaps correlation when COM server DLLs fail to load

---

## Example Output Vision

```
loadwhat run broken_com_app.exe
...
COM_ACTIVATION_FAILED clsid={...} hr=0x80040154 reason=REGDB_E_CLASSNOTREG
COM_SUGGESTED_FIX message="CLSID not registered - run regsvr32 foo.dll"
```

For version mismatch:

```
COM_VERSION_MISMATCH clsid={...}
  manifest_expects="1.0.0.0"
  registered_path="C:\old\foo.dll"
  registered_version="0.9.0.0"
COM_SUGGESTED_FIX message="Wrong version at C:\old\foo.dll, expected 1.0.0.0, found 0.9.0.0. Reinstall or re-register correct version."
```

---

## Feasibility Summary

| Feature | Feasible? | Effort | Value |
|---------|-----------|--------|-------|
| `com clsid/progid` helpers | Yes | Medium | High |
| Version info extraction | Yes | Low | High |
| Manifest comparison | Yes | Medium | High |
| Runtime CLSID capture | Complex | High | Medium |
| "You want version X" | Only with manifest | Medium | High |

---

## Conclusion

The standalone helpers are definitely achievable and fit the existing architecture well. Full runtime "what CLSID failed" detection would require ETW infrastructure which is a bigger lift, but the static/manifest-based approach could catch many real-world issues.
