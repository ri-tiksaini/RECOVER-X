from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class PrivilegeInfo:
    is_elevated: bool
    guidance: str


@dataclass(frozen=True)
class PartitionInfo:
    os_path: str
    start_lba: int
    len_bytes: int
    mount_point: Optional[str]


@dataclass(frozen=True)
class DeviceInfo:
    id: str
    os_path: str
    model: str
    serial: str
    size_bytes: int
    sector_size: int
    media_kind: str
    trim_status: str
    mount_state: str
    partitions: list[PartitionInfo]
