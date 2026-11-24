import argparse
import sys
import tempfile
from pathlib import Path
from app.service.report_service import build_report
from app.service.session_store import SessionStore


def run_selftest() -> int:
    from app.bridge import bindings

    priv = bindings.check_privilege()
    print(f"[selftest] elevated={priv.is_elevated}")
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        store = SessionStore(tmp / "sessions.db")
        sid = store.save_session("/tmp/fake.dd", "Ntfs", "quick", [{"id_hex": "abc", "status": "Intact"}])
        loaded = store.load_session(sid)
        assert loaded is not None and len(loaded["candidates"]) == 1
        rep = build_report(tmp / "rep", "/tmp/fake.dd", "Ntfs", "quick", [])
        assert rep["html_path"].exists() and rep["json_path"].exists()
    print("[selftest] PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="RECOVER-X Forensic Data Recovery")
    parser.add_argument("--selftest", action="store_true", help="Run headless self-test and exit")
    args = parser.parse_args()
    if args.selftest:
        return run_selftest()

    from PySide6.QtWidgets import QApplication
    from app.ui.main_window import MainWindow

    app = QApplication(sys.argv)
    win = MainWindow()
    win.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
