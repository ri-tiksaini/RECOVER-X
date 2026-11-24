# RECOVER-X (Drop 1: Phase A + B + C)

Forensic-grade, strictly read-only deleted-data recovery engine.

## Quick Start on Linux Mint

```bash
chmod +x scripts/setup_mint.sh
./scripts/setup_mint.sh
```

Then inspect your drives (read-only):

```bash
source .venv/bin/activate
sudo ./target/release/recoverx-cli devices
```

To inspect the backing device of `/media/x_x` (read-only):

```bash
findmnt -n -o SOURCE /media/x_x
sudo ./target/release/recoverx-cli inspect <DEVICE_PATH>
```
