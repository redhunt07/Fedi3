param(
  [switch]$UpdateOnly
)

$ErrorActionPreference = "Stop"

$repoUrl = "https://github.com/redhunt07/Fedi3.git"
$repoDir = Join-Path $env:LOCALAPPDATA "Fedi3\src"
$installDir = Join-Path $env:LOCALAPPDATA "Fedi3\app"
$coreServiceExe = Join-Path $installDir "fedi3_core_service.exe"

function Test-Command {
  param([string]$Name)
  return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Ensure-Winget {
  if (-not (Test-Command "winget")) {
    Write-Error "winget not found. Install App Installer from Microsoft Store."
  }
}

function Ensure-Dependencies {
  if ($UpdateOnly) {
    return
  }
  Ensure-Winget

  if (-not (Test-Command "git")) {
    winget install --id Git.Git -e --accept-package-agreements --accept-source-agreements
  }
  if (-not (Test-Command "rustup")) {
    winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
  }
  if (-not (Test-Command "flutter")) {
    try {
      winget install --id Google.Flutter -e --accept-package-agreements --accept-source-agreements
    } catch {
      Write-Host "Flutter not installed. Install Flutter and ensure it is in PATH."
      throw
    }
  }
  if (-not (Get-Command "cl.exe" -ErrorAction SilentlyContinue)) {
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements `
      --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools"
  }
}

function Update-Repo {
  if (-not (Test-Path $repoDir)) {
    New-Item -ItemType Directory -Force -Path $repoDir | Out-Null
    git clone $repoUrl $repoDir
  } else {
    git -C $repoDir fetch --all --prune
    git -C $repoDir pull --ff-only
  }
}

function Build-Core {
  & "$repoDir\scripts\build_core.ps1" -Profile release
}

function Build-App {
  Push-Location "$repoDir\app"
  try {
    $env:HTTP_PROXY = ""
    $env:HTTPS_PROXY = ""
    $env:ALL_PROXY = ""
    flutter pub get
    flutter build windows --release
  } finally {
    Pop-Location
  }
}

function Install-App {
  $releaseDir = "$repoDir\app\build\windows\x64\runner\Release"
  if (-not (Test-Path $releaseDir)) {
    throw "Release build not found: $releaseDir"
  }
  New-Item -ItemType Directory -Force -Path $installDir | Out-Null
  Copy-Item "$releaseDir\*" $installDir -Recurse -Force
  Write-Host "Installed to $installDir"
}

function Install-CoreService {
  $serviceBin = Join-Path $repoDir "target\release\fedi3_core_service.exe"
  if (-not (Test-Path $serviceBin)) {
    Write-Host "Core service binary not found: $serviceBin"
    return
  }
  $taskName = "Fedi3 Core"
  try {
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue | Out-Null
  } catch {}
  try {
    schtasks /End /TN $taskName | Out-Null
  } catch {}
  try {
    Get-Process -Name "fedi3_core_service" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  } catch {}
  Start-Sleep -Milliseconds 800
  function Test-FileLocked {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $false }
    try {
      $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
      $fs.Close()
      return $false
    } catch {
      return $true
    }
  }
  $waitMs = 0
  while (Test-FileLocked $coreServiceExe -and $waitMs -lt 5000) {
    Start-Sleep -Milliseconds 500
    $waitMs += 500
  }
  $copied = $false
  for ($i = 0; $i -lt 3; $i++) {
    try {
      Copy-Item $serviceBin $coreServiceExe -Force
      $copied = $true
      break
    } catch {
      Start-Sleep -Milliseconds 700
    }
  }
  if (-not $copied) {
    throw "Impossibile aggiornare fedi3_core_service.exe (file in uso). Chiudi l'app e riprova."
  }

  $configBase = if ($env:APPDATA) { $env:APPDATA } elseif ($env:USERPROFILE) { $env:USERPROFILE } else { "." }
  $configPath = Join-Path $configBase "Fedi3\config.json"
  $action = New-ScheduledTaskAction -Execute $coreServiceExe -Argument "--config `"$configPath`""
  $trigger = New-ScheduledTaskTrigger -AtLogOn
  $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType S4U -RunLevel Limited
  Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
  Start-ScheduledTask -TaskName $taskName | Out-Null
  Write-Host "Core service scheduled task installed: $taskName"
}

function Install-Fedi3 {
  param([switch]$UpdateOnly)
  Ensure-Dependencies
  Update-Repo
  Build-Core
  Build-App
  Install-App
  Install-CoreService
  Write-Host "Done. Launch: $installDir\fedi3.exe"
}

if ($MyInvocation.InvocationName -ne '.') {
  Install-Fedi3 -UpdateOnly:$UpdateOnly
}
