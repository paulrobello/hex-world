"""Fit a pinhole camera to a reference image from flower-outline corners, then back-project a traced
rock-foot polygon onto the ground at an assumed foot height. World frame of the fit: x right, y away
from the camera, z up, the centre cell at the origin (the ref's own frame; a rotation of the piece's
frame by a multiple of 60 degrees, so the outline is the same set of points)."""
import json
import math
import sys

import numpy as np

C = 16.0 * math.sqrt(3.0) / 2.0


def homography(world, img):
    A = []
    for (X, Y), (u, v) in zip(world, img):
        A.append([-X, -Y, -1, 0, 0, 0, u * X, u * Y, u])
        A.append([0, 0, 0, -X, -Y, -1, v * X, v * Y, v])
    _, _, vt = np.linalg.svd(np.array(A))
    H = vt[-1].reshape(3, 3)
    return H / H[2, 2]


def project_h(H, pts):
    p = np.column_stack([pts, np.ones(len(pts))]) @ H.T
    return p[:, :2] / p[:, 2:3]


def camera_from_h(H, w, h):
    """K, R, t from a ground homography, principal point at the image centre, square pixels."""
    T = np.array([[1, 0, -w / 2], [0, 1, -h / 2], [0, 0, 1]], float)
    Hp = T @ H
    a1, b1, c1 = Hp[:, 0]
    a2, b2, c2 = Hp[:, 1]
    f2a = -(a1 * a2 + b1 * b2) / (c1 * c2)
    f2b = (a1 * a1 + b1 * b1 - a2 * a2 - b2 * b2) / (c2 * c2 - c1 * c1)
    cands = [x for x in (f2a, f2b) if x > 0]
    f = math.sqrt(sum(cands) / len(cands))
    K = np.array([[f, 0, w / 2], [0, f, h / 2], [0, 0, 1]], float)
    M = np.linalg.inv(K) @ H
    lam = 2.0 / (np.linalg.norm(M[:, 0]) + np.linalg.norm(M[:, 1]))
    r1, r2, t = lam * M[:, 0], lam * M[:, 1], lam * M[:, 2]
    if t[2] < 0:
        r1, r2, t = -r1, -r2, -t
    r3 = np.cross(r1, r2)
    R = np.column_stack([r1, r2, r3])
    U, _, Vt = np.linalg.svd(R)
    R = U @ Vt
    return K, R, t, (f2a, f2b)


def project(K, R, t, P):
    c = (R @ np.asarray(P, float).T).T + t
    p = (K @ c.T).T
    return p[:, :2] / p[:, 2:3]


def backproject(K, R, t, uv, z):
    """Image points onto the plane at height z."""
    P = K @ np.column_stack([R, t])
    Hz = np.column_stack([P[:, 0], P[:, 1], z * P[:, 2] + P[:, 3]])
    p = np.column_stack([uv, np.ones(len(uv))]) @ np.linalg.inv(Hz).T
    return p[:, :2] / p[:, 2:3]


def poly_area(xy):
    x, y = xy[:, 0], xy[:, 1]
    return 0.5 * abs(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))


def main():
    cfg = json.load(open(sys.argv[1]))
    world = np.array([p[0] for p in cfg["corners"]], float)
    img = np.array([p[1] for p in cfg["corners"]], float)
    H = homography(world, img)
    res = np.linalg.norm(project_h(H, world) - img, axis=1)
    K, R, t, f2 = camera_from_h(H, cfg["w"], cfg["h"])
    res_cam = np.linalg.norm(project(K, R, t, np.column_stack([world, np.zeros(len(world))])) - img, axis=1)
    cam = -R.T @ t
    fwd = R.T @ np.array([0, 0, 1.0])
    elev = math.degrees(math.asin(-fwd[2]))
    vfov = math.degrees(2 * math.atan(cfg["h"] / 2 / K[0, 0]))
    out = {"homographyResidualPx": {"mean": round(float(res.mean()), 1), "max": round(float(res.max()), 1)},
           "cameraResidualPx": {"mean": round(float(res_cam.mean()), 1), "max": round(float(res_cam.max()), 1)},
           "focalPx": round(K[0, 0], 1), "f2candidates": [round(x, 1) for x in f2], "vfovDeg": round(vfov, 2),
           "camPos": [round(float(c), 2) for c in cam], "camFwd": [round(float(c), 4) for c in fwd],
           "elevDeg": round(elev, 2), "dist": round(float(np.linalg.norm(cam)), 1),
           "R": R.tolist(), "t": t.tolist(), "K": K.tolist()}
    if "foot" in cfg:
        foot = np.array(cfg["foot"], float)
        cx, cy = cfg.get("centre", [0.0, 0.0])
        out["foot"] = {}
        for z in cfg.get("footHeights", [0.0, 4.0, 8.0]):
            g = backproject(K, R, t, foot, z)
            r = np.hypot(g[:, 0] - cx, g[:, 1] - cy)
            th = np.degrees(np.arctan2(g[:, 1] - cy, g[:, 0] - cx))
            # Front-half area: the polygon closed through the centre, doubled (the back is hidden).
            closed = np.vstack([g, [[cx, cy]]])
            area = poly_area(closed)
            span = (math.degrees(math.atan2(g[-1, 1] - cy, g[-1, 0] - cx)) - math.degrees(math.atan2(g[0, 1] - cy, g[0, 0] - cx))) % 360.0
            out["foot"][str(z)] = {"rMax": round(float(r.max()), 1), "rMedian": round(float(np.median(r)), 1),
                                   "rMin": round(float(r.min()), 1),
                                   "frontArea": round(area, 0), "spanDeg": round(span, 0), "areaFull": round(area * 360.0 / span, 0),
                                   "shareOf7Hex": round(area * 360.0 / span / (7 * 1.5 * math.sqrt(3) * 256.0), 3),
                                   "points": [[round(float(a), 1), round(float(b), 1), round(float(c), 0)]
                                              for (a, b), c in zip(g, th)]}
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
