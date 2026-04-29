# loadwhat Manual Spot-Check Test Plan

This plan is for a human tester, or a local LLM assistant guiding a human tester, on a separate Windows x64 computer. The goal is to spot-check that `loadwhat` builds and behaves like a DLL-loading focused debugger against real programs outside the repository.

Authoritative behavior is defined by `docs/loadwhat_spec_v1.md`. If this plan and the spec disagree, follow the spec.

## Tester Instructions

Act like a careful QA operator. Run the commands in order, save command output, record exit codes, and mark each case `PASS`, `FAIL`, or `BLOCKED`.

Do not treat this as a full certification suite. This is a practical spot check.

Important rules:

- Use a Windows x64 machine.
- Run from PowerShell.
- Do not run from inside the repository's test harness unless the step explicitly says so.
- Public output must be line-oriented token output: `TOKEN key=value key=value ...`.
- For `loadwhat run` default summary mode, expect exactly one public output line when a public result is emitted.
- Do not depend on `LWTEST:*` lines. Those are internal harness output.
- Do not require byte-identical verbose runtime traces across machines.

## Required Tools

Install these before starting:

- Git
- Rust stable toolchain
- Visual Studio Build Tools or full Visual Studio with MSVC x64 tools

Optional tools:

- `dumpbin`
- Sysinternals tools
- Dependencies.exe

## Result Folder

Create a folder for logs:

```powershell
$ResultRoot = "$env:USERPROFILE\Desktop\loadwhat-spotcheck-results"
New-Item -ItemType Directory -Force $ResultRoot | Out-Null
```

For every command, save stdout/stderr when practical:

```powershell
<command> *> "$ResultRoot\some-useful-name.txt"
$LASTEXITCODE | Out-File "$ResultRoot\some-useful-name.exit.txt"
```

## 1. Clone and Build

Replace `<repo-url>` with the project URL.

```powershell
cd "$env:USERPROFILE\source"
git clone <repo-url> loadwhat
cd loadwhat
cargo build --release *> "$ResultRoot\build-release.txt"
$LASTEXITCODE | Out-File "$ResultRoot\build-release.exit.txt"
```

Expected:

- exit code `0`
- `target\release\loadwhat.exe` exists

Check:

```powershell
Test-Path .\target\release\loadwhat.exe
```

Define:

```powershell
$lw = (Resolve-Path .\target\release\loadwhat.exe).Path
```

## 2. Help and Basic CLI

Run:

```powershell
& $lw --help *> "$ResultRoot\help.txt"
$LASTEXITCODE | Out-File "$ResultRoot\help.exit.txt"

& $lw run *> "$ResultRoot\run-missing-args.txt"
$LASTEXITCODE | Out-File "$ResultRoot\run-missing-args.exit.txt"

& $lw imports *> "$ResultRoot\imports-missing-args.txt"
$LASTEXITCODE | Out-File "$ResultRoot\imports-missing-args.exit.txt"
```

Expected:

- `--help` succeeds.
- missing arguments produce a usage failure, expected exit code `20`.
- no panic text or Rust stack dump.

## 3. Optional Internal Preflight

This step checks the repository's own harness. It is useful, but this manual plan should still continue if the local computer lacks MSBuild support.

```powershell
cargo xtask test *> "$ResultRoot\cargo-xtask-test.txt"
$LASTEXITCODE | Out-File "$ResultRoot\cargo-xtask-test.exit.txt"
```

Expected:

- exit code `0`

If this fails because MSBuild is missing, mark this step `BLOCKED` and continue.

## 4. Good Windows Binaries

These cases check real system programs that should normally start successfully.

Targets:

```powershell
$GoodTargets = @(
  "C:\Windows\System32\notepad.exe",
  "C:\Windows\System32\cmd.exe",
  "C:\Windows\System32\where.exe",
  "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
)
```

Run:

```powershell
foreach ($target in $GoodTargets) {
  $name = [IO.Path]::GetFileNameWithoutExtension($target)

  & $lw imports $target *> "$ResultRoot\good-$name-imports.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\good-$name-imports.exit.txt"

  & $lw run $target *> "$ResultRoot\good-$name-run-summary.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\good-$name-run-summary.exit.txt"

  & $lw run --trace $target *> "$ResultRoot\good-$name-run-trace.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\good-$name-run-trace.exit.txt"

  & $lw run -v --summary $target *> "$ResultRoot\good-$name-run-verbose-then-summary.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\good-$name-run-verbose-then-summary.exit.txt"
}
```

Expected:

- `imports` should not report `STATIC_MISSING` or `STATIC_BAD_IMAGE` for normal system binaries.
- default `run` summary should emit exactly one public line, usually `SUCCESS status=0`.
- `run --trace` may emit trace lines but must remain token-shaped.
- `run -v --summary` should behave like summary mode because the later `--summary` wins.

## 5. Target Argument Passing

Run:

```powershell
& $lw run C:\Windows\System32\cmd.exe /c echo hello *> "$ResultRoot\cmd-arg-pass.txt"
$LASTEXITCODE | Out-File "$ResultRoot\cmd-arg-pass.exit.txt"

& $lw run C:\Windows\System32\cmd.exe --trace *> "$ResultRoot\cmd-target-receives-trace.txt"
$LASTEXITCODE | Out-File "$ResultRoot\cmd-target-receives-trace.exit.txt"
```

Expected:

- Arguments after the target are passed to the target process.
- `--trace` after `cmd.exe` must not be interpreted as a `loadwhat` option.

## 6. Determinism Spot Check

Run repeated static scans:

```powershell
1..5 | ForEach-Object {
  & $lw imports C:\Windows\System32\notepad.exe *> "$ResultRoot\determinism-imports-notepad-$_.txt"
}
```

Expected:

- The five `imports` outputs should be effectively identical.
- `SEARCH_PATH` order, if present, should not change between runs.

Do not require verbose runtime output to be byte-identical.

## 7. External Fixture Setup

This section builds tiny QA programs outside the repository. These are not project tests; they are external test samples.

Create a QA folder:

```powershell
$QaRoot = "$env:TEMP\loadwhat-manual-qa"
$Src = "$QaRoot\src"
$Bin = "$QaRoot\bin"
New-Item -ItemType Directory -Force $Src,$Bin | Out-Null
```

Open a "x64 Native Tools Command Prompt for VS", or initialize MSVC from PowerShell if available. The following commands require `cl.exe`.

Check:

```powershell
cl
```

If `cl.exe` is unavailable, mark sections 8-11 as `BLOCKED`.

## 8. Dynamic Missing DLL Fixture

Create source:

```powershell
@'
#include <windows.h>

int main() {
    LoadLibraryW(L"loadwhat_qa_definitely_missing.dll");
    return 0;
}
'@ | Out-File -Encoding ascii "$Src\dynamic_missing.cpp"
```

Build:

```powershell
cl /nologo /EHsc /Fe"$Bin\dynamic_missing.exe" "$Src\dynamic_missing.cpp"
```

Run:

```powershell
& $lw run "$Bin\dynamic_missing.exe" *> "$ResultRoot\fixture-dynamic-missing-summary.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-dynamic-missing-summary.exit.txt"

& $lw run --trace "$Bin\dynamic_missing.exe" *> "$ResultRoot\fixture-dynamic-missing-trace.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-dynamic-missing-trace.exit.txt"

& $lw run --no-loader-snaps "$Bin\dynamic_missing.exe" *> "$ResultRoot\fixture-dynamic-missing-no-loader-snaps.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-dynamic-missing-no-loader-snaps.exit.txt"
```

Expected:

- default summary should report `DYNAMIC_MISSING dll="loadwhat_qa_definitely_missing.dll" reason="NOT_FOUND"`.
- exit code should be `10` when the dynamic missing is diagnosed.
- `--trace` should include token-shaped trace output.
- `--no-loader-snaps` should not infer this dynamic missing case.

## 9. Direct Static Missing DLL Fixture

Create a DLL and host, then delete the DLL so the import is missing at runtime.

```powershell
@'
extern "C" __declspec(dllexport) int qa_missing_value() {
    return 42;
}
'@ | Out-File -Encoding ascii "$Src\qa_missing.cpp"

@'
extern "C" __declspec(dllimport) int qa_missing_value();

int main() {
    return qa_missing_value();
}
'@ | Out-File -Encoding ascii "$Src\static_missing_host.cpp"
```

Build:

```powershell
cl /nologo /LD /Fe"$Bin\qa_missing.dll" "$Src\qa_missing.cpp" /link /IMPLIB:"$Bin\qa_missing.lib"
cl /nologo /EHsc /Fe"$Bin\static_missing_host.exe" "$Src\static_missing_host.cpp" "$Bin\qa_missing.lib"
Remove-Item "$Bin\qa_missing.dll"
```

Run:

```powershell
& $lw run "$Bin\static_missing_host.exe" *> "$ResultRoot\fixture-static-missing-summary.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-static-missing-summary.exit.txt"

& $lw run --trace "$Bin\static_missing_host.exe" *> "$ResultRoot\fixture-static-missing-trace.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-static-missing-trace.exit.txt"

& $lw imports "$Bin\static_missing_host.exe" *> "$ResultRoot\fixture-static-missing-imports.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-static-missing-imports.exit.txt"
```

Expected:

- output reports `STATIC_MISSING dll="qa_missing.dll"`.
- exit code should be `10`.
- trace/imports output should include `SEARCH_ORDER` and `SEARCH_PATH` lines for the missing DLL.

## 10. Transitive Static Missing DLL Fixture

Create this dependency chain:

```text
transitive_host.exe imports qa_a.dll
qa_a.dll imports qa_b.dll
qa_b.dll is removed
```

Source:

```powershell
@'
extern "C" __declspec(dllexport) int qa_b_value() {
    return 7;
}
'@ | Out-File -Encoding ascii "$Src\qa_b.cpp"

@'
extern "C" __declspec(dllimport) int qa_b_value();
extern "C" __declspec(dllexport) int qa_a_value() {
    return qa_b_value();
}
'@ | Out-File -Encoding ascii "$Src\qa_a.cpp"

@'
extern "C" __declspec(dllimport) int qa_a_value();

int main() {
    return qa_a_value();
}
'@ | Out-File -Encoding ascii "$Src\transitive_host.cpp"
```

Build:

```powershell
cl /nologo /LD /Fe"$Bin\qa_b.dll" "$Src\qa_b.cpp" /link /IMPLIB:"$Bin\qa_b.lib"
cl /nologo /LD /Fe"$Bin\qa_a.dll" "$Src\qa_a.cpp" "$Bin\qa_b.lib" /link /IMPLIB:"$Bin\qa_a.lib"
cl /nologo /EHsc /Fe"$Bin\transitive_host.exe" "$Src\transitive_host.cpp" "$Bin\qa_a.lib"
Remove-Item "$Bin\qa_b.dll"
```

Run:

```powershell
& $lw imports "$Bin\transitive_host.exe" *> "$ResultRoot\fixture-transitive-missing-imports.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-transitive-missing-imports.exit.txt"

& $lw run "$Bin\transitive_host.exe" *> "$ResultRoot\fixture-transitive-missing-run.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-transitive-missing-run.exit.txt"
```

Expected:

- output reports `STATIC_MISSING dll="qa_b.dll"`.
- output should include `via="qa_a.dll"` and `depth=2` where the mode emits those fields.
- exit code should be `10`.

## 11. Bad Image Fixture

This checks wrong-architecture DLL handling. It requires x86 MSVC tools in addition to x64 tools.

If x86 tools are unavailable, mark this section `BLOCKED`.

Build an x86 DLL named `qa_bad.dll`, then link a 64-bit host against its import library or place the x86 DLL where a 64-bit host expects it. The exact command varies by local Visual Studio setup.

Run:

```powershell
& $lw run "$Bin\bad_image_host.exe" *> "$ResultRoot\fixture-bad-image-run.txt"
$LASTEXITCODE | Out-File "$ResultRoot\fixture-bad-image-run.exit.txt"
```

Expected:

- output reports `STATIC_BAD_IMAGE`.
- exit code should be `10`.
- no panic or stack dump.

If this setup takes too long, skip it. Sections 8-10 are higher priority.

## 12. Search Order Spot Check

Use the `static_missing_host.exe` from section 9.

Create controlled directories:

```powershell
$SearchRoot = "$env:TEMP\loadwhat-search-qa"
$AppDir = "$SearchRoot\app"
$CwdDir = "$SearchRoot\cwd"
$PathOne = "$SearchRoot\path-one"
$PathTwo = "$SearchRoot\path-two"
New-Item -ItemType Directory -Force $AppDir,$CwdDir,$PathOne,$PathTwo | Out-Null
Copy-Item "$Bin\static_missing_host.exe" "$AppDir\static_missing_host.exe" -Force
```

Run with controlled PATH:

```powershell
$OldPath = $env:PATH
$OldLocation = Get-Location
$env:PATH = "$PathOne;$PathTwo;$OldPath"
Set-Location $CwdDir

& $lw run --trace "$AppDir\static_missing_host.exe" *> "$ResultRoot\search-order-static-missing.txt"
$LASTEXITCODE | Out-File "$ResultRoot\search-order-static-missing.exit.txt"

Set-Location $OldLocation
$env:PATH = $OldPath
```

Expected:

- `SEARCH_PATH` lines should be in the fixed v1 order:
  1. application directory
  2. system directory
  3. 16-bit system directory, if it exists
  4. Windows directory
  5. current directory, position depending on SafeDllSearchMode
  6. PATH entries in order
- `$PathOne` appears before `$PathTwo`.

## 13. Real Third-Party App Smoke Tests

Pick installed programs outside the repository. Use any that exist on this machine:

- 7-Zip
- Git for Windows
- VS Code
- Notepad++
- Python
- Node.js
- Sysinternals tools

Example commands:

```powershell
$RealTargets = @(
  "C:\Program Files\7-Zip\7z.exe",
  "C:\Program Files\Git\cmd\git.exe",
  "C:\Program Files\nodejs\node.exe",
  "C:\Program Files\Python312\python.exe",
  "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe"
) | Where-Object { Test-Path $_ }

foreach ($target in $RealTargets) {
  $name = ([IO.Path]::GetFileNameWithoutExtension($target)) -replace '[^A-Za-z0-9_-]', '_'

  & $lw imports $target *> "$ResultRoot\real-$name-imports.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\real-$name-imports.exit.txt"

  & $lw run --trace $target *> "$ResultRoot\real-$name-run-trace.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\real-$name-run-trace.exit.txt"
}
```

Expected:

- `loadwhat` should not crash.
- output should remain token-shaped.
- summary behavior should remain one public result line where applicable.
- Do not automatically fail `loadwhat` just because an app uses behavior outside v1, such as SxS, KnownDLLs, `SetDllDirectory`, `AddDllDirectory`, package search rules, or special `LoadLibraryEx` flags.

## 14. Edge Inputs

Create a non-PE file:

```powershell
"not a pe file" | Out-File -Encoding ascii "$QaRoot\not-a-pe.txt"
```

Run:

```powershell
& $lw imports "$QaRoot\not-a-pe.txt" *> "$ResultRoot\edge-not-a-pe-imports.txt"
$LASTEXITCODE | Out-File "$ResultRoot\edge-not-a-pe-imports.exit.txt"

& $lw run "$QaRoot\does-not-exist.exe" *> "$ResultRoot\edge-missing-target-run.txt"
$LASTEXITCODE | Out-File "$ResultRoot\edge-missing-target-run.exit.txt"

if (Test-Path "C:\Windows\SysWOW64\notepad.exe") {
  & $lw run "C:\Windows\SysWOW64\notepad.exe" *> "$ResultRoot\edge-wow64-run.txt"
  $LASTEXITCODE | Out-File "$ResultRoot\edge-wow64-run.exit.txt"
}
```

Expected:

- no panic or stack dump.
- missing target should fail cleanly.
- WOW64 target support is out of scope for v1; expected exit code is `22` if detected as unsupported.

## 15. Pass/Fail Summary

Create a final file:

```powershell
@"
# loadwhat spot-check summary

Machine:
Windows version:
Rust version:
Visual Studio/MSVC version:
Repository commit:

## Results

- Build:
- Help/basic CLI:
- Internal preflight:
- Good Windows binaries:
- Argument passing:
- Determinism:
- Dynamic missing fixture:
- Static missing fixture:
- Transitive missing fixture:
- Bad image fixture:
- Search order:
- Real third-party apps:
- Edge inputs:

## Failures

List command, exit code, expected result, actual result, and log file path.

## Notes

List anything environment-specific, such as antivirus blocking, Smart App Control, missing MSBuild, missing x86 tools, or apps not installed.
"@ | Out-File -Encoding utf8 "$ResultRoot\SUMMARY.md"
```

## High-Priority Failures

Treat these as important bugs:

- `cargo build --release` fails on a normal Windows x64 dev machine.
- `loadwhat.exe` panics or prints a Rust stack dump.
- default `run` summary emits multiple public result lines.
- known static missing fixture does not produce `STATIC_MISSING`.
- known transitive missing fixture does not identify the missing transitive DLL.
- known dynamic missing fixture does not produce `DYNAMIC_MISSING` when loader-snaps are enabled.
- `--no-loader-snaps` still reports the simple dynamic `LoadLibrary` missing case.
- output is not line-oriented token output.
- exit code is not `10` for diagnosed missing/bad-image cases.
- loader-snaps setup leaves persistent IFEO registry state behind.

## Known Non-Failures

Do not fail the run only for these:

- `cargo xtask test` is blocked because Visual Studio/MSBuild is not installed.
- bad-image fixture is blocked because x86 MSVC tools are unavailable.
- verbose runtime token order/count differs across machines.
- a third-party app uses v1-unmodeled loader behavior.
- a real app has incidental dynamic loader noise, as long as `loadwhat` output remains truthful and token-shaped.
