Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. 'C:\loadwhat\scripts\assert.ps1'
Assert-ContainerTestEnvironment

$classesRoot = 'Software\Classes'
$markerKey = 'LoadWhat.Container.ComTests'
$healthyDll = 'C:\loadwhat\fixtures\healthy\lwtest_com_server_x64.dll'
$brokenDll = 'C:\loadwhat\fixtures\broken\lwtest_com_server_dep_missing.dll'
$localServer = 'C:\loadwhat\fixtures\healthy\lwtest_com_localserver_x64.exe'

$ids = @{
    Basic = '{7F4D0001-4C57-4A54-9000-000000000001}'
    Override = '{7F4D0002-4C57-4A54-9000-000000000002}'
    View32 = '{7F4D0003-4C57-4A54-9000-000000000003}'
    LocalServer = '{7F4D0004-4C57-4A54-9000-000000000004}'
    CurVer = '{7F4D0005-4C57-4A54-9000-000000000005}'
    TreatAs = '{7F4D0006-4C57-4A54-9000-000000000006}'
    Audit = '{7F4D0007-4C57-4A54-9000-000000000007}'
    TargetHasDep = '{7F4D0008-4C57-4A54-9000-000000000008}'
    ServerHasDep = '{7F4D0009-4C57-4A54-9000-000000000009}'
    BrokenOverride = '{7F4D0010-4C57-4A54-9000-000000000010}'
    Wow64System32 = '{7F4D0011-4C57-4A54-9000-000000000011}'
}

function Open-ClassesRoot {
    param(
        [Parameter(Mandatory = $true)][Microsoft.Win32.RegistryHive]$Hive,
        [Parameter(Mandatory = $true)][Microsoft.Win32.RegistryView]$View
    )

    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($Hive, $View)
    try {
        return $base.CreateSubKey($classesRoot, $true)
    }
    finally {
        $base.Dispose()
    }
}

function Set-StringValue {
    param(
        [Parameter(Mandatory = $true)][Microsoft.Win32.RegistryHive]$Hive,
        [Parameter(Mandatory = $true)][Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory = $true)][string]$SubKey,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Name,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value
    )

    $root = Open-ClassesRoot -Hive $Hive -View $View
    try {
        $key = $root.CreateSubKey($SubKey, $true)
        try {
            $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String)
        }
        finally {
            $key.Dispose()
        }
    }
    finally {
        $root.Dispose()
    }
    Write-Host "LWTEST:REGISTRY_CREATE hive=$Hive view=$View key=$SubKey"
}

$hklm = [Microsoft.Win32.RegistryHive]::LocalMachine
$hkcu = [Microsoft.Win32.RegistryHive]::CurrentUser
$view64 = [Microsoft.Win32.RegistryView]::Registry64
$view32 = [Microsoft.Win32.RegistryView]::Registry32

Set-StringValue $hklm $view64 $markerKey '' 'loadwhat container registry fixture marker'

Set-StringValue $hklm $view64 "CLSID\$($ids.Basic)\InprocServer32" '' $healthyDll
Set-StringValue $hklm $view64 "CLSID\$($ids.Basic)\InprocServer32" 'ThreadingModel' 'Both'
Set-StringValue $hklm $view64 'LoadWhat.Container.ComTests.Basic\CLSID' '' $ids.Basic

Set-StringValue $hklm $view64 "CLSID\$($ids.Override)\InprocServer32" '' $healthyDll
Set-StringValue $hkcu $view64 "CLSID\$($ids.Override)\InprocServer32" '' $brokenDll

Set-StringValue $hklm $view32 "CLSID\$($ids.View32)\InprocServer32" '' $healthyDll

$quotedLocalServer = '"' + $localServer + '" --container-test'
Set-StringValue $hklm $view64 "CLSID\$($ids.LocalServer)\LocalServer32" '' $quotedLocalServer

Set-StringValue $hklm $view64 'LoadWhat.Container.ComTests.Versioned\CurVer' '' 'LoadWhat.Container.ComTests.Versioned.1'
Set-StringValue $hklm $view64 'LoadWhat.Container.ComTests.Versioned.1\CLSID' '' $ids.CurVer
Set-StringValue $hklm $view64 "CLSID\$($ids.CurVer)\InprocServer32" '' $healthyDll

Set-StringValue $hklm $view64 "CLSID\$($ids.TreatAs)\TreatAs" '' $ids.Basic
Set-StringValue $hklm $view64 "CLSID\$($ids.Audit)\InprocServer32" '' $healthyDll
Set-StringValue $hklm $view64 "CLSID\$($ids.TargetHasDep)\InprocServer32" '' 'C:\loadwhat\fixtures\context\server_missing_dep\lwtest_com_context_server.dll'
Set-StringValue $hklm $view64 "CLSID\$($ids.ServerHasDep)\InprocServer32" '' 'C:\loadwhat\fixtures\context\server_has_dep\lwtest_com_context_server.dll'
Set-StringValue $hklm $view64 "CLSID\$($ids.BrokenOverride)\InprocServer32" '' $healthyDll
Set-StringValue $hkcu $view64 "CLSID\$($ids.BrokenOverride)\InprocServer32" 'ThreadingModel' 'Both'
Set-StringValue $hklm $view32 "CLSID\$($ids.Wow64System32)\InprocServer32" '' 'C:\Windows\System32\kernel32.dll'
