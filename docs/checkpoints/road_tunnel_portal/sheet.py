"""road_tunnel_portal owner review sheet (SKILL.md "Owner checkpoints"), Blender evidence only: the approved
reference ref-2 beside the build seen the same way, then textured headless EEVEE stills of the baked piece on a
stand-in of its cutting (eye level 1.7 m, the sun-side close-up, the top, a night preview with the lantern glass
lit), then the untextured six-view renders. The in-engine views wait for the portal tile, its runtime ground and
a booked GPU window (controller).
Usage: python3 docs/checkpoints/road_tunnel_portal/sheet.py (from the repo root; needs Pillow), after
eevee_still.py has written the portal-*.jpg stills."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/road_tunnel_portal"
RENDERS = ROOT / "assets-source/kit/road_tunnel_portal/renders"
REF = ROOT / "assets-source/refs/road_tunnel_portal/ref-2.jpg"
BAKE = json.loads((ROOT / "assets-source/kit/road_tunnel_portal/bake-report.json").read_text())
COMP = json.loads((ROOT / "assets-source/kit/road_tunnel_portal/compress-report.json").read_text())
W, H = 640, 457
CELLS = [
    (REF, "approved reference ref-2 (owner 2026-10-29)"),
    (DOCS / "portal-ref2.jpg", "the build, ref-2's view: baked granite, buff stones, lime mortar, lichen, moss"),
    (DOCS / "portal-close.jpg", "close-up, sun side: voussoirs, keystone, coping, scrolled lantern bracket"),
    (DOCS / "portal-three_quarter.jpg", "play distance: the cutting into the hill, wall top flush at 4.0 m"),
    (DOCS / "portal-fringe.jpg", "2x crop at play distance: grass fringe over the coping, fern clusters with gaps"),
    (DOCS / "portal-front.jpg", "front: opening 7.8 m, crown 3.15 m over the road's ground"),
    (DOCS / "portal-eye.jpg", "walker eye level 1.7 m, 14 m out: the barrel stone catching light at the mouth"),
    (DOCS / "portal-top.jpg", "top: the wall on edge 0, turf behind the coping (runtime blades grow there)"),
    (DOCS / "portal-night.jpg", "night preview: lantern glass masks B = 1 (inert until P5)"),
    (RENDERS / "three_quarter.jpg", "six-view render (untextured): three-quarter"),
]
COLS = 3
tris = BAKE["lodTris"]
dens = BAKE["texelDensityPxPerM"]
kb = COMP["budgets"]["ktxBytes"]
TITLE = (f"road_tunnel_portal review sheet (Blender EEVEE on a stand-in cutting, evidence only). LOD {tris[0]}/{tris[1]}/{tris[2]} "
         f"tris; stone {dens['stone']['median']:.0f} px/m median, ferns {dens['fern']['median']:.0f}; KTX2 "
         f"{kb['bc7']:,} B BC7 / {kb['astc']:,} B ASTC; pack core, kind prop, class piece.")
NOTE = ("Look pass 2 (controller, 2026-10-29): coping ferns in clusters of 2-4 at varied scale, yaw and lean; a grass fringe "
        "along the coping lip; a coursed-stone barrel lining fading to dark inside the mouth. Pending owner review.")
rows = (len(CELLS) + COLS - 1) // COLS
sheet = Image.new("RGB", (COLS * W, 60 + rows * (H + 22)), (24, 24, 24))
d = ImageDraw.Draw(sheet)
d.text((10, 8), TITLE, fill=(230, 230, 230))
d.text((10, 30), NOTE, fill=(255, 193, 7))
for i, (path, label) in enumerate(CELLS):
    x, y = (i % COLS) * W, 60 + (i // COLS) * (H + 22)
    d.text((x + 6, y + 4), label, fill=(230, 230, 230))
    img = Image.open(path).convert("RGB")
    img.thumbnail((W - 8, H - 8))
    sheet.paste(img, (x + 4, y + 22))
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print(f"wrote {out}")
