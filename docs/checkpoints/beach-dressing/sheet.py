"""Owner review sheet for the beach dressing (owner D3, 2026-10-20): the approved driftwood and
dune-grass refs, the textured EEVEE stills, each new piece's six-view render, and the beach tiles
in play before and after (straight, river mouth both hands, headland and bay at play distance, and
eye level in walk mode on the straight, the mouth and the headland), with the scatter counts per tile
from the after state dumps.
Usage: python3 docs/checkpoints/beach-dressing/sheet.py <before shots dir> <after shots dir> <stills dir>
(from the repo root, after run.sh before/after and still.py; needs Pillow). Writes the play and still
JPEGs beside this file and sheet.jpg."""
import json
import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/beach-dressing"
before, after, stills = (pathlib.Path(a) for a in sys.argv[1:4])
W, H = 520, 325
PIECES = ("beach_driftwood", "beach_driftwood_b", "beach_driftwood_c", "beach_dunegrass",
          "beach_dunegrass_b", "beach_dunegrass_c")
VIEWS = ("front", "back", "left", "right", "top", "three_quarter")
PLAY = [("straight-play", "straight beach (4, -1), play distance"),
        ("straight-eye", "straight beach, eye level 1.7 m (walk)"),
        ("mouth-play", "river mouth (3, 0), play distance"),
        ("mouth-eye", "river mouth, eye level 1.7 m (walk)"),
        ("mouth-l-play", "river mouth l (-3, 0), play distance"),
        ("headland-play", "headland (beach_outer), play distance"),
        ("headland-eye", "headland, eye level 1.7 m (walk)"),
        ("bay-play", "bay (beach_inner), play distance")]


def cell(path, label, w=W, h=H):
    c = Image.new("RGB", (w, h + 22), (24, 24, 24))
    if path is not None and pathlib.Path(path).exists():
        im = Image.open(path).convert("RGB")
        im.thumbnail((w, h))
        c.paste(im, ((w - im.width) // 2, 22 + (h - im.height) // 2))
    else:
        label += " (missing)"
    ImageDraw.Draw(c).text((6, 5), label, fill=(232, 232, 232))
    return c


def jpeg(src, name):
    dst = DOCS / f"{name}.jpg"
    if pathlib.Path(src).exists():
        im = Image.open(src).convert("RGB")
        im.thumbnail((1600, 1000))
        im.save(dst, quality=85)
    return dst


counts = {}
for world in ("mouth", "coast"):
    p = after / f"bd-after-{world}-state.json"
    if p.exists():
        for t in json.loads(p.read_text())["scatter"]["tiles"]:
            if "beach_driftwood" in t.get("placed", {}):
                counts.setdefault(t["tile"], t["placed"])
rows = []
refs = [(ROOT / "assets-source/refs/driftwood/ref-1.jpg", "driftwood ref-1 (approved 2026-10-20)"),
        (ROOT / "assets-source/refs/driftwood/ref-2.jpg", "driftwood ref-2 (approved)"),
        (ROOT / "assets-source/refs/dune_grass/ref-1.jpg", "dune_grass ref-1 (approved)"),
        (ROOT / "assets-source/refs/dune_grass/ref-2.jpg", "dune_grass ref-2 (approved)")]
rows.append([cell(p, l) for p, l in refs])
st = [(jpeg(stills / "lineup.png", "eevee-lineup"), "EEVEE lineup, 1.7 m figure (untuned light)")]
st += [(jpeg(stills / f"{p}-eevee.png", f"{p}-eevee"), f"EEVEE {p}") for p in PIECES]
rows.append([cell(p, l) for p, l in st[:4]])
rows.append([cell(p, l) for p, l in st[4:]])
for p in PIECES:
    rows.append([cell(ROOT / "assets-source/kit" / p / "renders" / f"{v}.jpg", f"{p} {v}", 346, 216)
                 for v in VIEWS])
label = ", ".join(f"{t}: {c.get('beach_driftwood', 0)} driftwood + {c.get('beach_dunegrass', 0)} clumps"
                  for t, c in sorted(counts.items()))
for name, what in PLAY:
    b = jpeg(before / f"bd-before-{name}.png", f"bd-before-{name}")
    a = jpeg(after / f"bd-after-{name}.png", f"bd-after-{name}")
    rows.append([cell(b, f"BEFORE {what}", 1040, 650), cell(a, f"AFTER {what}", 1040, 650)])
kit = [(jpeg(after / f"{n}.png", n), what) for n, what in
       (("bd-after-kit-beach", "kit scenario: the beach dressing cell (5, -1), bd-kit.json"),
        ("bd-after-kit-beach-close", "kit scenario: close, the three logs and the clumps"))]
rows.append([cell(p, l, 1040, 650) for p, l in kit])
width = max(sum(c.width for c in r) for r in rows)
head = 40
sheet = Image.new("RGB", (width, head + sum(max(c.height for c in r) for r in rows)), (16, 16, 16))
ImageDraw.Draw(sheet).text((10, 6), "Beach dressing (D3): driftwood x3 and marram clumps x3 on the dry sand. "
                           f"Counts per tile in play: {label}", fill=(240, 240, 240))
y = head
for r in rows:
    x = 0
    for c in r:
        sheet.paste(c, (x, y))
        x += c.width
    y += max(c.height for c in r)
sheet.save(DOCS / "sheet.jpg", quality=82)
print("sheet", sheet.size, counts)
