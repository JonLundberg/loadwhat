# loadwhat COM Plan

Status: planning document for future COM support, likely v3 or later.

This document is not the source of truth for current implemented behavior. Current behavior remains defined by [docs/loadwhat_spec_v1.md](./loadwhat_spec_v1.md).

This document is also not the v2 spec. V2 is reserved for x86/WOW64 support for the existing DLL-loading mission.

Purpose: extend `loadwhat` with deterministic COM registration and activation-prerequisite diagnosis without changing the v1 behavior of `run` or `imports`.

Output remains line-oriented and greppable:

```text
TOKEN key=value key=value ...
```

## 1) CLI

### COM commands

```text
loadwhat com clsid [OPTIONS] <{CLSID}>
loadwhat com progid [OPTIONS] <PROGID>
loadwhat com server [OPTIONS] <PATH>
loadwhat com audit [OPTIONS] <TARGET> <{CLSID}|PROGID>
```

### Common COM options

- Default mode is summary mode.
- `--trace` enables supporting COM tokens.
- `-v` / `--verbose` is accepted and behaves the same as `--trace` for COM commands.
- Later flags win per dimension: `--trace` vs `--summary`.

### View-selection options

- `com clsid` and `com progid`:

```text
--view 64
--view 32
```

- Default is `--view 64`.
- `com server` accepts `--view 64`, `--view 32`, or `--view both`.
- Default for `com server` is `--view both`.
- `com audit` does not accept `--view`; it derives the effective registry view from the target image machine type.
- In `com audit`, a braced GUID argument is treated as a CLSID query; any other argument is treated as a ProgID query.

## 2) Output mode policy

### Summary mode

Summary mode emits exactly one token line:

- `com clsid` / `com progid`: `COM_LOOKUP`
- `com server`: `COM_SERVER`
- `com audit`: `COM_AUDIT`

### Trace mode

Trace mode may emit:

- `COM_LOOKUP`
- `COM_SERVER`
- `COM_REGISTRATION`
- `COM_PROGID`
- `COM_MANIFEST`
- `COM_AUDIT`
- `COM_DEPENDENCY_STATUS`
- `SEARCH_ORDER`
- `SEARCH_PATH`
- `NOTE`

`SEARCH_ORDER` and `SEARCH_PATH` are only emitted when a COM server dependency walk encounters a missing or bad-image DLL and the fixed DLL search model from v1 can be reconstructed for that server.

### Verbose mode

COM commands do not emit runtime timeline tokens. For COM commands, `-v` / `--verbose` is equivalent to `--trace`.

## 3) Scope

### Supported future COM model

Future COM support covers:

- registry-backed `InprocServer32`
- registry-backed `LocalServer32`
- HKCU/HKLM merged lookup within a selected registry view
- 32-bit and 64-bit registry views on x64 Windows
- `ProgID` and `CurVer` resolution
- `TreatAs` redirection
- target-scoped registration-free COM manifest declarations in `com audit`
- PE validation and dependency diagnosis of resolved server binaries

### Not in planned COM scope

This COM plan does not define:

- `run --com`
- `imports --com`
- ETW-backed runtime COM tracing
- `AppID`-driven `LocalService`
- `DllSurrogate`
- `RemoteServerName` or remote COM / DCOM
- COM launch or activation permissions
- cross-user HKCU inspection
- full SxS assembly resolution or publisher policy

When a likely out-of-scope feature is encountered, emit:

```text
NOTE topic="com" detail="out-of-scope" feature="..."
```

## 4) Resolution model

### Registry merge rules

Within a selected registry view:

1. Query `HKCU\Software\Classes` first.
2. Fall back to `HKLM\Software\Classes`.
3. Treat `HKCU` as overriding `HKLM` for the same key path.
4. Do not rely on direct `HKCR` reads for public behavior.

### ProgID resolution

`com progid` resolves as follows:

1. Resolve the input ProgID in the selected view using the merge rules above.
2. If the ProgID has a `CurVer`, follow it.
3. Continue until a ProgID without `CurVer` is reached.
4. Detect cycles deterministically.
5. Resolve the terminal ProgID's `CLSID` value.

Failure behavior:

- missing ProgID or missing CLSID after a ProgID chain: `COM_LOOKUP status="PROGID_BROKEN"`
- cyclic `CurVer` chain: `COM_LOOKUP status="PROGID_BROKEN"`

### CLSID resolution

`com clsid` resolves as follows:

1. Locate the CLSID in the selected view using the merge rules above.
2. If the CLSID has a `TreatAs`, follow it.
3. Continue until a CLSID without `TreatAs` is reached.
4. Detect cycles deterministically.
5. Inspect supported server subkeys: `InprocServer32`, then `LocalServer32`.

Failure behavior:

- missing CLSID key: `COM_LOOKUP status="NOT_REGISTERED"`
- cyclic `TreatAs` chain: `COM_LOOKUP status="TREATAS_BROKEN"`
- CLSID present but no supported server subkey: `COM_LOOKUP status="BROKEN_REGISTRATION"`

### Target-scoped audit

`com audit` resolves as follows:

1. Parse the target PE to determine `target_machine`.
2. Choose the registry view from `target_machine`:
   - x64 target -> 64-bit view
   - x86 target -> 32-bit view
3. Parse the target manifest for registration-free COM declarations.
4. If the target manifest declares the queried class, that declaration is the primary source for the audit result.
5. Otherwise, resolve the class through registry-backed `com clsid` / `com progid` logic in the target view.
6. Validate the resolved server and, if it is a DLL or EXE path, run dependency diagnosis on that server.

Manifest support is only target-scoped. `com clsid`, `com progid`, and `com server` do not consult manifests.

## 5) `com server` reverse lookup

`com server` validates a server binary and performs reverse lookup over supported registrations.

Behavior:

1. Normalize the input path to an absolute path.
2. Validate the file and determine its machine type.
3. Scan the selected registry views for `InprocServer32` and `LocalServer32` entries.
4. For each candidate:
   - expand environment variables for expandable registry values
   - if `LocalServer32`, extract the executable path from the command line
   - normalize the candidate path
   - compare case-insensitively against the normalized input path
5. Emit matching registrations in deterministic order.

Deterministic output order for reverse lookup:

1. view order: `64`, then `32`
2. hive order: `HKCU`, then `HKLM`
3. CLSID lexicographic
4. ProgID lexicographic

## 6) Server validation

When a supported server path is resolved, future COM support validates it in this order:

1. file exists
2. PE image is readable and structurally valid
3. if `server_kind="InprocServer32"`, machine type is compatible with the relevant caller:
   - `com clsid` / `com progid`: the current `loadwhat` build (`x64`)
   - `com audit`: the target image machine type
4. if `server_kind="LocalServer32"`, report machine type but do not classify x86/x64 differences as `BITNESS_MISMATCH`
5. transitive DLL dependency diagnosis using the existing deterministic import walk

`LocalServer32` command lines are validated by executable path only. Command-line arguments are preserved in trace output when available but are not interpreted semantically.

## 7) Token contract

### `COM_LOOKUP`

Purpose: resolution result for `com clsid` or `com progid`.

Required fields:

- `query_kind="clsid|progid"`
- `query="..."`
- `status="REGISTERED|NOT_REGISTERED|PROGID_BROKEN|TREATAS_BROKEN|BROKEN_REGISTRATION|ACCESS_DENIED"`

Optional fields:

- `clsid="{...}"`
- `hive="HKCU|HKLM"`
- `view="64|32"`
- `server_kind="InprocServer32|LocalServer32"`
- `server_status="OK|SERVER_MISSING|SERVER_BAD_IMAGE|SERVER_DEPS_MISSING|BITNESS_MISMATCH|SKIPPED"`

`status` is a lookup result. `server_status` is a separate optional server-health result.
`BITNESS_MISMATCH` applies only to `InprocServer32`.

### `COM_SERVER`

Purpose: validation result for `com server`, or supporting server detail in trace output.

Required fields:

- `path="..."`
- `status="OK|SERVER_MISSING|SERVER_BAD_IMAGE|SERVER_DEPS_MISSING|BITNESS_MISMATCH|ACCESS_DENIED"`

Optional fields:

- `machine="x64|x86|unknown"`
- `views="64|32|64,32"`
- `registrations=<n>`
- `server_kind="InprocServer32|LocalServer32"`
- `threading_model="..."`

`BITNESS_MISMATCH` applies only to `InprocServer32`.

### `COM_REGISTRATION`

Purpose: reverse-lookup registration pointing to a server path.

Required fields:

- `clsid="{...}"`
- `hive="HKCU|HKLM"`
- `view="64|32"`
- `server_kind="InprocServer32|LocalServer32"`
- `path="..."`

Optional fields:

- `threading_model="..."`

### `COM_PROGID`

Purpose: ProgID associated with a CLSID.

Required fields:

- `clsid="{...}"`
- `progid="..."`

Optional fields:

- `curver="..."`

### `COM_MANIFEST`

Purpose: target-scoped registration-free COM declaration used by `com audit`.

Required fields:

- `source="embedded|sidecar"`
- `file="..."`
- `clsid="{...}"`
- `server="..."`

Optional fields:

- `progid="..."`
- `threading_model="..."`

### `COM_AUDIT`

Purpose: overall activation-prerequisite result for a target and class.

Required fields:

- `target="..."`
- `target_machine="x64|x86|unknown"`
- `query_kind="clsid|progid"`
- `query="..."`
- `source="registry|manifest"`
- `status="OK|NOT_REGISTERED|PROGID_BROKEN|TREATAS_BROKEN|BROKEN_REGISTRATION|SERVER_MISSING|SERVER_BAD_IMAGE|SERVER_DEPS_MISSING|BITNESS_MISMATCH|ACCESS_DENIED"`

Optional fields:

- `clsid="{...}"`
- `server_kind="InprocServer32|LocalServer32"`
- `server_path="..."`

### `COM_DEPENDENCY_STATUS`

Purpose: failing dependency discovered while validating a COM server.

Required fields:

- `status="MISSING|BAD_IMAGE"`
- `dll="..."`

Optional fields:

- `via="..."`
- `depth=<n>`

## 8) Summary behavior examples

Examples:

```text
COM_LOOKUP query_kind="clsid" query="{...}" status="NOT_REGISTERED"
COM_LOOKUP query_kind="progid" query="Vendor.Object" status="REGISTERED" clsid="{...}" hive="HKLM" view="64" server_kind="InprocServer32" server_status="OK"
COM_SERVER path="C:\Vendor\foo.dll" status="SERVER_DEPS_MISSING" machine="x64" views="64,32" registrations=2
COM_AUDIT target="app.exe" target_machine="x86" query_kind="clsid" query="{...}" source="registry" status="BITNESS_MISMATCH" clsid="{...}" server_kind="InprocServer32" server_path="C:\Vendor\foo.dll"
```

## 9) Exit codes

- `0` = command completed and reported no COM issue
- `10` = command completed and reported a definitive COM issue
- `20` = usage error
- `21` = command could not determine the answer because required data was inaccessible or unsupported for the requested path
- `22` = unsupported architecture for the requested operation

`ACCESS_DENIED` is a public result, but it still exits `21` because the diagnosis is incomplete.

For `COM_AUDIT`, `BITNESS_MISMATCH` applies only when the selected server kind is `InprocServer32`.

## 10) Constraints

- Windows-only
- single executable
- deterministic output ordering
- no fabricated CLSIDs, paths, server kinds, or manifest declarations
- summary mode remains one line per COM command
- current `run` and `imports` behavior remains unchanged by this COM plan
