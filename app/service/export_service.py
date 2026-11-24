from pathlib import Path
from typing import Any
from app.bridge import bindings
from app.bridge.errors import DestOnSourceError
from app.service.report_service import build_report


def export_selected(
    source_path: str,
    fs_kind: str,
    mode: str,
    candidates: list[dict[str, Any]],
    dest_dir: str,
) -> dict[str, Any]:
    if bindings.check_same_device(source_path, dest_dir):
        raise DestOnSourceError(
            "Refused [LAW-3]: Destination folder is on the same physical disk as the source drive. "
            "Pick a folder on a DIFFERENT physical drive."
        )
    records = bindings.export_candidates(source_path, candidates, dest_dir)
    report = build_report(Path(dest_dir), source_path, fs_kind, mode, records)
    return {
        "records": records,
        "html_path": str(report["html_path"]),
        "json_path": str(report["json_path"]),
        "manifest_path": str(Path(dest_dir) / "manifest.jsonl"),
    }
