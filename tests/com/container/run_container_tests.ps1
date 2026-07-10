Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. 'C:\loadwhat\scripts\assert.ps1'
Assert-ContainerTestEnvironment
. 'C:\loadwhat\scripts\setup_registry.ps1'

Invoke-LoadWhatCase `
    -Name 'progid-hklm-64' `
    -Arguments @('com', 'progid', '--view', '64', 'LoadWhat.Container.ComTests.Basic') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*query_kind="progid" .*status="REGISTERED" .*hive="HKLM" .*view="64" .*server_status="OK"$'

Invoke-LoadWhatCase `
    -Name 'clsid-hklm-64' `
    -Arguments @('com', 'clsid', '--view', '64', '{7F4D0001-4C57-4A54-9000-000000000001}') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*query_kind="clsid" .*status="REGISTERED" .*hive="HKLM" .*view="64" .*server_status="OK"$'

Invoke-LoadWhatCase `
    -Name 'clsid-hklm-32-view' `
    -Arguments @('com', 'clsid', '--view', '32', '{7F4D0003-4C57-4A54-9000-000000000003}') `
    -ExpectedExitCode 10 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*status="REGISTERED" .*hive="HKLM" .*view="32" .*server_status="BITNESS_MISMATCH"$'

Invoke-LoadWhatCase `
    -Name 'hkcu-overrides-hklm' `
    -Arguments @('com', 'clsid', '--view', '64', '{7F4D0002-4C57-4A54-9000-000000000002}') `
    -ExpectedExitCode 10 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*status="REGISTERED" .*hive="HKCU" .*server_status="SERVER_DEPS_MISSING"$'

Invoke-LoadWhatCase `
    -Name 'curver-chain' `
    -Arguments @('com', 'progid', '--view', '64', 'LoadWhat.Container.ComTests.Versioned') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*status="REGISTERED" .*clsid="\{7F4D0005-4C57-4A54-9000-000000000005\}" .*server_status="OK"$'

Invoke-LoadWhatCase `
    -Name 'treatas-chain' `
    -Arguments @('com', 'clsid', '--view', '64', '{7F4D0006-4C57-4A54-9000-000000000006}') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*status="REGISTERED" .*clsid="\{7F4D0001-4C57-4A54-9000-000000000001\}" .*server_status="OK"$'

Invoke-LoadWhatCase `
    -Name 'localserver-command-line' `
    -Arguments @('com', 'clsid', '--view', '64', '{7F4D0004-4C57-4A54-9000-000000000004}') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_LOOKUP .*status="REGISTERED" .*server_kind="LocalServer32" .*server_status="OK"$'

Invoke-LoadWhatCase `
    -Name 'server-healthy' `
    -Arguments @('com', 'server', 'C:\loadwhat\fixtures\healthy\lwtest_com_server_x64.dll') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_SERVER .*status="OK" .*machine="x64" .*registrations=[1-9][0-9]*(?: .*)?$'

Invoke-LoadWhatCase `
    -Name 'server-dependency-missing' `
    -Arguments @('com', 'server', 'C:\loadwhat\fixtures\broken\lwtest_com_server_dep_missing.dll') `
    -ExpectedExitCode 10 `
    -ExpectedSummaryPattern '^COM_SERVER .*status="SERVER_DEPS_MISSING" .*machine="x64"'

Invoke-LoadWhatCase `
    -Name 'audit-registry-fallback' `
    -Arguments @('com', 'audit', 'C:\loadwhat\fixtures\context\target_missing_dep\lwtest_com_target_x64.exe', '{7F4D0007-4C57-4A54-9000-000000000007}') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_AUDIT .*target_machine="x64" .*source="registry" .*status="OK"'

Invoke-LoadWhatCase `
    -Name 'audit-sidecar-precedes-registry' `
    -Arguments @('com', 'audit', 'C:\loadwhat\fixtures\target\lwtest_com_target_x64.exe', '{7F4D0007-4C57-4A54-9000-000000000007}') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_AUDIT .*target_machine="x64" .*source="manifest" .*status="OK" .*server_path="C:\\\\loadwhat\\\\fixtures\\\\target\\\\lwtest_manifest_server.dll"$'

Invoke-LoadWhatCase `
    -Name 'x86-server-file' `
    -Arguments @('com', 'server', '--view', '32', 'C:\loadwhat\fixtures\x86\lwtest_com_server_x86.dll') `
    -ExpectedExitCode 0 `
    -ExpectedSummaryPattern '^COM_SERVER .*status="OK" .*machine="x86" .*views="32" .*registrations=0$'

Write-Host 'LWTEST:COM_CONTAINER PASS all'
