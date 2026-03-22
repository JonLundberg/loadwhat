# loadwhat Diagrams

## 1. Phase Flow Diagram

```mermaid
flowchart TD
    START([loadwhat run target.exe]) --> NORM[Normalize target path]
    NORM -->|path not found| EXIT20[exit 20]
    NORM --> LS_SETUP[Setup loader-snaps]

    LS_SETUP --> PEB_TRY[Try PEB write\nNtGlobalFlag &= FLG_SHOW_LDR_SNAPS]
    PEB_TRY -->|success| PHASE_A
    PEB_TRY -->|WOW64 target| WOW64[exit 22]
    PEB_TRY -->|fail| IFEO_TRY[Try IFEO registry fallback]
    IFEO_TRY -->|success| PHASE_A
    IFEO_TRY -->|fail| PHASE_A

    PHASE_A[[Phase A — Debug Loop\nCreateProcessW DEBUG_ONLY_THIS_PROCESS\nCapture LOAD_DLL + DEBUG_STRING events]]
    PHASE_A --> RESTORE[Restore loader-snaps state]
    RESTORE --> ANALYZE{Analyze RunOutcome}

    ANALYZE -->|loader exception code\n0xC0000135 / 0xC000007B / etc.| PHASE_B
    ANALYZE -->|early-exit heuristic:\nexit≠0 AND elapsed<1500ms\nAND modules≤6| PHASE_B
    ANALYZE -->|success / timeout\nwith progress| SUCCESS_PATH

    PHASE_B[[Phase B — Static Import Diagnosis\nBFS PE import graph\nFixed v1 search order]]
    PHASE_B -->|StaticReport: missing_count>0\nor bad_image_count>0| EMIT_STATIC[Emit STATIC_MISSING\nor STATIC_BAD_IMAGE\nexit 10]
    PHASE_B -->|StaticReport: nothing found| PHASE_C_CHECK{loader-snaps\ncaptured?}

    PHASE_C_CHECK -->|no debug strings| SUCCESS_PATH
    PHASE_C_CHECK -->|yes| PHASE_C

    PHASE_C[[Phase C — Dynamic Inference\nScan debug strings\nScore and rank candidates]]
    PHASE_C -->|best candidate found| EMIT_DYN[Emit DYNAMIC_MISSING\nexit 10]
    PHASE_C -->|no viable candidate| SUCCESS_PATH

    SUCCESS_PATH[Emit SUCCESS status=0\nexit 0]
```

---

## 2. DLL Search Order

```mermaid
flowchart LR
    subgraph SAFE["SafeDllSearchMode = ON (default)"]
        direction TB
        S1[1. App directory\nexe parent dir]
        S2[2. System32\nC:/Windows/System32]
        S3[3. System16\nC:/Windows/System\nif exists]
        S4[4. Windows dir\nC:/Windows]
        S5[5. CWD\nif ≠ app dir]
        S6[6. PATH entries\nin order]
        S1 --> S2 --> S3 --> S4 --> S5 --> S6
    end

    subgraph UNSAFE["SafeDllSearchMode = OFF"]
        direction TB
        U1[1. App directory\nexe parent dir]
        U2[2. CWD\nif ≠ app dir]
        U3[3. System32\nC:/Windows/System32]
        U4[4. System16\nC:/Windows/System\nif exists]
        U5[5. Windows dir\nC:/Windows]
        U6[6. PATH entries\nin order]
        U1 --> U2 --> U3 --> U4 --> U5 --> U6
    end

    subgraph ABS["Absolute path in import"]
        direction TB
        A1[Check only the\nexact requested path]
        A2{exists?}
        A3{valid PE?}
        A4[Found]
        A5[BadImage]
        A6[Missing]
        A1 --> A2
        A2 -->|yes| A3
        A2 -->|no| A6
        A3 -->|yes| A4
        A3 -->|no| A5
    end

    NOTE["Each candidate classified:\nHIT = Found\nMISS = Missing\nBAD_IMAGE = BadImage\n\nFirst Found or BadImage stops search.\nMissing continues to next root."]
```

---

## 3. BFS Import Walk (Phase B)

```mermaid
flowchart TD
    ROOT([Root module\ntarget.exe]) --> VISITED{In visited\nset?}
    VISITED -->|yes| SKIP[Skip]
    VISITED -->|no| PARSE[pe::direct_imports\nparse import table]

    PARSE -->|Err| DIAG_FAIL[diagnose_static_imports\nreturns Err\nlogged as NOTE]

    PARSE -->|Ok imports| FOREACH[For each imported DLL name]

    FOREACH --> APISET{api-ms-win-*\nor ext-ms-win-*?}
    APISET -->|yes| SKIP_API[Skip — API set\nresolved by OS]
    APISET -->|no| DEPTH0{depth == 0 AND\nin runtime_loaded?}

    DEPTH0 -->|yes| RUNTIME_OBS[Mark RUNTIME_OBSERVED\nqueue if unvisited]
    DEPTH0 -->|no| RESOLVE[resolve_dll via\nSearchContext]

    RESOLVE --> KIND{ResolutionKind}

    KIND -->|Found| EMIT_FOUND[Emit STATIC_FOUND\nqueue for recursion\nif unvisited]
    KIND -->|Missing| EMIT_MISS[Emit STATIC_MISSING\nincrement missing_count\nrecord FirstIssue]
    KIND -->|BadImage| EMIT_BAD[Emit STATIC_BAD_IMAGE\nincrement bad_image_count\nrecord FirstIssue\nNO recursion]

    EMIT_FOUND --> QUEUE[Add to BFS queue\nwith depth+1]
    QUEUE --> NEXT[Next item in queue]
    NEXT --> VISITED

    EMIT_MISS --> STOPCHECK{FailuresOnly or\nSummaryOnly mode?}
    STOPCHECK -->|yes| STOP_EARLY[Stop BFS]
    STOPCHECK -->|no| NEXT

    FIRST_ISSUE["FirstIssue selection:\nlowest depth\nthen via alphabetically\nthen dll alphabetically"]
```

---

## 4. Dynamic Candidate Scoring (Phase C)

```mermaid
flowchart TD
    STRINGS([Captured debug strings\nfrom Phase A]) --> SCAN[Scan each string\nlowercase for matching]

    SCAN --> T100["Score 100 — UnableToLoadDll\n'ldrpprocesswork - error: unable to load dll'"]
    SCAN --> T95["Score 95 — UnableToLoadDll\n'- error: unable to load dll'"]
    SCAN --> T92["Score 92 — InitializeProcessFailure\n'ldrpinitializenode - error: init routine'\n+ 'dll_process_attach'"]
    SCAN --> T90["Score 90 — InitializeProcessFailure\n'walking the import tables'"]
    SCAN --> T85["Score 85 — InitializeProcessFailure\n'process initialization failed'\nor '_ldrpinitialize - error'"]
    SCAN --> T80["Score 80 — LoadDllFailed\n'ldrloaddll' + 'failed'"]
    SCAN --> T70["Score 70 — SearchPathFailure\n'ldrpsearchpath - return'\n+ loader failure status code"]
    SCAN --> T60["Score 60 — Other\nloader failure code\n+ (failed or error) + loader context"]
    SCAN --> T0["Score 0 — Other\nno match"]

    T100 & T95 & T92 & T90 & T85 & T80 & T70 & T60 & T0 --> FILTER

    FILTER["Filter: discard if dll\nappears in RuntimeLoaded\nAFTER this event index"]

    FILTER --> SORT["Sort candidates — first match wins:\n1. Kind  UnableToLoadDll › Init › LoadDll › SearchPath › Other\n2. Score  higher wins\n3. app_local_hint  true › false\n4. framework_or_os_hint  false › true\n5. thread_correlated  true › false\n6. event_idx  ascending earliest first\n7. dll name  alphabetical\n8. tid  ascending"]

    SORT --> REASON["Classify reason:\n0xC0000135 / 0x8007007E → NOT_FOUND\n0xC000007B / 0x800700C1 / 0xC000012F → BAD_IMAGE\n'not found' text → NOT_FOUND\n'bad image' text → BAD_IMAGE\ndefault → OTHER"]

    REASON --> OUT([DYNAMIC_MISSING\ndll=… reason=… status=0x…])
```

---

## 7. Loader-Snaps Enable Sequence

```mermaid
sequenceDiagram
    participant LC as loadwhat
    participant PEB as Child Process PEB
    participant REG as IFEO Registry
    participant OS as Windows Loader

    LC->>LC: Check target architecture
    alt WOW64 target
        LC-->>LC: emit NOTE wow64-target-unsupported
        LC-->>LC: exit 22
    end

    Note over LC: Attempt 1 — PEB direct write

    LC->>PEB: NtQueryInformationProcess\n(ProcessBasicInformation)
    PEB-->>LC: PebBaseAddress

    LC->>PEB: ReadProcessMemory\n(PebBaseAddress + 0xBC)
    PEB-->>LC: current NtGlobalFlag

    LC->>PEB: WriteProcessMemory\n(NtGlobalFlag |= FLG_SHOW_LDR_SNAPS 0x02)

    alt PEB write succeeds
        Note over LC,OS: loader-snaps active via PEB
        LC->>LC: emit NOTE detail=peb-ntglobalflag (verbose)
    else PEB write fails
        LC-->>LC: emit NOTE detail=peb-enable-failed code=0x… (trace)

        Note over LC: Attempt 2 — IFEO registry fallback

        LC->>REG: RegOpenKeyExW / RegCreateKeyExW\nIFEO\ImageName\GlobalFlag
        REG-->>LC: key handle

        LC->>REG: RegSetValueExW\nGlobalFlag = FLG_SHOW_LDR_SNAPS

        alt IFEO write succeeds
            Note over LC,OS: loader-snaps active via registry
        else IFEO write fails
            LC-->>LC: emit NOTE detail=enable-failed code=0x… (trace)
            LC-->>LC: exit 21
        end
    end

    LC->>OS: CreateProcessW target.exe\nDEBUG_ONLY_THIS_PROCESS
    OS->>OS: Loader reads NtGlobalFlag\nemits debug strings to debugger

    LC->>LC: Run debug loop\ncapture OUTPUT_DEBUG_STRING events

    Note over LC: Restore phase — always runs

    alt PEB was used
        LC->>PEB: WriteProcessMemory\n(restore original NtGlobalFlag)
    else IFEO was used
        LC->>REG: RegSetValueExW (restore original)\nor RegDeleteValueW if was absent
        alt restore fails
            LC-->>LC: emit NOTE detail=restore-failed code=0x… (trace)
        end
    end
```
