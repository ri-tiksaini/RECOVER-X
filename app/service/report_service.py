import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from jinja2 import Template


HTML_TEMPLATE = Template(
    r"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>RECOVER-X Recovery Report</title>
  <style>
    body { font-family: system-ui, -apple-system, sans-serif; background: #0b090a; color: #f4f1f2; margin: 0; padding: 32px; }
    .wrap { max-width: 1120px; margin: 0 auto; }
    h1 { margin: 0 0 6px; font-size: 24px; color: #f9f6f7; }
    .sub { color: #a89ca0; margin-bottom: 24px; font-size: 13px; }
    .kpis { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin-bottom: 24px; }
    .kpi { background: #141012; padding: 16px; border-radius: 10px; border: 1px solid #292023; }
    .kpi .num { font-size: 26px; font-weight: 700; color: #f9f6f7; }
    .kpi .lbl { font-size: 11px; color: #a89ca0; text-transform: uppercase; letter-spacing: 0.06em; }
    table { width: 100%; border-collapse: collapse; background: #141012; border-radius: 10px; overflow: hidden; font-size: 13px; border: 1px solid #292023; }
    th, td { padding: 10px 12px; border-bottom: 1px solid #241b1e; text-align: left; }
    th { background: #0d0a0b; color: #786b70; text-transform: uppercase; font-size: 11px; letter-spacing: 0.05em; }
    .badge { padding: 3px 9px; border-radius: 999px; font-weight: 600; font-size: 11px; display: inline-block; }
    .Intact { background: #0a2e23; color: #34d399; }
    .Partial { background: #331c08; color: #fbbf24; }
    .Unrecoverable { background: #3b0d1b; color: #fda4af; }
    .LowConfidence { background: #211c1e; color: #a89ca0; }
    code { font-family: ui-monospace, SFMono-Regular, monospace; font-size: 11px; color: #fda4af; word-break: break-all; }
  </style>
</head>
<body>
  <div class="wrap">
    <h1>RECOVER-X Recovery Report</h1>
    <div class="sub">{{ generated_utc }} &bull; Source: <strong>{{ source_path }}</strong> ({{ fs_kind }}, {{ mode }})</div>

    <div class="kpis">
      <div class="kpi"><div class="num">{{ total_exported }}</div><div class="lbl">Recovered</div></div>
      <div class="kpi"><div class="num">{{ counts.get("Intact", 0) }}</div><div class="lbl">Intact</div></div>
      <div class="kpi"><div class="num">{{ counts.get("Partial", 0) }}</div><div class="lbl">Partial</div></div>
      <div class="kpi"><div class="num">{{ counts.get("Unrecoverable", 0) }}</div><div class="lbl">Unrecoverable</div></div>
    </div>

    <table>
      <thead>
        <tr>
          <th>Status</th>
          <th>File</th>
          <th>Bytes</th>
          <th>SHA-256</th>
        </tr>
      </thead>
      <tbody>
        {% for r in records %}
        <tr>
          <td><span class="badge {{ r.status }}">{{ r.status }}</span></td>
          <td>{{ r.path_display }}</td>
          <td>{{ r.bytes_written }}</td>
          <td><code>{{ r.sha256_exported }}</code></td>
        </tr>
        {% endfor %}
      </tbody>
    </table>
  </div>
</body>
</html>
"""
)


def build_report(
    dest_dir: Path,
    source_path: str,
    fs_kind: str,
    mode: str,
    records: list[dict[str, Any]],
) -> dict[str, Path]:
    dest_dir.mkdir(parents=True, exist_ok=True)
    generated_utc = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    counts = Counter(r.get("status", "Unknown") for r in records)

    json_path = dest_dir / "report.json"
    html_path = dest_dir / "report.html"

    summary = {
        "generated_utc": generated_utc,
        "source_path": source_path,
        "fs_kind": fs_kind,
        "mode": mode,
        "total_exported": len(records),
        "counts": dict(counts),
        "records": records,
    }
    json_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    html = HTML_TEMPLATE.render(**summary)
    html_path.write_text(html, encoding="utf-8")
    return {"json_path": json_path, "html_path": html_path}
