import json
from typing import Any, Optional
import _recoverx_py
from app.bridge.dto import DeviceInfo, PartitionInfo, PrivilegeInfo
from app.bridge.handles import SourceContext


def check_privilege() -> PrivilegeInfo:
    raw = json.loads(_recoverx_py.check_privilege())
    return PrivilegeInfo(**raw)


def check_same_device(source_path: str, dest_dir: str) -> bool:
    return bool(_recoverx_py.check_same_device(source_path, dest_dir))


def device_list() -> list[DeviceInfo]:
    raw = json.loads(_recoverx_py.device_list())
    out: list[DeviceInfo] = []
    for d in raw:
        parts = [PartitionInfo(**p) for p in d.pop("partitions", [])]
        out.append(DeviceInfo(**d, partitions=parts))
    return out


def open_source(os_path: str, partition_index: Optional[int] = None) -> SourceContext:
    return SourceContext(os_path, partition_index)


def scan_source(
    os_path: str,
    mode: str = "quick",
    max_candidates: Optional[int] = None,
    partition_index: Optional[int] = None,
) -> dict[str, Any]:
    with open_source(os_path, partition_index) as src:
        return json.loads(src.scan_json(mode=mode, max_candidates=max_candidates))


def export_candidates(
    os_path: str,
    candidates: list[dict[str, Any]],
    dest_dir: str,
    partition_index: Optional[int] = None,
) -> list[dict[str, Any]]:
    with open_source(os_path, partition_index) as src:
        raw = src.export_candidates_json(json.dumps(candidates), dest_dir)
        return json.loads(raw)


def live_handle_count() -> int:
    return int(_recoverx_py.live_handle_count())
