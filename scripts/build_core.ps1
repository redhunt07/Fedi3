Param(
  [ValidateSet("debug","release")]
  [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

function Ensure-Cmd($name) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    throw "Missing command: $name"
  }
}

$cargo = (Get-Command cargo -ErrorAction SilentlyContinue)
if (-not $cargo) {
  $candidate = Join-Path $env:USERPROFILE ".cargo\\bin\\cargo.exe"
  if (Test-Path $candidate) {
    $cargo = Get-Command $candidate
  }
}
if (-not $cargo) {
  throw "Missing command: cargo (install rustup/cargo)"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$coreDir = Join-Path $repoRoot "crates\\fedi3_core"

$vsDevCmd = "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\Common7\\Tools\\VsDevCmd.bat"
if ($IsWindows -and (Test-Path $vsDevCmd)) {
  $cmd = @"
cd /d "$coreDir" && "$vsDevCmd" -arch=x64 -host_arch=x64 >nul && "$($cargo.Source)" build -p fedi3_core --$Profile
"@
  cmd /c $cmd | Write-Output
} else {
  Push-Location $coreDir
  try {
    & $cargo.Source build -p fedi3_core --$Profile
  } finally {
    Pop-Location
  }
}

$suffix = if ($Profile -eq "release") { "release" } else { "debug" }
$dll = Join-Path $repoRoot "target\\$suffix\\fedi3_core.dll"
if (-not (Test-Path $dll)) {
  Write-Host "Build completata, ma DLL non trovata in: $dll"
  exit 1
}

$appDir = Join-Path $repoRoot "app"

function Copy-CoreDll([string]$dest) {
  try {
    Copy-Item $dll $dest -Force
    Write-Host "Copiata: $dest"
  } catch {
    Write-Host "Errore copiando: $dest"
    Write-Host $_.Exception.Message
    Write-Host "Chiudi l'app Flutter (se in esecuzione) e riprova."
    throw
  }
}

Copy-CoreDll (Join-Path $appDir "fedi3_core.dll")

# Se esistono output dir di Flutter, copia anche lì (debug/run).
$candidates = @(
  (Join-Path $appDir "build\\windows\\x64\\runner\\Debug\\fedi3_core.dll"),
  (Join-Path $appDir "build\\windows\\x64\\runner\\Release\\fedi3_core.dll"),
  (Join-Path $appDir "build\\windows\\runner\\Debug\\fedi3_core.dll"),
  (Join-Path $appDir "build\\windows\\runner\\Release\\fedi3_core.dll")
)
foreach ($c in $candidates) {
  $dir = Split-Path -Parent $c
  if (Test-Path $dir) {
    Copy-CoreDll $c
  }
}
