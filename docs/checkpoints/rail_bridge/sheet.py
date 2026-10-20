"""River rail bridge owner review sheet (SKILL.md "Owner checkpoints"): the approved references, the
textured headless EEVEE stills (evidence only), and the in-engine shots with tilt-shift off (play
distance, both rail seams, the upstream and downstream river seams, sun-side and shade-side arch
close-ups, eye level, the train on the deck), labelled, 3 columns. There is no rail-bridge tile yet,
so the kit scenario places the piece on river_straight_base and draws no water.
Usage: python3 docs/checkpoints/rail_bridge/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_bridge"
PROV = json.loads((ROOT / "assets-source/kit/river_rail_bridge_center/provenance.json").read_text())
W, H = 640, 400
CELLS = [
    (ROOT / "assets-source/refs/river_road_bridge/ref-1.jpg", "reference: road bridge ref-1 (approved stone arch at 60 deg)"),
    (ROOT / "assets-source/refs/rail/ref-1.jpg", "reference: rail ref-1 (A17: single track, ballast)"),
    (DOCS / "still-three.jpg", "Blender EEVEE three-quarter, textured + masks; water plane at -1.4 m"),
    (DOCS / "still-arch.jpg", "EEVEE arch from the channel: crown -0.45 m, 0.95 m over the water"),
    (DOCS / "still-scale.jpg", "EEVEE scale: engine and carriage on the deck at the 0.46 m rail top"),
    (DOCS / "bridge-play.jpg", "in engine, play distance ~70 m, tilt off (kit scenario: no water)"),
    (DOCS / "bridge-seam-rail-ne.jpg", "rail seam NE (world edge 5, border mid-frame at world (20.8, 108)): bridge right"),
    (DOCS / "bridge-seam-rail-sw.jpg", "rail seam SW (world edge 2, border mid-frame at world (6.9, 132)): bridge left"),
    (DOCS / "bridge-seam-river-up.jpg", "river seam upstream: river_edge at world (6.9, 108) mid-frame, (-2, 4) beyond"),
    (DOCS / "bridge-seam-river-down.jpg", "river seam downstream: river_edge at world (20.8, 132) mid-frame, (-2, 6) beyond"),
    (DOCS / "bridge-close-sun.jpg", "in engine, arch face 8 m, sun side (SE): wall luma 160"),
    (DOCS / "bridge-close-shade.jpg", "shade side (NW): wall luma 15, sky light only; same slot + tint as the road bridge"),
    (DOCS / "bridge-eye.jpg", "in engine, eye level 1.7 m beside the NE approach"),
    (DOCS / "bridge-scale.jpg", "in engine, scale: engine + carriage on the deck"),
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
    f"river_rail_bridge_center (P3): rail on edges 1/4 level at its 0.46 m rail top (0% grade) across the river "
    f"(edges 0/3) on the road bridge's arch; parapets 2.6 m off the track, top 1.0 m. {tris} tris, "
    f"GLB {b['glbBytes']:,} B, masks KTX2 {b['ktxBytes']['bc7']:,} B BC7",
    fill=(235, 235, 235))
for k, c in enumerate(cells):
    sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
