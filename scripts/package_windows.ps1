$ErrorActionPreference = "Stop"

Write-Host "[1/4] Setting up Python venv..."
if (-not (Test-Path ".venv")) {
    python -m venv .venv
}
.\.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
pip install maturin PySide6 jinja2 structlog zstandard pyinstaller

Write-Host "[2/4] Building Rust CLI (recoverx-cli.exe) & Python bridge..."
cargo build --release -p recoverx_cli
maturin develop --release

Write-Host "[3/4] Building single-file offline RECOVER-X.exe (with UAC Admin manifest)..."
if (Test-Path "dist\RECOVER-X.exe") { Remove-Item -Force "dist\RECOVER-X.exe" }
pyinstaller `
  --noconfirm `
  --clean `
  --onefile `
  --windowed `
  --uac-admin `
  --name RECOVER-X `
  --hidden-import _recoverx_py `
  --add-data "config;config" `
  --exclude-module PySide6.QtWebEngineCore `
  --exclude-module PySide6.QtWebEngineWidgets `
  --exclude-module PySide6.Qt3DCore `
  --exclude-module PySide6.QtQuick `
  --exclude-module PySide6.QtQml `
  --exclude-module PySide6.QtMultimedia `
  --exclude-module PySide6.QtPdf `
  --exclude-module PySide6.QtCharts `
  app/main.py

Write-Host "[4/4] Assembling portable Windows folder..."
$outDir = "dist\RECOVER-X-Portable-Windows"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Copy-Item -Force "dist\RECOVER-X.exe" "$outDir\RECOVER-X.exe"
Copy-Item -Force "target\release\recoverx-cli.exe" "$outDir\recoverx-cli.exe"

Write-Host ""
Write-Host "Done! Standalone offline Windows executable ready at:"
Write-Host "  $outDir\RECOVER-X.exe"
