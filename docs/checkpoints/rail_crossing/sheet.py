"""Rail-road level crossing owner review sheet (SKILL.md "Owner checkpoints"): the approved reference,
the textured headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play
distance, the seams with the rail straight and with a road stub, sun-side and shade-side close-ups of
the deck and a gate, eye level), labelled, 3 columns. There is no crossing tile yet, so the piece sits
on a meadow base with road stubs on its road lanes.
Usage: python3 docs/checkpoints/rail_crossing/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_crossing"
REFS = ROOT / "assets-source/refs/rail"
PROV = json.loads((ROOT / "assets-source/kit/rail_road_crossing_center/provenance.json").read_text())
W, H = 640, 400
CELLS = [
    (REFS / "ref-3.jpg", "reference ref-3 (approved: 60-degree crossing, open field gates)"),
    (DOCS / "still-three.jpg", "Blender EEVEE three-quarter, textured + masks AO"),
    (DOCS / "still-deck.jpg", "Blender EEVEE deck and gate ~4 m, sun side"),
    (DOCS / "still-scale.jpg", "EEVEE scale: a sedan on the deck beside a gate"),
    (DOCS / "crossing-play.jpg", "in engine, play distance ~65 m, tilt off"),
    (DOCS / "crossing-seam-rail.jpg", "in engine, seam with rail_straight_center (edge 0, x = 13.86, mid-frame)"),
    (DOCS / "crossing-seam-road.jpg", "in engine, seam with the edge-1 road stub (r = 6)"),
    (DOCS / "crossing-close-sun.jpg", "in engine, deck and gate, sun side (SE); scale: gates by the barn"),
    (DOCS / "crossing-close-shade.jpg", "in engine, shade side (NW): gate faces luma 32 vs twostorey N wall 22"),
    (DOCS / "crossing-eye.jpg", "in engine, eye level 1.7 m, edge-4 approach (N faces: sky light only)"),
]


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


cells = [cell(p, label) for p, label in CELLS]
cols = 3
rows = (len(cells) + cols - 1) // cols
cw, ch = cells[0].size
tris = "/".join(str(lod["tris"]) for lod in PROV["lods"])
b = PROV["compression"]["budgets"]
sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text(
    (8, 8),
    f"rail_road_crossing_center (P3): rail on edges 0/3, road on edges 1/4 (60 degrees), deck flush with the "
    f"rail heads, four gates open along the track. {tris} tris, GLB {b['glbBytes']:,} B, masks KTX2 "
    f"{b['ktxBytes']['bc7']:,} B BC7",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
