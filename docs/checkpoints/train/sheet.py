"""Train owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the textured
headless EEVEE stills (evidence only) with runtime body colours applied the way car_material.wesl
does, and the in-engine shots with tilt-shift off (play distance, the consist on the bend, sun-side
and shade-side close-ups, eye level, and the scale shot), labelled, 3 columns. In engine the kit
scenario draws the pieces through KitMaterial, which has no paint input, so the bodies show their
unpainted white there. Cells whose shot is missing are skipped.
Usage: python3 docs/checkpoints/train/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/train"
REFS = ROOT / "assets-source/refs/train"
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved A17)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved A17: engine as drawn, two carriages)"),
    (DOCS / "still-engine.jpg", "EEVEE train_engine, textured + masks AO, paint green"),
    (DOCS / "still-carriage.jpg", "EEVEE train_carriage, paint maroon"),
    (DOCS / "still-straight.jpg", "EEVEE consist on the straight, buffers touching"),
    (DOCS / "still-bend.jpg", "EEVEE consist on the 8 m bend (last carriage over the joint)"),
    (DOCS / "still-close.jpg", "EEVEE engine front ~4 m, sun side"),
    (DOCS / "still-scale.jpg", "EEVEE scale: engine beside a sedan and a cottage"),
    (DOCS / "still-paint-a.jpg", "EEVEE runtime colour A (blue)"),
    (DOCS / "still-paint-b.jpg", "EEVEE runtime colour B (ochre)"),
    (DOCS / "train-play.jpg", "in engine, play distance, tilt off (unpainted white)"),
    (DOCS / "train-bend.jpg", "in engine, consist on the bend, tilt off"),
    (DOCS / "train-close-sun.jpg", "in engine, sun side (south), ~6 m"),
    (DOCS / "train-close-shade.jpg", "in engine, shade side (north), ~6 m"),
    (DOCS / "train-eye.jpg", "in engine, eye level 1.7 m beside the track"),
    (DOCS / "train-scale.jpg", "in engine, scale: train toward the barn and oak"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


def tris(pid):
    rep = json.loads((ROOT / f"assets-source/kit/{pid}/export-report.json").read_text())
    return "/".join(str(rep["lodTris"][f"{pid}_lod{i}"]) for i in range(3))


cells = [cell(p, label) for p, label in CELLS if p.exists()]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text(
    (8, 8),
    f"train (P3): review sheet. engine 0-6-0 {tris('train_engine')} tris, carriage 4-wheel {tris('train_carriage')} tris; "
    "1.6 m gauge, treads on the 0.46 m rail top; bend clearance engine 2.02 m, carriage 1.87 m (limit 2.5)",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", len(cells), "cells")
