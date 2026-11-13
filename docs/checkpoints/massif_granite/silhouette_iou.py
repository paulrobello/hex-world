"""Silhouette IoU of massif_granite against ref-4 (or ref-2) at that reference's fitted camera
(brief target 5; form pass 2 adds ref-2).

Run: python3 docs/checkpoints/massif_granite/silhouette_iou.py <stills dir> <cam json> <out dir> [ref-4|ref-2]
Needs Pillow and numpy. The stills dir holds review_render.py's `mask4-<k>-o1.png` (`mask2-<k>-o1.png`
for ref-2: flat white on black, six bearings 60 degrees apart at the reference's camera, half size). ref-4's silhouette is every pixel
that is not the grey backdrop (flood-filled from the frame edge; cast shadows, grey and darker than
the backdrop, count as backdrop). Prints the whole-piece IoU and the IoU above the back rim (the
mountain against the sky) per bearing, and writes `silhouette-overlay.png` for the best bearing
(ref only: red, ours only: blue, both: white); for ref-2, `silhouette-overlay-ref2.png`."""
import json
import math
import pathlib
import sys

import numpy as np
from PIL import Image

STILLS, CAM, OUT = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
WHICH = sys.argv[4] if len(sys.argv) > 4 else "ref-4"
assert WHICH in ("ref-4", "ref-2"), WHICH
REF = pathlib.Path(__file__).resolve().parents[3] / f"assets-source/refs/massif_granite/{WHICH}.jpg"
MASK = "mask4" if WHICH == "ref-4" else "mask2"
cam = json.load(open(CAM))
W, H = cam["w"] // 2, cam["h"] // 2

im = np.asarray(Image.open(REF).convert("RGB").resize((W, H), Image.BILINEAR)).astype(np.float32)
bg = np.median(im[:, :12, :], axis=1)  # per-row backdrop from the left edge
diff = np.abs(im - bg[:, None, :]).sum(axis=2)
sat = im.max(axis=2) - im.min(axis=2)
val = im.mean(axis=2)
shadow = (sat < 14) & (val < bg.mean(axis=1)[:, None]) & (val > 70)
backdrop = (diff < 30) | shadow
# Flood the backdrop from the frame edge (iterated 4-neighbour dilation inside the backdrop);
# everything not reached is the piece.
reach = np.zeros_like(backdrop)
reach[0, :], reach[-1, :], reach[:, 0], reach[:, -1] = backdrop[0, :], backdrop[-1, :], backdrop[:, 0], backdrop[:, -1]
for _ in range(4000):
    grown = reach.copy()
    grown[1:] |= reach[:-1]
    grown[:-1] |= reach[1:]
    grown[:, 1:] |= reach[:, :-1]
    grown[:, :-1] |= reach[:, 1:]
    grown &= backdrop
    if (grown == reach).all():
        break
    reach = grown
ref = ~reach

# The back rim's image row: ref-4's frame has a ring cell toward the viewer, the back cell's far
# edge at y = 41.57 m.
R = np.array(cam["R"])
c = np.array(cam["pos"])
if WHICH == "ref-4":
    p = R @ (np.array([0.0, 41.57, 0.0]) - c)
    rim_v = (cam["f"] * p[1] / p[2] + cam["h"] / 2) / 2.0
else:
    # ref-2's frame has a notch toward the viewer: the back rim's top row is the highest of the
    # flower outline's corners.
    rows = []
    for k in range(6):
        a = math.radians(60 * k)  # ring cell centres along the edge normals, corners at 30 + 60 j
        cx, cy = 16.0 * math.sqrt(3.0) * math.cos(a), 16.0 * math.sqrt(3.0) * math.sin(a)
        for j in range(6):
            b = math.radians(30 + 60 * j)
            p = R @ (np.array([cx + 16.0 * math.cos(b), cy + 16.0 * math.sin(b), 0.0]) - c)
            rows.append((cam["f"] * p[1] / p[2] + cam["h"] / 2) / 2.0)
    rim_v = min(rows)
upper = np.zeros((H, W), bool)
upper[: int(rim_v)] = True

res = []
for k in range(6):
    f = STILLS / f"{MASK}-{k}-o1.png"
    if not f.exists():
        continue
    ours = np.asarray(Image.open(f).convert("L").resize((W, H))) > 127
    inter, union = (ours & ref).sum(), (ours | ref).sum()
    ui, uu = (ours & ref & upper).sum(), ((ours | ref) & upper).sum()
    res.append({"bearing": k, "azimDeg": (-120 if WHICH == "ref-4" else 0) + 60 * k, "iou": round(float(inter / union), 3),
                "iouAboveRim": round(float(ui / uu), 3) if uu else None,
                "oursAboveRimPx": int((ours & upper).sum()), "refAboveRimPx": int((ref & upper).sum())})
best = max(res, key=lambda r: r["iouAboveRim"] or 0)
ours = np.asarray(Image.open(STILLS / f"{MASK}-{best['bearing']}-o1.png").convert("L").resize((W, H))) > 127
ov = np.zeros((H, W, 3), np.uint8)
ov[ref & ~ours] = (220, 40, 40)
ov[ours & ~ref] = (40, 90, 230)
ov[ours & ref] = (235, 235, 235)
ov[int(rim_v), :] = (255, 200, 0)
OUT.mkdir(parents=True, exist_ok=True)
Image.fromarray(ov).save(OUT / ("silhouette-overlay.png" if WHICH == "ref-4" else "silhouette-overlay-ref2.png"))
# Height of each silhouette's top above the rim row, in rows.
def top_row(m):
    rows = np.nonzero(m.any(axis=1))[0]
    return int(rows.min()) if len(rows) else None
print(json.dumps({"rimRow": round(rim_v, 1), "refTopRow": top_row(ref), "oursTopRow": top_row(ours),
                  "perBearing": res, "best": best}, indent=1))
