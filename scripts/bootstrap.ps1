Param(
  [switch]$CheckOnly
)

Write-Host "Fedi3 bootstrap (sviluppo)"

function Test-Cmd($name) {
  return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

$missing = @()
if (-not (Test-Cmd flutter)) { $missing += "flutter" }
if (-not (Test-Cmd rustup)) { $missing += "rustup" }

if ($missing.Count -gt 0) {
  Write-Host ""
  Write-Host "Mancano toolchain: $($missing -join ', ')"
  Write-Host "Installa:"
  Write-Host "- Flutter: https://docs.flutter.dev/get-started/install"
  Write-Host "- Rust:    https://rustup.rs"
  if ($CheckOnly) { exit 1 }
} else {
  Write-Host "OK: flutter e rustup trovati nel PATH."
}

if ($IsWindows) {
  if (-not (Test-Cmd link)) {
    Write-Host ""
    Write-Host "Nota Windows: `link.exe` (MSVC) non trovato nel PATH."
    Write-Host "Per compilare Rust con target MSVC installa Visual Studio Build Tools:"
    Write-Host "- https://visualstudio.microsoft.com/visual-cpp-build-tools/"
    Write-Host "Seleziona: 'Desktop development with C++' + Windows SDK."
    Write-Host "Poi esegui build da 'x64 Native Tools Command Prompt for VS'."
    if ($CheckOnly) { exit 1 }
  } else {
    Write-Host "OK: link.exe trovato."
  }
}
