"""Road-verge band luminance and 4x contact crops from the road-verge.json shots.

Usage: python3 docs/checkpoints/road-verge/measure.py <label>   (label: before | after)

Reads target/shots/road-verge-*.png, writes docs/checkpoints/road-verge/luminance-<label>.json and
road-verge-<label>-crop4x-{sun,shade}.png (4x nearest-neighbour crops of the contact at x = CX
from the two play-height shots). The road_straight cells sit at (1, -1) and (2, -1) at rotation 0,
so the road runs along world +X through z = CZ = -24. The kit sun (KIT_SUN_DIR) is toward +x +z,
so the +z verge slope faces it ("sun") and the -z slope faces away ("shade"). The verge sink slope
spans |z - CZ| in [4.0, 4.6] (_road.PROFILE) and the meadow cap relief cuts it somewhere in
[4.1, 4.52]. Regions, sampled over x in cell (1, -1) +-5 m:
  band   |z - CZ| in [4.015, 4.085]  (slope above the cap at any relief)
  verge  |z - CZ| in [3.45, 3.90]    (flat verge grass)
  meadow |z - CZ| in [4.90, 5.80]    (meadow cap grass beyond the sink edge)
Luminance is relative luminance Y (Rec. 709 weights on sRGB-decoded values), mean over the region.
"""
import json
import math
import pathlib
import sys

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
SHOTS = ROOT / "target" / "shots"
DOCS = pathlib.Path(__file__).resolve().parent
CX, CZ = 16.0 * math.sqrt(3.0) * 0.5, -24.0  # cell (1, -1) centre, HEX_R = 16
FOV = math.pi / 4.0

CAMERAS = {  # must match road-verge.json
    "road-verge-play": ((CX - 12.0, 10.0, CZ + 10.0), (CX + 2.0, 0.0, CZ)),
    "road-verge-play-north": ((CX - 12.0, 10.0, CZ - 10.0), (CX + 2.0, 0.0, CZ)),
    "road-verge-top-sun": ((CX, 6.0, CZ + 4.8), (CX, 0.0, CZ + 4.5)),
    "road-verge-top-shade": ((CX, 6.0, CZ - 4.8), (CX, 0.0, CZ - 4.5)),
}
REGIONS = {"band": (4.015, 4.085), "verge": (3.45, 3.90), "meadow": (4.90, 5.80)}


def project(p, cam, w, h):
    c, t = (np.asarray(v, np.float64) for v in cam)
    f = (t - c) / np.linalg.norm(t - c)
    r = np.cross(f, (0.0, 1.0, 0.0))
    r /= np.linalg.norm(r)
    u = np.cross(r, f)
    d = np.asarray(p, np.float64) - c
    zc = d @ f
    k = math.tan(FOV / 2.0)
    nx, ny = (d @ r) / (zc * k * w / h), (d @ u) / (zc * k)
    return (nx + 1.0) / 2.0 * w, (1.0 - ny) / 2.0 * h


def luminance(img):
    s = img[..., :3].astype(np.float64) / 255.0
    lin = np.where(s <= 0.04045, s / 12.92, ((s + 0.055) / 1.055) ** 2.4)
    return lin @ np.array([0.2126, 0.7152, 0.0722])


def region_mean(lum, cam, sign, z0, z1, w, h):
    """Mean luminance over x in CX +- 5 m, z in CZ + sign * [z0, z1], sampled on a 0.01 m world grid."""
    vals = []
    for x in np.arange(CX - 5.0, CX + 5.0, 0.05):
        for z in np.arange(z0, z1 + 1e-9, 0.01):
            px, py = project((x, 0.0, CZ + sign * z), cam, w, h)
            i, j = int(py), int(px)
            if 0 <= i < h and 0 <= j < w:
                vals.append(lum[i, j])
    return float(np.mean(vals)), len(vals)


def crop4x(img, cam, sign, w, h, out):
    px, py = project((CX, 0.0, CZ + sign * 4.3), cam, w, h)
    x0, y0 = int(px) - 60, int(py) - 30
    tile = Image.fromarray(img[y0:y0 + 60, x0:x0 + 120, :3])
    tile.resize((480, 240), Image.NEAREST).save(out)


def main(label):
    res = {"label": label, "regions_m": REGIONS, "luminance": "relative Y, Rec. 709 on sRGB-decoded pixels"}
    for side, sign, shot in (("sun", 1.0, "road-verge-top-sun"), ("shade", -1.0, "road-verge-top-shade")):
        img = np.asarray(Image.open(SHOTS / f"{shot}.png").convert("RGB"))
        h, w = img.shape[:2]
        lum = luminance(img)
        side_res = {}
        for name, (z0, z1) in REGIONS.items():
            m, n = region_mean(lum, CAMERAS[shot], sign, z0, z1, w, h)
            side_res[name] = {"meanY": round(m, 4), "samples": n}
        side_res["band_over_meadow"] = round(side_res["band"]["meanY"] / side_res["meadow"]["meanY"], 3)
        side_res["band_over_verge"] = round(side_res["band"]["meanY"] / side_res["verge"]["meanY"], 3)
        res[side] = side_res
    # Each side is cropped from the play-height shot on its own side of the road (the slope faces it).
    for side, sign, shot in (("sun", 1.0, "road-verge-play"), ("shade", -1.0, "road-verge-play-north")):
        play = np.asarray(Image.open(SHOTS / f"{shot}.png").convert("RGB"))
        h, w = play.shape[:2]
        crop4x(play, CAMERAS[shot], sign, w, h, DOCS / f"road-verge-{label}-crop4x-{side}.png")
    (DOCS / f"luminance-{label}.json").write_text(json.dumps(res, indent=2) + "\n")
    print(json.dumps(res, indent=2))


if __name__ == "__main__":
    main(sys.argv[1])
