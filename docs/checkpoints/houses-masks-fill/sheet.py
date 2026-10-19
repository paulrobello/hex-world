"""houses-masks-fill review sheet: the approved reference, then per house the textured headless EEVEE
stills before and after the masks fill (same cameras; masks.A on UV0, nearest, so an unbaked texel reads
black), labelled with the centroid sampler's unbaked-triangle counts per LOD and the LOD0 masks density.
Evidence only (Blender's AgX is not Bevy's tonemapper); no in-engine cells until the yaw-lighting fix.
Usage: python3 docs/checkpoints/houses-masks-fill/sheet.py <stills dir> (from the repo root; needs Pillow)."""
import json
import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/houses-masks-fill"
STILLS = pathlib.Path(sys.argv[1])
W, H = 600, 450


def counts(name):
    data = {p["piece"]: p for p in json.loads((DOCS / name).read_text())}
    out = {}
    for pid, p in data.items():
        lods = p["lods"]
        out[pid] = ("unbaked tris " + "/".join(str(lods[f"lod{i}"]["unbakedTris"]) for i in range(3))
                    + f", LOD0 masks {lods['lod0']['densityPxPerM']['median']} px/m")
    return out


before, after = counts("centroids-before.json"), counts("centroids-after.json")


def cell(path, label):
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


rows = []
for pid in ("house_cottage", "house_twostorey"):
    for v in ("front", "back"):
        rows.append([cell(STILLS / f"{pid}-{v}-before.png", f"{pid} {v} BEFORE: {before[pid]}"),
                     cell(STILLS / f"{pid}-{v}-after.png", f"{pid} {v} AFTER: {after[pid]}")])
# 3x zooms of the front stills where the fill changed most (black strips: frame returns, timber and
# tile-course butts).
for pid, (x, y) in (("house_cottage", (800, 540)), ("house_twostorey", (360, 460))):
    zoom = []
    for w in ("before", "after"):
        im = Image.open(STILLS / f"{pid}-front-{w}.png").convert("RGB").crop((x, y, x + 200, y + 150))
        path = STILLS / f"{pid}-front-{w}-zoom.png"
        im.resize((600, 450), Image.NEAREST).save(path)
        zoom.append(cell(path, f"{pid} front {w.upper()}, 3x zoom at ({x}, {y})"))
    rows.append(zoom)
refs = ROOT / "assets-source/refs/house_cluster"
rows.insert(0, [cell(refs / "ref-1.jpg", "reference ref-1 (approved)"), cell(refs / "ref-2.jpg", "reference ref-2 (approved)")])
cw, ch = rows[0][0].size
sheet = Image.new("RGB", (cw * 2, ch * len(rows) + 30), (24, 24, 24))
ImageDraw.Draw(sheet).text((8, 8), "houses masks fill: EEVEE, masks.A nearest on UV0 (evidence only). Geometry and tris unchanged; "
                           "unbaked counts are LOD0/LOD1/LOD2 on the compressed GLB.", fill=(235, 235, 235))
for r, row in enumerate(rows):
    for k, c in enumerate(row):
        sheet.paste(c, (k * cw, 30 + r * ch))
sheet.save(DOCS / "sheet.jpg", quality=85)
for pid in ("house_cottage", "house_twostorey"):
    for v in ("front", "back"):
        Image.open(STILLS / f"{pid}-{v}-after.png").convert("RGB").save(DOCS / f"{pid}-{v}-after.jpg", quality=85)
        Image.open(STILLS / f"{pid}-{v}-before.png").convert("RGB").save(DOCS / f"{pid}-{v}-before.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
