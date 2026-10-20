"""Before/after review sheet for hay_bale_round_b's LOD2 straw-apron seam: in engine (run-web.sh, web
build in headless GPU Chromium, tilt off) at Force LOD 2 and 0 from the same cameras, then textured
EEVEE stills (still.py) cropped to the apron's front corners, where the seams are, at LOD2 and LOD0,
then the measured steps (measure-{before,after}.json from docs/checkpoints/hay-bale-seams/measure.py).
Usage: python3 docs/checkpoints/hay-bale-b-apron/sheet.py <scratch dir> (holding before-web/,
after-web/ and stills/). Crop boxes are in the 1280x800 web frame and the 1000x800 still frame.
Writes docs/checkpoints/hay-bale-b-apron/sheet.jpg."""
import json
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

DOCS = pathlib.Path(__file__).resolve().parent
scr = pathlib.Path(sys.argv[1])
CAMS = [
    ("apron-high-a", (380, 40, 940, 580), "round_b in slot (7.5, -0.4), from above the sun side"),
    ("apron-high-b", (330, 60, 950, 680), "round_b in slot (10.4, -1.4), from above the shade side"),
    ("apron-sun", (336, 176, 944, 784), "round_b, sun side at bale height"),
]
COLS = [("before", "lod2", "before, LOD2 (forced)"), ("after", "lod2", "after, LOD2 (forced)"),
        ("before", "lod0", "before, LOD0 (forced)"), ("after", "lod0", "after, LOD0 (forced)")]
STILL_BOX = (120, 480, 860, 800)
T, LABEL, HEAD = 400, 26, 96
W = len(COLS) * T
font = ImageFont.load_default(size=18)
big = ImageFont.load_default(size=24)
still_h = round((STILL_BOX[3] - STILL_BOX[1]) * (W / 2 - 6) / (STILL_BOX[2] - STILL_BOX[0]))
CORNER_BOX = (310, 650, 576, 800)
corner_h = round((CORNER_BOX[3] - CORNER_BOX[1]) * (W / 2 - 6) / (CORNER_BOX[2] - CORNER_BOX[0]))
rows_h = len(CAMS) * (T + LABEL) + 2 * (still_h + LABEL + 6) + corner_h + LABEL + 6
table_h = 250
sheet = Image.new("RGB", (W, HEAD + rows_h + table_h), (24, 24, 24))
d = ImageDraw.Draw(sheet)
d.text((10, 10), "hay_bale_round_b LOD2 straw-apron seam: before / after (same cameras, tilt off)", fill=(235, 235, 235), font=big)
d.text((10, 42), "In engine (web build, headless GPU Chromium). " + "  ".join(f"col {i + 1}: {c[2]}" for i, c in enumerate(COLS)),
       fill=(190, 190, 190), font=font)
d.text((10, 66), "Forced LOD2 hides most of the apron in engine (its 60-degree chords run inside the bale's foot, and the LOD2 meadow"
       " most likely covers more); the EEVEE rows below show it whole.", fill=(190, 190, 190), font=font)
y = HEAD
for shot, box, label in CAMS:
    d.text((10, y + 3), label, fill=(235, 220, 160), font=font)
    for c, (tag, lod, _name) in enumerate(COLS):
        img = Image.open(scr / f"{tag}-web" / f"{shot}-{lod}.png").convert("RGB")
        img = img.crop(box).resize((T - 6, T - 6), Image.LANCZOS)
        sheet.paste(img, (c * T + 3, y + LABEL))
    y += T + LABEL
for lod in ("lod2", "lod0"):
    d.text((10, y + 3), f"Textured EEVEE still of {lod.upper()} alone, apron front corners (the six seams sit on the hexagon's"
           " corners; left before, right after; ground 0.1 m low, so the gap at the foot is open ground)",
           fill=(235, 220, 160), font=font)
    for c, tag in enumerate(("before", "after")):
        img = Image.open(scr / "stills" / f"{tag}-{lod}-high.png").convert("RGB")
        img = img.crop(STILL_BOX).resize((W // 2 - 6, still_h), Image.LANCZOS)
        sheet.paste(img, (c * (W // 2) + 3, y + LABEL))
    y += still_h + LABEL + 6
    if lod == "lod2":
        d.text((10, y + 3), "LOD2, one hexagon corner (a segment seam) at 3x: left before, right after",
               fill=(235, 220, 160), font=font)
        for c, tag in enumerate(("before", "after")):
            img = Image.open(scr / "stills" / f"{tag}-lod2-high.png").convert("RGB")
            img = img.crop(CORNER_BOX).resize((W // 2 - 6, corner_h), Image.LANCZOS)
            sheet.paste(img, (c * (W // 2) + 3, y + LABEL))
        y += corner_h + LABEL + 6
m = {t: json.loads((DOCS / f"measure-{t}.json").read_text()) for t in ("before", "after")}
y += 8
d.text((10, y), "Measured on the shipped GLB + maps (hay-bale-seams/measure.py: both sides of every shared edge, one albedo texel"
       " in; normal step in degrees, albedo step in sRGB 0-255):", fill=(235, 235, 235), font=font)
y += 28
for lod in (0, 1, 2):
    k = f"hay_bale_round_b_lod{lod}"
    s = {t: m[t][k]["seam apron|apron"] for t in m}
    base = m["after"][k]["interior apron|apron"]
    d.text((10, y), (f"LOD{lod} apron seam: normal step {s['before']['dNormalDeg']['mean']:.2f} -> {s['after']['dNormalDeg']['mean']:.2f} deg"
                     f" (p95 {s['before']['dNormalDeg']['p95']:.1f} -> {s['after']['dNormalDeg']['p95']:.1f}),"
                     f" albedo {s['before']['dRGB']['mean']:.1f} -> {s['after']['dRGB']['mean']:.1f}"
                     f"   (apron interior edges: {base['dNormalDeg']['mean']:.2f} deg, albedo {base['dRGB']['mean']:.1f})"),
           fill=(210, 210, 210), font=font)
    y += 26
lines = [
    "Cause: at LOD2 each 60-degree apron segment is one quad with its own vertices, so a seam vertex carried that quad's facet",
    "normal, 27 deg off LOD0's (41 deg between the two sides); the tangent normal map was baked against LOD0's normals.",
    "Fix: LOD2's apron corners take LOD0's vertex normal for the same island at the same point (now 0.00 deg off). Same tris",
    "(792 / 240 / 76), positions, UVs and maps; LOD0 and LOD1 decode identical (compare.py). LOD2 barrel seam for scale: 4.32 deg.",
]
for line in lines:
    d.text((10, y), line, fill=(190, 190, 190), font=font)
    y += 24
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", sheet.size)
