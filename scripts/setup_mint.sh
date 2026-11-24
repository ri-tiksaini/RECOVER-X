#!/usr/bin/env bash
set -euo pipefail

echo "[1/5] Installing Linux Mint system packages..."
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev \
  python3-dev python3-venv libxcb-cursor0 libgl1 \
  ntfs-3g e2fsprogs dosfstools exfatprogs util-linux

if ! command -v cargo >/dev/null 2>&1; then
  echo "[2/5] Installing Rust toolchain..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "[2/5] Rust already installed: $(rustc --version)"
fi

echo "[3/5] Creating Python venv and installing dependencies..."
python3 -m venv .venv
# shellcheck disable=SC1091
source .venv/bin/activate
pip install --upgrade pip
pip install maturin PySide6 jinja2 pytest structlog zstandard

echo "[4/5] Running Rust unit + integration tests (Phase A & B)..."
cargo test --workspace
cargo build --release -p recoverx_cli

echo "[5/5] Building PyO3 Python bridge and running pytest (Phase C)..."
maturin develop --release
pytest tests/python -v

echo ""
echo "Setup and Phase A-C verification complete!"
echo "Try (read-only): sudo ./target/release/recoverx-cli devices"
