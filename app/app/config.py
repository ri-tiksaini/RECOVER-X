import tomllib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class AppConfig:
    max_rss_mb: int = 512
    threads: int = 4
    carve_alignment: str = "sector"
    poll_batch_size: int = 256


def load_config(path: Path = Path("config/default.toml")) -> AppConfig:
    if not path.exists():
        return AppConfig()
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    rt = data.get("runtime", {})
    cv = data.get("carve", {})
    ui = data.get("ui", {})
    return AppConfig(
        max_rss_mb=int(rt.get("max_rss_mb", 512)),
        threads=max(1, int(rt.get("threads", 4))),
        carve_alignment=str(cv.get("carve_alignment", "sector")),
        poll_batch_size=int(ui.get("poll_batch_size", 256)),
    )
