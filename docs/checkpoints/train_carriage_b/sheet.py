"""train_carriage_b owner review sheet (SKILL.md "Owner checkpoints"): the approved references (which
draw two identical carriages, so none draws this brake third), the textured headless EEVEE stills
(evidence only; runtime liveries applied the way car_material.wesl does), and the in-engine shots in
`scenario=play` with tilt-shift off and the integration patch applied, where trains run in their
runtime liveries (hex_play::trains::LIVERIES by train id): play distance with three trains, the scale
shot at the level crossing between two house clusters, sun-side close-ups of a standing and a running
train, the shade side, and eye level 1.7 m beside the track (walk mode). 3 columns; missing cells are
skipped.
Usage: python3 docs/checkpoints/train_carriage_b/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/train_carriage_b"
REFS = ROOT / "assets-source/refs/train"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved A17; two identical carriages)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved A17; no brake carriage drawn)"),
    (DOCS / "still-tail.jpg", "EEVEE train_carriage_b from the tail, crimson"),
    (DOCS / "still-side.jpg", "EEVEE sun side: guard's doors, lookout, veranda"),
    (DOCS / "still-pair.jpg", "EEVEE train_carriage (back) and train_carriage_b, blue"),
    (DOCS / "still-straight.jpg", "EEVEE engine, train_carriage, train_carriage_b, green"),
    (DOCS / "still-bend.jpg", "EEVEE consist on the 8 m bend, chocolate"),
    (DOCS / "still-close.jpg", "EEVEE the veranda ~4 m, shade side"),
    (DOCS / "tcb-play.jpg", "play, tilt off: three trains, three liveries, brake third last"),
    (DOCS / "tcb-scale.jpg", "play: scale at the level crossing between house clusters"),
    (DOCS / "tcb-close-sun-crimson.jpg", "play: sun side ~7 m, crimson train standing"),
    (DOCS / "tcb-close-sun.jpg", "play: sun side ~7 m, green train running"),
    (DOCS / "tcb-close-shade.jpg", "play: shade side (north) ~7 m"),
    (DOCS / "tcb-eye.jpg", "play: eye level 1.7 m beside the track (walk mode)"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


def tris(pid: str) -> str:
    rep = json.loads((ROOT / f"assets-source/kit/{pid}/export-report.json").read_text())
    return "/".join(str(rep["lodTris"][f"{pid}_lod{i}"]) for i in range(3))


geo = json.loads((ROOT / "assets-source/kit/train_carriage_b/bake-report.json").read_text())["geometry"]
cells = [cell(p, label) for p, label in CELLS if p.exists()]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text(
    (8, 8),
    f"train_carriage_b (P3): brake third, {tris('train_carriage_b')} tris (train_carriage {tris('train_carriage')}); "
    f"{geo['lengthM']:.2f} m over buffers, {geo['widthM']:.2f} m wide, axles +-1.5 m as train_carriage; "
    f"{geo['heightAboveRailM']:.2f} m tall at the lookout; bend {max(geo['maxOffTrackCentreOnBendM']):.2f} m (limit 2.5)",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", len(cells), "cells")
