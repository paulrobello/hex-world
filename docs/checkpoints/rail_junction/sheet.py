"""Rail junction owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the
textured headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play
distance, the seams on all three lanes, sun-side and shade-side close-ups of the switch and frog, eye
level), labelled, 3 columns. There is no junction tile yet, so the piece sits on a meadow base.
Usage: python3 docs/checkpoints/rail_junction/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_junction"
REFS = ROOT / "assets-source/refs/rail"
PROV = json.loads((ROOT / "assets-source/kit/rail_junction_center/provenance.json").read_text())
W, H = 640, 400
CELLS = [
    (REFS / "ref-1.jpg", "reference ref-1 (approved: straight, wider gauge)"),
    (REFS / "ref-2.jpg", "reference ref-2 (approved: bend across adjacent edges)"),
    (DOCS / "still-three.jpg", "Blender EEVEE three-quarter, textured + masks AO"),
    (DOCS / "still-switch.jpg", "EEVEE close-up: switch blades, stretcher bar, timbers"),
    (DOCS / "still-frog.jpg", "EEVEE close-up: frog vee, wing rails, check rail"),
    (DOCS / "still-top.jpg", "EEVEE top-down over the switch and frog"),
    (DOCS / "junction-play.jpg", "in engine, play distance ~65 m, tilt off"),
    (DOCS / "junction-seam-toe.jpg", "in engine, toe seam with rail_bend_60_center (1, 2)"),
    (DOCS / "junction-seam-straight.jpg", "in engine, straight seam with rail_straight_center (-1, 4)"),
    (DOCS / "junction-seam-branch.jpg", "in engine, branch seam with rail_bend_60_center (1, 3)"),
    (DOCS / "junction-close-sun.jpg", "in engine, frog and switch ~6 m, sun side (SE)"),
    (DOCS / "junction-close-shade.jpg", "in engine, frog and switch ~6 m, shade side (NW)"),
    (DOCS / "junction-eye.jpg", "in engine, eye level 1.7 m beside the point lever"),
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
    f"rail_junction_center (P3): straight on edges 0/3, 8 m branch from the toe at edge 0 to edge 1, blades, "
    f"frog, check rails, long timbers, point lever. {tris} tris, GLB {b['glbBytes']:,} B, masks KTX2 "
    f"{b['ktxBytes']['bc7']:,} B BC7",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
