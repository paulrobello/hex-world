"""Per-tick turn along the bakery legs (0.4 m step = 8 m/s / 20 Hz), mirroring sim.rs depart()."""
import json
import math
import sys

sys.path.insert(0, __import__("os").path.dirname(__import__("os").path.abspath(__file__)))
from check import APOTHEM, heading, hermite, lane, ln, sc, sub  # noqa: E402

cfg = json.load(open(sys.argv[1]))
xs, zb = cfg["xs"], cfg["zb"]
j, node = (xs, 0.0), (xs, zb)


def resample(pts, step):
    cum = [0.0]
    for a, b in zip(pts, pts[1:]):
        cum.append(cum[-1] + ln(sub(b, a)))
    out, s, i = [], 0.0, 1
    while s <= cum[-1] and i < len(pts):
        while i < len(pts) - 1 and cum[i] < s:
            i += 1
        a, b = pts[i - 1], pts[i]
        d = sub(b, a)
        out.append((s, sc(d, 1 / max(ln(d), 1e-9))))
        s += step
    return out


def worst(pts, rev):
    samples = resample(pts, 0.4)
    w = (0.0, 0.0)
    for (s0, d0), (s1, d1) in zip(samples, samples[1:]):
        a = math.degrees(math.acos(max(-1, min(1, d0[0] * d1[0] + d0[1] * d1[1]))))
        if a > w[0]:
            w = (a, s0)
    return w


for k, s in enumerate(cfg["spots"]):
    pose = (tuple(s["at"]), heading(s["yaw"]))
    back = [(tuple(p["at"]), heading(p["yaw"])) for p in s.get("back", [])]
    enter = [(tuple(p["at"]), heading(p["yaw"])) for p in s.get("enter", [])]
    for name, start in (("from0", (APOTHEM, 0.0)), ("from3", (-APOTHEM, 0.0))):
        lp = lane([start, j, node])
        d = sub(lp[-1], lp[-2])
        into = (lp[-1], sc(d, 1 / ln(d)))
        print(f"spot{k} enter/{name}", [round(v, 1) for v in worst(hermite([into, *enter, pose], False), False)])
    bp = hermite([pose, *back], True)
    print(f"spot{k} back", [round(v, 1) for v in worst(bp, True)])
    for name, end in (("to0", (APOTHEM, 0.0)), ("to3", (-APOTHEM, 0.0))):
        lp = lane([node, j, end])
        d = sub(lp[1], lp[0])
        out = (lp[0], sc(d, 1 / ln(d)))
        print(f"spot{k} leave/{name}", [round(v, 1) for v in worst(hermite([back[-1], out], False), False)])
