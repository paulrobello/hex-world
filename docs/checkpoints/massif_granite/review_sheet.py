"""Composes the massif_granite owner review sheet (JPEG) for the form pass.

Run: python3 docs/checkpoints/massif_granite/review_sheet.py <stills dir> <previous stills dir> <out.jpg>
Needs Pillow and numpy. The stills dir holds review_render.py's `ref2cam-o1.png`, `ref4cam-o1.png`,
`play-o1.png`, `close-o1.png` and `top-o1.png`, silhouette_iou.py's `silhouette-overlay.png` and
form_measure.py's `topdown-classes.npy` and `form-measure.json`; the previous stills dir holds the
column-rock pass rendered at ref-2's camera (`ref2cam-o1.png`). Row 1: the approved ref-2, the
massif at ref-2's fitted camera, the play camera with meadow_base east. Row 2: ref-4, the massif at
ref-4's fitted low camera, the silhouette overlay. Row 3: the cliff-foot close-up, the top-down
view with the rock coverage outlined, the previous pass at ref-2's camera. Every still is headless
EEVEE at the same light.
"""
import json
import math
import pathlib
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

STILLS, PREV, OUT = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
REFS = pathlib.Path(__file__).resolve().parents[3] / "assets-source/refs/massif_granite"
W, H = 800, 560
m = json.load(open(STILLS / "form-measure.json"))
iou = json.load(open(STILLS / "iou.json")) if (STILLS / "iou.json").exists() else None


def top_with_outline():
    """The top-down still with the rock (yellow) and scree (cyan) outlines from the class map, and
    the centre cell's outline (white): rock past it reaches into the ring cells."""
    im = Image.open(STILLS / "top-o1.png").convert("RGB")
    n = im.width
    half = 46.0
    cls = np.load(STILLS / "topdown-classes.npy")
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
    d = ImageDraw.Draw(im)
    corners = [(16.0 * math.cos(math.radians(30 + 60 * k)), 16.0 * math.sin(math.radians(30 + 60 * k))) for k in range(7)]
    d.line([((x + half) / (2 * half) * n, (half - y) / (2 * half) * n) for x, y in corners], fill=(255, 255, 255), width=2)
    return im


rock = m["rockShare"]
cells = [
    (REFS / "ref-2.jpg", "approved ref-2 (A53)"),
    (STILLS / "ref2cam-o1.png", "form pass at ref-2's fitted camera (27.9 deg, 183 m)"),
    (STILLS / "play-o1.png", "play camera ~65 deg, meadow_base east"),
    (REFS / "ref-4.jpg", "ref-4, the silhouette guide"),
    (STILLS / "ref4cam-o1.png", "low: ref-4's fitted camera (13.4 deg)"),
    (STILLS / "silhouette-overlay.png",
     f"silhouette vs ref-4: IoU {iou['best']['iou']:.2f}, above rim {iou['best']['iouAboveRim']:.2f} "
     "(red ref only, blue ours only)" if iou else "silhouette vs ref-4"),
    (STILLS / "close-o1.png", "cliff foot, sun side, ~30 m"),
    (None, f"top-down: rock {rock * 100:.1f}% of the 7 cells (yellow), scree (cyan), centre cell (white)"),
    (PREV / "ref2cam-o1.png", "previous pass (column rock) at ref-2's camera"),
]
font = ImageFont.load_default(size=20)
sheet = Image.new("RGB", (3 * W, 3 * (H + 32) + 40), (30, 30, 30))
d = ImageDraw.Draw(sheet)
d.text((10, 8), "massif_granite, form pass: headless EEVEE stills, texture route O1, same sun in every still "
       "(Blender view transform, not Bevy's; runtime grass blades and scatter pines are not shown)",
       fill=(230, 230, 230), font=font)
for k, (path, label) in enumerate(cells):
    im = top_with_outline() if path is None else Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    x, y = (k % 3) * W, 40 + (k // 3) * (H + 32)
    sheet.paste(im, (x + (W - im.width) // 2, y + (H - im.height) // 2))
    d.text((x + 8, y + H + 5), label, fill=(230, 230, 230), font=font)
sheet.save(OUT, quality=85)
print(OUT, sheet.size)
