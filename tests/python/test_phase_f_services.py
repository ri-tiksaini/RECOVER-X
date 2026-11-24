from pathlib import Path
from app.service.report_service import build_report
from app.service.session_store import SessionStore


def test_session_store_and_html_report(tmp_path: Path):
    store = SessionStore(tmp_path / "sessions.db")
    sample = [
        {
            "id_hex": "00112233445566778899aabbccddeeff",
            "path_display": "/Docs/report.docx",
            "bytes_written": 12345,
            "status": "Intact",
            "reason_code": "None",
            "confidence": 95,
            "sha256_exported": "a" * 64,
            "sha256_source_region": "a" * 64,
        }
    ]
    sid = store.save_session("/dev/sdb1", "Ntfs", "quick", sample)
    loaded = store.load_session(sid)
    assert loaded is not None
    assert loaded["source_path"] == "/dev/sdb1"
    assert len(loaded["candidates"]) == 1

    out = build_report(tmp_path / "out", "/dev/sdb1", "Ntfs", "quick", sample)
    html = out["html_path"].read_text(encoding="utf-8")
    assert "RECOVER-X Forensic Recovery Report" in html
    assert "/Docs/report.docx" in html
    assert "a" * 64 in html
