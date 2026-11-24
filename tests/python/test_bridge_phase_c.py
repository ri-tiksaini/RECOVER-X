import gc
import json
from pathlib import Path
from app.bridge import bindings


def test_privilege_and_device_enum():
    priv = bindings.check_privilege()
    assert isinstance(priv.is_elevated, bool)
    assert len(priv.guidance) > 0
    devs = bindings.device_list()
    assert isinstance(devs, list)


def test_open_image_and_handle_cleanup(tmp_path: Path):
    img = tmp_path / "test.dd"
    buf = bytearray(4096)
    buf[446 + 4] = 0x07
    buf[446 + 8 : 446 + 12] = (2).to_bytes(4, "little")
    buf[446 + 12 : 446 + 16] = (4).to_bytes(4, "little")
    buf[510] = 0x55
    buf[511] = 0xAA
    img.write_bytes(buf)

    assert bindings.check_same_device(str(img), str(tmp_path / "out")) is True

    with bindings.open_source(str(img)) as src:
        assert src.size_bytes == 4096
        assert src.sector_size == 512
        parts = json.loads(src.partitions_json())
        assert len(parts) == 1
        assert parts[0]["scheme"] == "MBR"

    gc.collect()
    assert bindings.live_handle_count() == 0
