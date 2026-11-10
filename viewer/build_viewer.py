"""Embed simulator output (report.json + replays/*.json) into the viewer template."""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent
results = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root.parent / "results" / "v05")
out = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else root / "index.html")

order = ["search_vs_search", "shaper_vs_search", "air_vs_ground", "rush_vs_fort", "search_vs_greedy"]
replays = {}
for name in order:
    p = results / "replays" / f"{name}.json"
    if p.exists():
        replays[name] = json.loads(p.read_text())
data = {"replays": replays, "report": json.loads((results / "report.json").read_text())}
html = (root / "template.html").read_text()
html = html.replace("/*__DATA__*/", "const DATA = " + json.dumps(data, separators=(",", ":")) + ";")
out.write_text(html)
print(f"wrote {out} ({out.stat().st_size // 1024} KB, {len(replays)} replays)")
