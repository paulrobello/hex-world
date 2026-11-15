"""road_tunnel_bore owner review sheet (with the open road_tunnel_portal it joins). Rows: in-engine kit-scenario
shots of the bore alone on a meadow at (4, 1) (make shots-piece with bore-kit-shots.json); headless EEVEE stills of
a portal-bore-bore-portal tunnel on a stand-in (eevee_still.py: evidence only, Blender's light is not Bevy's); the
cross-section (section.py); the closed portal's exterior, the approved still beside the open build rendered the
same way with the runtime's back-face culling (cull_runtime.py), with exterior_diff.py's numbers.
Usage: python3 docs/checkpoints/road_tunnel_bore/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/road_tunnel_bore"
PORTAL = ROOT / "docs/checkpoints/road_tunnel_portal"
BAKE = json.loads((ROOT / "assets-source/kit/road_tunnel_bore/bake-report.json").read_text())
COMP = json.loads((ROOT / "assets-source/kit/road_tunnel_bore/compress-report.json").read_text())
PBAKE = json.loads((ROOT / "assets-source/kit/road_tunnel_portal/bake-report.json").read_text())
DIFF = json.loads((DOCS / "exterior-diff-culled.json").read_text())
RAW = json.loads((DOCS / "exterior-diff.json").read_text())
W, H = 640, 400


def dn(v):
    s = DIFF[v]["beforeVsAfter"]
    return f"max {s['max']}/255, {s['pixelsOver8']} px > 8"


CELLS = [
    (DOCS / "bore-kit-inside.jpg", "in engine: inside the bore, lamps every 9.24 m, road on the floor"),
    (DOCS / "bore-kit-lamp.jpg", "in engine: lamp glass and its always-on glow on the lining"),
    (DOCS / "bore-kit-mid.jpg", "in engine: mid-bore, looking down the tube"),
    (DOCS / "bore-kit-outside.jpg", "in engine: the bore alone on a meadow (shell; cover cell goes over it)"),
    (DOCS / "bore-kit-lod1.jpg", "in engine: overview from 226 m (LOD not confirmed; EEVEE row shows LODs)"),
    (DOCS / "bore-kit-lod2.jpg", "in engine: overview from 467 m (LOD not confirmed; EEVEE row shows LODs)"),
    (DOCS / "bore-mouth.jpg", "EEVEE: the bore's interior from the portal mouth (back slot hidden)"),
    (DOCS / "bore-mid.jpg", "EEVEE: mid-bore"),
    (DOCS / "bore-car.jpg", "EEVEE: a car in the bore"),
    (DOCS / "bore-lamp.jpg", "EEVEE: lamp glow (emissive map at strength 1)"),
    (DOCS / "bore-seam_bb.jpg", "EEVEE: bore-bore seam at the cell edge"),
    (DOCS / "bore-seam_bp.jpg", "EEVEE: bore-portal seam at X_E (portal lining's dark band ends here)"),
    (DOCS / "bore-alone.jpg", "EEVEE: LOD0 alone"),
    (DOCS / "bore-alone-lod1.jpg", "EEVEE: LOD1 alone"),
    (DOCS / "bore-alone-lod2.jpg", "EEVEE: LOD2 alone"),
    (PORTAL / "portal-eye.jpg", "closed portal, APPROVED still (eye level)"),
    (DOCS / "portal-closed-after-eye.jpg", f"closed portal, open build, eye ({dn('eye')} vs before)"),
    (PORTAL / "portal-front.jpg", "closed portal, APPROVED still (front)"),
    (DOCS / "portal-closed-after-front.jpg", f"closed portal, open build, front ({dn('front')} vs before)"),
    (DOCS / "section.png", "cross-section, LOD0, with the numbers"),
]
COLS = 3
tris, ptris = BAKE["lodTris"], PBAKE["lodTris"]
kb = COMP["budgets"]["ktxBytes"]
TITLE = (f"road_tunnel_bore review sheet. Bore LOD {tris[0]}/{tris[1]}/{tris[2]} tris (LOD2 floor {BAKE['bore']['lod2Floor']}); "
         f"stone {BAKE['texelDensityPxPerM']['stone']['median']:.0f} px/m median; KTX2 {kb['bc7']:,} B BC7 / {kb['astc']:,} B "
         f"ASTC; pack core, kind prop. Open portal LOD {ptris[0]}/{ptris[1]}/{ptris[2]} tris.")
NOTE = (f"Closed portal before vs after with the runtime's back-face culling: three_quarter {dn('three_quarter')}, front "
        f"{dn('front')}, eye {dn('eye')}. With culling off (the portal still's default) {RAW['eye']['beforeVsAfter']['pixelsOver8']} "
        "eye px differ inside the 15 mm voussoir joints: the back patch's back side, a face the game never draws. Pending owner review.")
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
