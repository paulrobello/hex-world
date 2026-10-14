"""Writes docs/review/2026-10-14-grade-compare.html from measurements.json, the frames and the
grade list `shots.py` dumped (target/shots/grade-state.json). Run after shots.py:
    python3 docs/checkpoints/grades/page.py
"""

import html
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(REPO, "docs", "review", "2026-10-14-grade-compare.html")
GRADES = ["aces", "tony", "rich-1", "rich-2", "rich-3"]

# Mirrors crates/hex-render/src/grade.rs.
SETTINGS = {
    "aces": "AcesFitted, EV100 12.5, no grading, Bevy's default clear colour (43, 44, 47)",
    "tony": "TonyMcMapface, EV100 12.9, no grading, lookdev::BACKDROP",
    "rich-1": "TonyMcMapface, EV100 12.9, post-tonemap saturation 1.2",
    "rich-2": "TonyMcMapface, EV100 12.9, every section gamma 0.926 and gain 1.135 (a 1.08 log "
              "contrast about 0.18 linear), post-tonemap saturation 1.12, backdrop linear "
              "(0.0046, 0.0047, 0.0058)",
    "rich-3": "AcesFitted, EV100 12.5, shadows and highlights lift +0.013 before the curve, "
              "backdrop linear (0.011, 0.0118, 0.0142)",
}

VIEWS = [
    ("overview", "Overview with houses", "scatter world, high three-quarter view"),
    ("houses", "Houses", "house cluster beside the road"),
    ("bridge", "River and bridge", "river chain, bridge and mouth (river-play.json)"),
    ("mouth", "River mouth wall", "the shade-diagnosis mouth camera (k5-river-04)"),
    ("oak", "Forest and oak", "scatter world, oak tile among pines (scatter-12-oak)"),
    ("coast", "Coast and beach", "coast island (beach-closeups.json, beach-05 camera)"),
    ("meadow", "Meadow close-up", "scatter world, low meadow view (scatter-03-meadow)"),
]

ROWS = [
    ("Shade (sRGB luma /255)", [
        ("mouth-left-wall", "River-mouth wall, left"),
        ("mouth-right-wall", "River-mouth wall, right"),
        ("bridge-face", "Bridge arch face"),
        ("meadow-wall", "Meadow cut wall (coast)"),
        ("backdrop", "Backdrop"),
    ], "srgb_luma"),
    ("Lit surfaces (sRGB luma /255)", [
        ("mouth-grass", "Grass (mouth)"),
        ("bridge-gravel", "Gravel"),
        ("mouth-water", "River water (mouth)"),
        ("bridge-water", "River water (bridge)"),
    ], "srgb_luma"),
    ("Saturation (mean HSV S per pixel)", [
        ("mouth-grass", "Grass (mouth)"),
        ("meadow-grass", "Grass (meadow close-up)"),
        ("mouth-water", "River water"),
        ("sea", "Sea (coast)"),
        ("roof", "Roof"),
        ("forest", "Forest canopy"),
    ], "hsv_sat"),
]

CSS = """
:root{--bg:#f4f5f2;--card:#fff;--ink:#1c211e;--muted:#5b655f;--line:#d7dbd5;--accent:#2f6f4f;--rec:#eaf3ee;--warn:#9a5b13}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23;--warn:#e0a35a}}
:root[data-theme=dark]{--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23;--warn:#e0a35a}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,sans-serif}
main{max-width:1500px;margin:0 auto;padding:20px 16px 80px;overflow-wrap:anywhere}h1{font-size:22px;margin:0 0 6px}
nav{display:flex;flex-wrap:wrap;gap:6px;margin:12px 0 20px}nav a{font-size:13px;color:var(--accent);text-decoration:none;border:1px solid var(--line);border-radius:6px;padding:3px 8px}
section{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:16px;margin:0 0 18px;scroll-margin-top:12px}
h2{font-size:18px;margin:0 0 8px}h2 span{color:var(--muted);font-weight:500}p{margin:6px 0;max-width:100ch}
.rec{background:var(--rec);border-left:3px solid var(--accent);padding:8px 12px;border-radius:4px;margin:10px 0}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(270px,100%),1fr));gap:10px;margin-top:10px}
figure{margin:0}figure img{width:100%;border-radius:4px;border:1px solid var(--line);display:block;aspect-ratio:16/10;object-fit:cover}
figcaption{font-size:12px;color:var(--muted)}figcaption b{color:var(--ink);font-size:13px}
.top{font-size:13px;color:var(--muted)}.scroll{overflow-x:auto}
table{border-collapse:collapse;font-size:13px;margin:8px 0}td,th{border:1px solid var(--line);padding:4px 8px;text-align:right;white-space:nowrap}
th:first-child,td:first-child{text-align:left}th{background:var(--rec)}td.low{color:var(--warn);font-weight:600}
code{font-size:13px}dl{margin:6px 0}dt{font-weight:600;margin-top:6px}dd{margin:0 0 0 16px;color:var(--muted)}
"""

RETRY = """<script>
// The plain review server can drop a few requests when many images load at once: retry each failed image up to 3 times.
document.addEventListener("error", (e) => {
  const img = e.target;
  if (!(img instanceof HTMLImageElement)) return;
  const n = Number(img.dataset.retry || 0);
  if (n >= 3) return;
  img.dataset.retry = String(n + 1);
  const src = img.getAttribute("src").replace(/[?&]retry=\\d+$/, "");
  setTimeout(() => { img.src = src + (src.includes("?") ? "&" : "?") + "retry=" + (n + 1); }, 300 * (n + 1));
}, true);
</script>"""


def main():
    with open(os.path.join(HERE, "measurements.json")) as f:
        m = json.load(f)
    details = {g: "" for g in GRADES}
    state = os.path.join(REPO, "target", "shots", "grade-state.json")
    if os.path.exists(state):
        with open(state) as f:
            for p in json.load(f)["grade"]["presets"]:
                details[p["name"]] = p["detail"]
    with open(os.path.join(HERE, "summary.json")) as f:
        summary = json.load(f)
    e = html.escape
    out = ["<!doctype html><html lang=en><head><meta charset=utf-8>"
           "<meta name=viewport content='width=device-width,initial-scale=1'>"
           f"<title>Grade Compare</title><style>{CSS}</style></head><body><main>",
           "<h1>Colour grade comparison, 2026-10-14</h1>",
           "<p class=top>Owner feedback: the TonyMcMapface look feels washed out next to the earlier ACES "
           "look; something between the two. Five presets, each shot live from the same native frames "
           "(1600x1000, seed 1, fixed clocks, tilt-shift off, photo mode with depth of field off). The "
           "default stays <code>tony</code>; nothing here changes it. Click an image for full size.</p>",
           f"<div class=rec>{summary['headline']}</div>"]
    out.append("<nav>" + "".join(f"<a href='#{v}'>{e(t)}</a>" for v, t, _ in VIEWS)
               + "<a href='#numbers'>Numbers</a><a href='#switch'>How to switch</a></nav>")
    out.append("<section id=presets><h2>Presets</h2><div class=scroll><table><tr><th>Preset</th>"
               "<th style='text-align:left'>Settings</th><th style='text-align:left'>In one line</th></tr>")
    for g in GRADES:
        out.append(f"<tr><td><b>{g}</b></td><td style='text-align:left;white-space:normal'>{e(SETTINGS[g])}</td>"
                   f"<td style='text-align:left;white-space:normal'>{e(details[g])}</td></tr>")
    out.append("</table></div>")
    for line in summary.get("notes", []):
        out.append(f"<p>{line}</p>")
    out.append("</section>")
    for v, title, where in VIEWS:
        out.append(f"<section id='{v}'><h2>{e(title)} <span>{e(where)}</span></h2><div class=grid>")
        for g in GRADES:
            src = f"../checkpoints/grades/frames/grade-{v}-{g}.jpg"
            out.append(f"<figure><a href='{src}' target=_blank><img src='{src}' loading=lazy "
                       f"alt='{e(title)}, {g}'></a><figcaption><b>{g}</b> {e(details[g])}</figcaption></figure>")
        out.append("</div></section>")
    out.append("<section id=numbers><h2>Numbers <span>measured on the PNG frames, "
               "docs/checkpoints/grades/measurements.txt</span></h2>"
               "<p>Crop boxes are in <code>crops.json</code>: the shade-diagnosis boxes on the mouth and "
               "bridge-arch cameras, plus new boxes on other cameras: the shaded meadow cut wall and the sea (coast view), a "
               "roof, the forest canopy and the meadow close-up. Walls under 8/255 are marked.</p>")
    for head, rows, key in ROWS:
        out.append(f"<h3>{e(head)}</h3><div class=scroll><table><tr><th>Crop</th>"
                   + "".join(f"<th>{g}</th>" for g in GRADES) + "</tr>")
        for crop, label in rows:
            cells = []
            for g in GRADES:
                val = m.get(g, {}).get(crop, {}).get(key)
                if val is None:
                    cells.append("<td>–</td>")
                    continue
                low = key == "srgb_luma" and "wall" in crop and val < 8
                txt = f"{val:.1f}" if key == "srgb_luma" else f"{val:.3f}"
                cells.append(f"<td class='{'low' if low else ''}'>{txt}</td>")
            out.append(f"<tr><td>{e(label)}</td>{''.join(cells)}</tr>")
        out.append("</table></div>")
    out.append("</section>")
    out.append("<section id=switch><h2>How to switch</h2><dl>"
               "<dt>Settings</dt><dd>Settings, Graphics tab, Colour grade: pick a row; it applies at once and "
               "is remembered in this browser (not in the world save).</dd>"
               "<dt>Web address</dt><dd><code>?grade=rich-2</code> (overrides the remembered choice for that "
               "visit).</dd>"
               "<dt>Native</dt><dd><code>cargo run -p hex-app --release -- --scenario play --grade rich-2</code>; "
               "<code>--tonemap</code> and <code>--ev100</code> still override the preset's own values.</dd>"
               "<dt>Live, from the console or a script</dt><dd><code>hexWorld.cmd({cmd: 'set_grade', grade: "
               "'rich-2'})</code>; <code>hexWorld.state().grade</code> reports the active preset and the list.</dd>"
               "</dl></section>")
    out.append("</main>" + RETRY + "\n</body></html>\n")
    with open(OUT, "w") as f:
        f.write("\n".join(out))
    print("wrote", OUT)


if __name__ == "__main__":
    main()
