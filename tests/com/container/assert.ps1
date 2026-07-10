Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-ContainerTestEnvironment {
    if ($env:LOADWHAT_CONTAINER_TESTS -ne '1' -or
        $env:LOADWHAT_CONTAINER_IMAGE -ne 'loadwhat-com-tests') {
        throw 'Container test markers are missing. Refusing to continue.'
    }

    $control = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control'
    if ($control.ContainerType -ne 2 -or -not (Test-Path -LiteralPath 'C:\WcSandboxState')) {
        throw 'Windows container identity checks failed. Refusing to continue.'
    }
}

function Invoke-LoadWhatCase {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int]$ExpectedExitCode,
        [Parameter(Mandatory = $true)][string]$ExpectedSummaryPattern
    )

    $lines = @(& 'C:\loadwhat\loadwhat.exe' @Arguments 2>&1 | ForEach-Object { "$_" })
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne $ExpectedExitCode) {
        throw "${Name}: expected exit $ExpectedExitCode, got $exitCode.`n$($lines -join "`n")"
    }
    if ($lines.Count -ne 1) {
        throw "${Name}: expected exactly one summary line, got $($lines.Count).`n$($lines -join "`n")"
    }
    if ($lines[0] -notmatch $ExpectedSummaryPattern) {
        throw "${Name}: output did not match $ExpectedSummaryPattern.`n$($lines[0])"
    }
    Write-Host "LWTEST:COM_CONTAINER PASS name=$Name"
}

function Invoke-LoadWhatTraceCase {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int]$ExpectedExitCode,
        [Parameter(Mandatory = $true)][string]$ExpectedLinePattern
    )

    $lines = @(& 'C:\loadwhat\loadwhat.exe' @Arguments 2>&1 | ForEach-Object { "$_" })
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne $ExpectedExitCode) {
        throw "${Name}: expected exit $ExpectedExitCode, got $exitCode.`n$($lines -join "`n")"
    }
    if (-not ($lines | Where-Object { $_ -match $ExpectedLinePattern })) {
        throw "${Name}: no line matched $ExpectedLinePattern.`n$($lines -join "`n")"
    }
    Write-Host "LWTEST:COM_CONTAINER PASS name=$Name"
}
