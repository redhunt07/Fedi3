Param(
  [ValidateSet("patch","minor","major")]
  [string]$Bump = "patch",
  [switch]$NoBump
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$appDir = Join-Path $repoRoot "app"
$pubspec = Join-Path $appDir "pubspec.yaml"

function Get-VersionLine($content) {
  $m = [regex]::Match($content, "version:\s*([0-9]+)\.([0-9]+)\.([0-9]+)(\+([0-9]+))?")
  if (-not $m.Success) { throw "Missing version in pubspec.yaml" }
  return $m
}

function Bump-Version($m, $kind) {
  $major = [int]$m.Groups[1].Value
  $minor = [int]$m.Groups[2].Value
  $patch = [int]$m.Groups[3].Value
  $build = 0
  if ($m.Groups[5].Success) { $build = [int]$m.Groups[5].Value }
  switch ($kind) {
    "major" { $major += 1; $minor = 0; $patch = 0 }
    "minor" { $minor += 1; $patch = 0 }
    "patch" { $patch += 1 }
  }
  $build += 1
  return "$major.$minor.$patch+$build"
}

$content = Get-Content $pubspec -Raw
$m = Get-VersionLine $content
$newVersion = $m.Value
if (-not $NoBump) {
  $newVersion = Bump-Version $m $Bump
  $content = $content -replace "version:\s*[0-9A-Za-z\.\+\-]+", "version: $newVersion"
  Set-Content $pubspec $content -Encoding UTF8
}

Write-Host "Version: $newVersion"

& (Join-Path $repoRoot "scripts\\build_core.ps1") release
& (Join-Path $repoRoot "scripts\\release_build_windows.ps1") -Version $newVersion
