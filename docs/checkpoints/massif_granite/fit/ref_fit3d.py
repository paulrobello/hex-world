"""Full 3x4 camera (DLT) fit to a reference image from flower-outline corners at z = 0 and skirt
bottoms at z = -4 (non-coplanar, so heights are conditioned), its K R t, a Blender camera, and
summit-height estimates. World frame of the fit: x right, y away from the camera, z up, the centre
cell at the origin."""
import json
import math
import sys

import numpy as np


def dlt(Xw, uv):
    A = []
    for (X, Y, Z), (u, v) in zip(Xw, uv):
        A.append([X, Y, Z, 1, 0, 0, 0, 0, -u * X, -u * Y, -u * Z, -u])
        A.append([0, 0, 0, 0, X, Y, Z, 1, -v * X, -v * Y, -v * Z, -v])
    _, _, vt = np.linalg.svd(np.array(A, float))
    P = vt[-1].reshape(3, 4)
    return P / np.linalg.norm(P[2, :3])


def proj(P, Xw):
    Xh = np.column_stack([Xw, np.ones(len(Xw))])
    p = Xh @ P.T
    return p[:, :2] / p[:, 2:3]


def rq(M):
    Q, R = np.linalg.qr(np.flipud(M).T)
    R = np.flipud(R.T)[:, ::-1]
    Q = Q.T[::-1, :]
    T = np.diag(np.sign(np.diag(R)))
    return R @ T, T @ Q


def decompose(P):
    K, R = rq(P[:, :3])
    if np.linalg.det(R) < 0:
        R = -R
        P = -P
        K, R = rq(P[:, :3])
    s = K[2, 2]
    K = K / s
    t = np.linalg.solve(K, P[:, 3] / s)
    if (R @ np.zeros(3) + t)[2] < 0:
        pass
    return K, R, t


def main():
    cfg = json.load(open(sys.argv[1]))
    Xw = np.array([c[0] if len(c[0]) == 3 else c[0] + [0.0] for c in cfg["points"]], float)
    uv = np.array([c[1] for c in cfg["points"]], float)
    P = dlt(Xw, uv)
    res = np.linalg.norm(proj(P, Xw) - uv, axis=1)
    K, R, t = decompose(P)
    cam = -R.T @ t
    # Camera looks along +z of its frame; make sure the scene is in front.
    if ((Xw - cam) @ R[2]).mean() < 0:
        R = -R
        t = -t
        cam = -R.T @ t
    fwd = R[2]
    elev = math.degrees(math.asin(-fwd[2] / np.linalg.norm(fwd)))
    out = {"residualPx": {"mean": round(float(res.mean()), 1), "max": round(float(res.max()), 1)},
           "K": K.round(2).tolist(), "camPos": cam.round(2).tolist(), "fwd": fwd.round(4).tolist(),
           "elevDeg": round(elev, 2), "dist": round(float(np.linalg.norm(cam)), 1),
           "R": R.tolist(), "t": t.tolist(), "P": P.tolist(), "w": cfg["w"], "h": cfg["h"]}
    # Summit height if the summit stands over each candidate ground point (the drawn summit pixel).
    if "summit" in cfg:
        su, sv = cfg["summit"]
        est = {}
        for gx, gy in cfg.get("summitXY", [[0, 0], [-3, -3], [-3, 3], [3, -3]]):
            # Solve for z such that proj(gx, gy, z) has image v = sv (u is reported for checking).
            zs = np.linspace(0, 80, 8001)
            p = proj(P, np.column_stack([np.full_like(zs, gx), np.full_like(zs, gy), zs]))
            k = int(np.argmin(np.abs(p[:, 1] - sv)))
            est[f"{gx},{gy}"] = {"z": round(float(zs[k]), 1), "u": round(float(p[k, 0]), 0)}
        out["summitHeight"] = est
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
