from dataclasses import dataclass
from typing import Optional
from app.bridge import bindings
from app.bridge.dto import DeviceInfo, PrivilegeInfo


@dataclass(frozen=True)
class SourceOption:
    label: str
    os_path: str
    device_model: str
    media_kind: str
    trim_status: str
    mount_state: str
    size_bytes: int
    mount_point: Optional[str]


def human_bytes(n: int) -> str:
    val = float(n)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if val < 1024.0 or unit == "TB":
            return f"{val:.1f} {unit}" if unit != "B" else f"{int(val)} B"
        val /= 1024.0
    return f"{n} B"


def get_privilege() -> PrivilegeInfo:
    return bindings.check_privilege()


def list_devices() -> list[DeviceInfo]:
    return bindings.device_list()


def list_source_options() -> list[SourceOption]:
    options: list[SourceOption] = []
    for dev in bindings.device_list():
        trim_label = {
            "Enabled": "TRIM ON (low recovery odds)",
            "Disabled": "TRIM OFF",
            "NotApplicable": "TRIM n/a (high recovery odds)",
        }.get(dev.trim_status, "TRIM unknown")

        for part in dev.partitions:
            mp = f" [{part.mount_point}]" if part.mount_point else ""
            label = (
                f"{part.os_path}{mp} — {dev.model} "
                f"({dev.media_kind}, {human_bytes(part.len_bytes)}, {trim_label})"
            )
            options.append(
                SourceOption(
                    label=label,
                    os_path=part.os_path,
                    device_model=dev.model,
                    media_kind=dev.media_kind,
                    trim_status=dev.trim_status,
                    mount_state=dev.mount_state,
                    size_bytes=part.len_bytes,
                    mount_point=part.mount_point,
                )
            )
    return options
