from typing import Optional
import _recoverx_py


class SourceContext:
    def __init__(self, os_path: str, partition_index: Optional[int] = None) -> None:
        self._os_path = os_path
        self._partition_index = partition_index
        self._handle: Optional[_recoverx_py.SourceHandle] = None

    def __enter__(self) -> _recoverx_py.SourceHandle:
        self._handle = _recoverx_py.open_source(self._os_path, self._partition_index)
        return self._handle

    def __exit__(self, exc_type, exc, tb) -> None:
        if self._handle is not None:
            self._handle.close()
            self._handle = None
