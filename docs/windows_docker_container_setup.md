# Windows Docker Setup for COM Container Tests

This guide prepares a Windows machine to run the loadwhat COM container tests.
The goal is to run registry-mutating COM tests inside a Windows container so the
host COM registry is not used as the test fixture.

## Safety Boundary

These setup steps may change Docker Desktop and Windows container features on
the host. They must not create, update, or delete loadwhat COM test keys under:

- `HKCU:\Software\Classes`
- `HKLM:\Software\Classes`

The later container test runner is responsible for host-registry sentinel checks
before and after tests. Real COM test registrations must be created only inside
the Windows container.

## Required Host Prerequisites

- Windows 10/11 or Windows Server with Windows container support.
- Docker Desktop or another Docker-compatible runtime with Windows container
  support.
- Docker Desktop switched to Windows containers, not Linux containers.
- Windows optional feature `Containers` enabled.
- Windows optional feature `Microsoft-Hyper-V-All` enabled if using Hyper-V
  isolation.
- At least 20 GB free disk for Windows container base images and layers.
- Network access to pull `mcr.microsoft.com/windows/servercore:ltsc2022`.
- Rust stable toolchain.
- Visual Studio Build Tools or Visual Studio with MSVC x64 tools.

## Step 1: Check Whether PowerShell Is Elevated

Some setup steps require an elevated PowerShell. Check with:

```powershell
$isAdmin = ([Security.Principal.WindowsPrincipal] `
  [Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
"IsElevated=$isAdmin"
```

If this prints `IsElevated=False`, open PowerShell as Administrator for the
Windows feature and Docker repair/install steps below.

## Step 2: Enable Windows Container Features

Run in an elevated PowerShell:

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName Containers -All
```

If you want Hyper-V isolation support, also run:

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-All -All
```

Reboot if Windows requests it.

To verify after reboot:

```powershell
Get-WindowsOptionalFeature -Online -FeatureName Containers
Get-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-All
```

Expected:

- `Containers` is `Enabled`.
- `Microsoft-Hyper-V-All` is `Enabled` if Hyper-V isolation is needed.

## Step 3: Enable Windows Containers in Docker Desktop

If Docker Desktop was installed with Windows containers disabled, switching with
`DockerCli.exe -SwitchWindowsEngine` fails with:

```text
switching to windows engine: windows containers have been disabled for this installation
```

Check the persisted Docker Desktop install settings:

```powershell
Get-Content C:\ProgramData\DockerDesktop\install-settings.json -Raw
```

If it contains:

```json
{
  "noWindowsContainers": true
}
```

remove that persisted installer setting before repairing Docker Desktop:

```powershell
Remove-Item C:\ProgramData\DockerDesktop\install-settings.json -Force
```

After removing that setting, restart Docker Desktop and try the engine switch:

```powershell
& "C:\Program Files\Docker\Docker\DockerCli.exe" -Shutdown
Start-Process -FilePath "C:\Program Files\Docker\Docker\Docker Desktop.exe"
Start-Sleep -Seconds 15
& "C:\Program Files\Docker\Docker\DockerCli.exe" -SwitchWindowsEngine
docker info --format '{{.OSType}}'
```

Expected:

```text
windows
```

If Windows containers are still disabled, repair or reinstall Docker Desktop
with Windows container support. The `Docker Desktop Installer.exe` copy under
`C:\Program Files\Docker\Docker` may be only the installed stub and can fail with
`Missing package flag`. If that happens, use the full Docker Desktop installer
that was downloaded from Docker or installed by your package manager.

Run the full installer from an elevated PowerShell:

```powershell
& "C:\Path\To\Downloaded\Docker Desktop Installer.exe" install `
  --quiet `
  --accept-license `
  --backend=windows `
  --always-run-service
```

Notes:

- `--backend=windows` selects Windows containers as the default backend.
- `--always-run-service` lets regular users switch Windows containers without
  repeated administrator prompts.
- If Docker Desktop is not installed at that path, run the downloaded Docker
  Desktop installer with the same options.
- If the install log says `settings from previous installation exist: match the
  backend mode to that`, Docker reused old settings. Remove
  `C:\ProgramData\DockerDesktop\install-settings.json` and rerun the command.
- If the installed copy prints `Missing package flag`, use the downloaded full
  installer instead of the copy under `C:\Program Files\Docker\Docker`.

## Step 4: Start and Unpause Docker Desktop

Start Docker Desktop normally. If the CLI says:

```text
Docker Desktop is manually paused. Unpause it through the Whale menu or Dashboard.
```

open Docker Desktop and click Resume or Unpause. The command line helper does
not reliably unpause a manually paused Desktop instance.

## Step 5: Switch to Windows Containers

Run:

```powershell
& "C:\Program Files\Docker\Docker\DockerCli.exe" -SwitchWindowsEngine
```

Then verify:

```powershell
docker info --format '{{.OSType}}'
```

Expected:

```text
windows
```

If it prints `linux`, Docker is still in Linux-container mode.

## Step 6: Pull the Base Image

Run:

```powershell
docker pull mcr.microsoft.com/windows/servercore:ltsc2022
```

If process isolation fails later because the host Windows version does not match
the image closely enough, use Hyper-V isolation in the test runner or switch to
a Server Core tag that matches the host.

## Step 7: Verify the Existing loadwhat Toolchain

From the repository root:

```powershell
cargo test
cargo xtask test
```

Expected:

- `cargo test` passes.
- `cargo xtask test` passes.

These commands do not require Docker and should not mutate host COM registry
test keys.

## Step 8: Preflight Commands for Container Test Work

Before implementing or running `cargo xtask test-container`, collect:

```powershell
docker version
docker context ls
docker info --format '{{.OSType}}'
docker image inspect mcr.microsoft.com/windows/servercore:ltsc2022
```

Expected:

- Docker daemon is reachable.
- Active Docker engine reports `windows`.
- The Server Core image exists locally or can be pulled.

## Troubleshooting

### Docker Desktop is manually paused

Open Docker Desktop and click Resume or Unpause. After that, rerun:

```powershell
docker info --format '{{.OSType}}'
```

### Windows containers disabled for this installation

Remove `C:\ProgramData\DockerDesktop\install-settings.json`, restart Docker
Desktop, and try `DockerCli.exe -SwitchWindowsEngine` again. If it still fails,
run the full downloaded Docker Desktop installer from Step 3 in an elevated
PowerShell, then restart Docker Desktop.

### Docker installer prints `Missing package flag`

The installed copy of `Docker Desktop Installer.exe` is not always a full
installer package. Use the downloaded Docker Desktop installer instead.

### DISM or feature commands require elevation

Open PowerShell as Administrator. Non-elevated shells cannot inspect or enable
the required Windows optional features on some systems.

### Docker reports Linux containers

Run:

```powershell
& "C:\Program Files\Docker\Docker\DockerCli.exe" -SwitchWindowsEngine
```

If that reports Windows containers are disabled, repair or reinstall Docker
Desktop with `--backend=windows`.

### Base image pull is slow or fails

Windows Server Core images are large. Confirm disk space and network access,
then retry:

```powershell
docker pull mcr.microsoft.com/windows/servercore:ltsc2022
```

## Current Local Finding Template

Use this template when recording setup state in
`docs/com_docker_and_open_issues_plan.md`:

```text
- PowerShell elevated: yes/no
- Docker CLI installed: yes/no
- Docker Desktop running: yes/no
- Docker paused: yes/no
- Windows containers enabled in Docker install: yes/no
- Docker engine OSType: windows/linux/unreachable
- Containers optional feature: enabled/disabled/unknown
- Hyper-V optional feature: enabled/disabled/unknown
- Server Core image present: yes/no
```
