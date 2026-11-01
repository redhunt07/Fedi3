Param(
  [string]$Version = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$appDir = Join-Path $repoRoot "app"
$distDir = Join-Path $repoRoot "dist"

if ($Version -ne "") {
  $pubspec = Join-Path $appDir "pubspec.yaml"
  $content = Get-Content $pubspec -Raw
  $content = $content -replace "version:\s*[0-9A-Za-z\.\+\-]+", "version: $Version"
  Set-Content $pubspec $content -Encoding UTF8
}

Push-Location $appDir
flutter clean
flutter pub get
flutter build windows --release
Pop-Location

if (!(Test-Path $distDir)) { New-Item -ItemType Directory -Path $distDir | Out-Null }

$coreDll = Join-Path $repoRoot "target\\release\\fedi3_core.dll"
$releaseDir = Join-Path $appDir "build\\windows\\x64\\runner\\Release"
$coreDest = Join-Path $releaseDir "fedi3_core.dll"
if (!(Test-Path $coreDll)) { throw "fedi3_core.dll not found at $coreDll" }
if (!(Test-Path $releaseDir)) { throw "Release dir not found at $releaseDir" }
Copy-Item $coreDll $coreDest -Force

$exePath = Join-Path $releaseDir "Fedi3.exe"
if (!(Test-Path $exePath)) { throw "Fedi3.exe not found at $exePath" }

Compress-Archive -Path (Join-Path $releaseDir "*") -DestinationPath (Join-Path $distDir "Fedi3-windows-x64.zip") -Force

$zipPath = Join-Path $distDir "Fedi3-windows-x64.zip"
function Write-Checksums($dir) {
  $lines = @()
  Get-ChildItem $dir -File | ForEach-Object {
    if ($_.Name -eq "checksums.txt") { return }
    $hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLower()
    $lines += "$hash  $($_.Name)"
  }
  $lines = $lines | Sort-Object
  Set-Content -Path (Join-Path $dir "checksums.txt") -Value $lines -Encoding ASCII
}

Write-Checksums $distDir

Write-Host "Windows update ready in $distDir"
