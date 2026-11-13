"""Composes the massif_granite form pass 2 (or 3) review sheet (JPEG): the raised grassy flanks (owner
A55 Q1 O1) beside ref-2 and the previous pass, every still headless EEVEE at the same light. Pass 3
(the grass alone rises, the rock as the form pass left it) compares with the original piece, the
form pass before pass 2.

Run: python3 docs/checkpoints/massif_granite/form_pass2_sheet.py <stills dir> <previous stills dir> <out.jpg> [2|3]
Needs Pillow and numpy. Both stills dirs hold review_render.py's `ref2cam-o1.png`, `play-o1.png`,
`low-o1.png` and `top-o1.png`, form_measure.py's `form-measure.json` and `topdown-classes.npy`, and
silhouette_iou.py's `iou-ref2.json` and `silhouette-overlay-ref2.png` (ref-2) and `iou.json` (ref-4).
Row 1: ref-2, this pass at ref-2's fitted camera, the previous pass there. Row 2: the play camera
(this pass, the previous pass) and this pass top-down with the rock and scree outlined. Row 3: the
low camera (this pass, the previous pass) and the silhouette overlay against ref-2.
"""
import json
import math
import pathlib
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

STILLS, PREV, OUT = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
PASS = sys.argv[4] if len(sys.argv) > 4 else "2"
assert PASS in ("2", "3"), PASS
NEW = f"pass {PASS}"
OLD = "previous pass" if PASS == "2" else "original (form pass)"
TITLE = ("form pass 2 (A55 Q1 O1, taller grassy flanks)" if PASS == "2" else
         "form pass 3 (A55 Q1 O1, only the grass rises; the form pass's rock)")
REFS = pathlib.Path(__file__).resolve().parents[3] / "assets-source/refs/massif_granite"
W, H = 800, 560


def load(d):
    m = json.load(open(d / "form-measure.json"))
    fz = np.array([v["footZ"] for v in m["groundProfile"]["perBearing"].values()])
    return m, float(np.median(fz)), json.load(open(d / "iou-ref2.json"))["best"], json.load(open(d / "iou.json"))["best"]


new, new_foot, new_i2, new_i4 = load(STILLS)
old, old_foot, old_i2, old_i4 = load(PREV)


def top_with_outline(d):
    im = Image.open(d / "top-o1.png").convert("RGB")
    n = im.width
    half = 46.0
    cls = np.load(d / "topdown-classes.npy")
    step, ext = 0.25, 45.0
    px = np.arange(n)
    wx = (px + 0.5) / n * 2 * half - half
    wy = half - (px + 0.5) / n * 2 * half
    ii = np.clip(np.round((wx + ext) / step).astype(int), 0, cls.shape[1] - 1)
    jj = np.clip(np.round((wy + ext) / step).astype(int), 0, cls.shape[0] - 1)
    big = cls[jj[:, None], ii[None, :]]
    a = np.asarray(im).copy()
    for value, colour in ((3, (255, 220, 0)), (2, (0, 230, 255))):
        msk = big == value
        edge = msk & ~(np.roll(msk, 1, 0) & np.roll(msk, -1, 0) & np.roll(msk, 1, 1) & np.roll(msk, -1, 1))
        edge = edge | np.roll(edge, 1, 0) | np.roll(edge, 1, 1)
        a[edge] = colour
    im = Image.fromarray(a)
    d2 = ImageDraw.Draw(im)
    corners = [(16.0 * math.cos(math.radians(30 + 60 * k)), 16.0 * math.sin(math.radians(30 + 60 * k))) for k in range(7)]
    d2.line([((x + half) / (2 * half) * n, (half - y) / (2 * half) * n) for x, y in corners], fill=(255, 255, 255), width=2)
    return im


band = f", tile level {new['rimBand']['maxInM']:.1f} m in" if "rimBand" in new else ""
cells = [
    (REFS / "ref-2.jpg", "approved ref-2 (A53)"),
    (STILLS / "ref2cam-o1.png", f"{NEW} at ref-2's camera: foot median {new_foot:.1f} m up, top {new['topM']:.1f} m"),
    (PREV / "ref2cam-o1.png", f"{OLD} there: foot median {old_foot:.1f} m up, top {old['topM']:.1f} m"),
    (STILLS / "play-o1.png", f"{NEW}, play camera ~65 deg, meadow_base east"),
    (PREV / "play-o1.png", f"{OLD}, play camera"),
    (None, f"{NEW} top-down: rock {new['rockShare'] * 100:.1f}% (yellow; was {old['rockShare'] * 100:.1f}%), scree (cyan){band}"),
    (STILLS / "low-o1.png", f"{NEW}, low camera ~12 deg: the flanks rise to the rock foot"),
    (PREV / "low-o1.png", f"{OLD}, low camera"),
    (STILLS / "silhouette-overlay-ref2.png",
     f"vs ref-2: IoU {new_i2['iou']:.2f} (was {old_i2['iou']:.2f}); ref-4 {new_i4['iou']:.2f} (was {old_i4['iou']:.2f})"),
]
font = ImageFont.load_default(size=20)
sheet = Image.new("RGB", (3 * W, 3 * (H + 32) + 40), (30, 30, 30))
d = ImageDraw.Draw(sheet)
d.text((10, 8), f"massif_granite, {TITLE}: headless EEVEE stills, same sun "
       "(Blender view transform, not Bevy's; runtime grass and pines not shown). Overlay: red ref only, blue ours only",
       fill=(230, 230, 230), font=font)
for k, (path, label) in enumerate(cells):
    im = top_with_outline(STILLS) if path is None else Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    x, y = (k % 3) * W, 40 + (k // 3) * (H + 32)
    sheet.paste(im, (x + (W - im.width) // 2, y + (H - im.height) // 2))
    d.text((x + 8, y + H + 5), label, fill=(230, 230, 230), font=font)
sheet.save(OUT, quality=85)
print(OUT, sheet.size)
