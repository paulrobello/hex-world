"""Before/after review sheet for the round bale sector seams (owner Q55): each run.sh camera cropped
to its bale, before and after, at LOD0 (automatic) and Force LOD 1, plus the measured seam steps from
measure-{before,after}-<piece>.json. Usage: python3 docs/checkpoints/hay-bale-seams/sheet.py <scratch dir> [suffix]
(the dir run.sh or run-web.sh wrote into: <tag><suffix>/, suffix "-web" for run-web.sh). Crop boxes
are in the native 1600x1000 frame and scale with the shot. Writes docs/checkpoints/hay-bale-seams/sheet.jpg."""
import json
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

DOCS = pathlib.Path(__file__).resolve().parent
scr = pathlib.Path(sys.argv[1])
suffix = sys.argv[2] if len(sys.argv) > 2 else ""
where = "web build, headless GPU Chromium" if suffix == "-web" else "native"
# (shot, crop box in the 1600x1000 frame, label)
CAMS = [
    ("seam-round-barrel", (360, 100, 1240, 980), "hay_bale_round, barrel (seams at 45 deg from the top)"),
    ("seam-round-cap", (380, 80, 1300, 1000), "hay_bale_round, +x cap and its rim"),
    ("seam-roundb-sun", (420, 220, 1180, 980), "hay_bale_round_b, sun side"),
    ("seam-roundb-near", (360, 180, 1260, 1000), "hay_bale_round_b, near (a vertical seam mid-frame)"),
]
COLS = [("before", "auto", "before, LOD0"), ("after", "auto", "after, LOD0"),
        ("before", "lod1", "before, LOD1 (forced)"), ("after", "lod1", "after, LOD1 (forced)")]
T, LABEL, HEAD = 420, 26, 70
font = ImageFont.load_default(size=18)
big = ImageFont.load_default(size=24)
rows_h = len(CAMS) * (T + LABEL)
table_h = 260
sheet = Image.new("RGB", (len(COLS) * T, HEAD + rows_h + table_h), (24, 24, 24))
d = ImageDraw.Draw(sheet)
d.text((10, 10), f"Round hay bale sector seams (Q55): before / after, same cameras, in engine ({where}, tilt off)",
       fill=(235, 235, 235), font=big)
d.text((10, 42), "  ".join(f"col {i + 1}: {c[2]}" for i, c in enumerate(COLS)), fill=(190, 190, 190), font=font)
for r, (shot, box, label) in enumerate(CAMS):
    y = HEAD + r * (T + LABEL)
    d.text((10, y + 3), label, fill=(235, 220, 160), font=font)
    for c, (tag, lod, _name) in enumerate(COLS):
        img = Image.open(scr / f"{tag}{suffix}" / f"{shot}-{lod}.png").convert("RGB")
        k = img.width / 1600
        img = img.crop(tuple(round(b * k) for b in box)).resize((T - 6, T - 6), Image.LANCZOS)
        sheet.paste(img, (c * T + 3, y + LABEL))
y = HEAD + rows_h + 8
d.text((10, y), "Measured on the shipped GLB + maps (measure.py; albedo step in sRGB 0-255, normal step in degrees,"
       " mean over 6 samples per edge one texel inside each side):", fill=(235, 235, 235), font=font)
y += 28
for piece in ("hay_bale_round", "hay_bale_round_b"):
    for lod in (0, 1):
        m = {t: json.loads((DOCS / f"measure-{t}-{piece}.json").read_text())[f"{piece}_lod{lod}"] for t in ("before", "after")}
        s = {t: m[t]["seam barrel|barrel"] for t in m}
        base = m["after"]["interior barrel|barrel"]
        d.text((10, y), (f"{piece} LOD{lod}: sector seam albedo {s['before']['dRGB']['mean']:.1f} -> {s['after']['dRGB']['mean']:.1f}"
                         f", normal {s['before']['dNormalDeg']['mean']:.1f} -> {s['after']['dNormalDeg']['mean']:.1f} deg"
                         f", normal-map edge tilt x {s['before']['tiltX']['left']:+.3f}/{s['before']['tiltX']['right']:+.3f}"
                         f" -> {s['after']['tiltX']['left']:+.3f}/{s['after']['tiltX']['right']:+.3f}"
                         f"   (interior edges: {base['dRGB']['mean']:.1f}, {base['dNormalDeg']['mean']:.1f} deg)"),
               fill=(210, 210, 210), font=font)
        y += 26
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", sheet.size)
