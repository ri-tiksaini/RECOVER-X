#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
source .venv/bin/activate

echo "[1/4] Installing PyInstaller..."
pip install -q pyinstaller

echo "[2/4] Building release Rust CLI & Python bridge..."
cargo build --release -p recoverx_cli
maturin develop --release

echo "[3/4] Building portable Linux bundle (--onedir)..."
rm -rf build dist/RECOVER-X dist/RECOVER-X-Portable-Linux
pyinstaller \
  --noconfirm \
  --clean \
  --onedir \
  --name RECOVER-X \
  --hidden-import _recoverx_py \
  --add-data "config:config" \
  --exclude-module PySide6.QtWebEngineCore \
  --exclude-module PySide6.QtWebEngineWidgets \
  --exclude-module PySide6.Qt3DCore \
  --exclude-module PySide6.QtQuick \
  --exclude-module PySide6.QtQml \
  --exclude-module PySide6.QtMultimedia \
  --exclude-module PySide6.QtPdf \
  --exclude-module PySide6.QtCharts \
  app/main.py

echo "[4/4] Assembling USB-ready portable folder..."
PORTABLE="dist/RECOVER-X-Portable-Linux"
mv dist/RECOVER-X "$PORTABLE"
cp target/release/recoverx-cli "$PORTABLE/recoverx-cli"

cat <<'LAUNCHER' > "$PORTABLE/Launch-RECOVER-X.sh"
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
if command -v xhost >/dev/null 2>&1; then
  xhost +SI:localuser:root >/dev/null 2>&1 || true
fi
sudo -E env \
  "QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-xcb}" \
  "QT_QPA_PLATFORMTHEME=" \
  "DBUS_SESSION_BUS_ADDRESS=" \
  "NO_AT_BRIDGE=1" \
  "$DIR/RECOVER-X" "$@"
LAUNCHER
chmod +x "$PORTABLE/Launch-RECOVER-X.sh" "$PORTABLE/recoverx-cli"

tar -czf dist/RECOVER-X-Portable-Linux.tar.gz -C dist RECOVER-X-Portable-Linux
echo ""
echo "Portable bundle ready:"
echo "  Folder:  $PORTABLE"
echo "  Archive: dist/RECOVER-X-Portable-Linux.tar.gz"
du -sh "$PORTABLE"
