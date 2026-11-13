"""Constrained pinhole fit (square pixels, principal point at the image centre, no skew) to a
reference's outline corners and skirt bottoms, by Levenberg-Marquardt from the DLT start. Writes the
camera (position, rotation, focal px) for Blender and prints the residuals and height estimates."""
import json
import math
import sys

import numpy as np

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent))
import ref_fit3d as d3  # noqa: E402


def rot(yaw, pitch, roll):
    """World-to-camera rotation (camera x right, y down, z forward)."""
    cy, sy = math.cos(yaw), math.sin(yaw)
    cp, sp = math.cos(pitch), math.sin(pitch)
    cr, sr = math.cos(roll), math.sin(roll)
    # forward from yaw (azimuth in XY) and pitch (down positive)
    fwd = np.array([cy * cp, sy * cp, -sp])
    right = np.array([sy, -cy, 0.0])
    up = np.cross(right, fwd)
    right, up = right * cr + up * sr, up * cr - right * sr
    return np.array([right, -up, fwd])


def project(params, Xw, w, h):
    c = params[:3]
    R = rot(*params[3:6])
    f = params[6]
    pc = (Xw - c) @ R.T
    return np.column_stack([f * pc[:, 0] / pc[:, 2] + w / 2, f * pc[:, 1] / pc[:, 2] + h / 2])


def fit(Xw, uv, w, h, p0):
    p = np.array(p0, float)
    lam = 1e-3

    def res(q):
        return (project(q, Xw, w, h) - uv).ravel()

    r = res(p)
    for _ in range(200):
        J = np.zeros((len(r), len(p)))
        for k in range(len(p)):
            e = 1e-4 * max(1.0, abs(p[k]))
            q = p.copy()
            q[k] += e
            J[:, k] = (res(q) - r) / e
        A = J.T @ J
        g = J.T @ r
        step = np.linalg.solve(A + lam * np.diag(np.diag(A) + 1e-9), -g)
        q = p + step
        rq = res(q)
        if rq @ rq < r @ r:
            p, r, lam = q, rq, lam * 0.3
        else:
            lam *= 10
        if np.linalg.norm(step) < 1e-7:
            break
    return p, r


def main():
    cfg = json.load(open(sys.argv[1]))
    out_path = sys.argv[2]
    Xw = np.array([c[0] if len(c[0]) == 3 else c[0] + [0.0] for c in cfg["points"]], float)
    uv = np.array([c[1] for c in cfg["points"]], float)
    w, h = cfg["w"], cfg["h"]
    P = d3.dlt(Xw, uv)
    K, R, t = d3.decompose(P)
    cam = -R.T @ t
    if ((Xw - cam) @ R[2]).mean() < 0:
        R, t = -R, -t
        cam = -R.T @ t
    fwd = R[2] / np.linalg.norm(R[2])
    yaw = math.atan2(fwd[1], fwd[0])
    pitch = math.asin(-fwd[2])
    p0 = [*cam, yaw, pitch, 0.0, 0.5 * (K[0, 0] + K[1, 1])]
    p, r = fit(Xw, uv, w, h, p0)
    rr = np.linalg.norm(r.reshape(-1, 2), axis=1)
    Rw = rot(*p[3:6])
    out = {"pos": p[:3].tolist(), "R": Rw.tolist(), "f": float(p[6]), "w": w, "h": h,
           "yawDeg": math.degrees(p[3]), "pitchDeg": math.degrees(p[4]), "rollDeg": math.degrees(p[5]),
           "residualPx": {"mean": float(rr.mean()), "max": float(rr.max())},
           "vfovDeg": math.degrees(2 * math.atan(h / 2 / p[6]))}
    if "summit" in cfg:
        su, sv = cfg["summit"]
        est = {}
        for gx, gy in cfg.get("summitXY", [[0, 0]]):
            zs = np.linspace(0, 80, 8001)
            pp = project(p, np.column_stack([np.full_like(zs, gx), np.full_like(zs, gy), zs]), w, h)
            k = int(np.argmin(np.abs(pp[:, 1] - sv)))
            est[f"{gx},{gy}"] = {"z": round(float(zs[k]), 1), "u": round(float(pp[k, 0]), 0)}
        out["summitHeight"] = est
    json.dump(out, open(out_path, "w"), indent=1)
    print(json.dumps({k: (round(v, 2) if isinstance(v, float) else v) for k, v in out.items() if k not in ("R",)},
                     indent=1))


if __name__ == "__main__":
    main()
