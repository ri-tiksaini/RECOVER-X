#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Allow local root Qt window on X11 (Linux Mint Cinnamon/MATE/XFCE)
if command -v xhost >/dev/null 2>&1; then
  xhost +SI:localuser:root >/dev/null 2>&1 || true
fi
sudo -E env "PATH=$HOME/.cargo/bin:$PATH" \
  "QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-xcb}" \
  ./.venv/bin/python -m app.main "$@"
