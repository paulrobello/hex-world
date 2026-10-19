"""Scratch clearance check for the bakery tile layout (mirrors hex-sim geom.rs + sim.rs depart()).

Runtime tile-local (x, z), z south. Yaw degrees from +x toward +z."""
import json
import math
import sys

APOTHEM = 13.856406
R = 16.0
CAR_L, CAR_W = 5.4, 2.0
OFF = 1.5


def heading(y):
    a = math.radians(y)
    return (math.cos(a), math.sin(a))


def add(a, b):
    return (a[0] + b[0], a[1] + b[1])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1])


def sc(a, s):
    return (a[0] * s, a[1] * s)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1]


def ln(a):
    return math.hypot(*a)


def right_of(d):
    return (-d[1], d[0])


def lane(center):
    line = []
    for p in center:
        if not line or ln(sub(p, line[-1])) > 1e-4:
            line.append(p)
    dirs = [sc(sub(b, a), 1 / ln(sub(b, a))) for a, b in zip(line, line[1:])]
    pts = []
    for i, p in enumerate(line):
        before = dirs[i - 1] if i > 0 else None
        after = dirs[i] if i < len(dirs) else None
        if before and after:
            s = add(right_of(before), right_of(after))
            n = sc(s, 1 / ln(s)) if ln(s) > 1e-3 else right_of(after)
        else:
            n = right_of(before or after)
        pts.append(add(p, sc(n, OFF)))
    return pts


def hermite(poses, reverse):
    sign = -1.0 if reverse else 1.0
    out = []
    for (pa, da), (pb, db) in zip(poses, poses[1:]):
        chord = ln(sub(pb, pa))
        ta, tb = sc(da, sign * chord), sc(db, sign * chord)
        n = max(8, math.ceil(chord / 0.1))
        first = 1 if out else 0
        for i in range(first, n + 1):
            s = i / n
            s2, s3 = s * s, s * s * s
            h00, h10, h01, h11 = 2 * s3 - 3 * s2 + 1, s3 - 2 * s2 + s, -2 * s3 + 3 * s2, s3 - s2
            out.append(add(add(sc(pa, h00), sc(ta, h10)), add(sc(pb, h01), sc(tb, h11))))
    return out


def corners(pos, d):
    r = (-d[1], d[0])
    hl, hw = CAR_L / 2, CAR_W / 2
    return [add(pos, add(sc(d, a), sc(r, b))) for a, b in ((hl, hw), (hl, -hw), (-hl, -hw), (-hl, hw))]


def sat_gap(p, q):
    """Separation distance between convex polygons (negative = overlap depth)."""
    best = -1e9
    for poly in (p, q):
        for i in range(len(poly)):
            e = sub(poly[(i + 1) % len(poly)], poly[i])
            n = (e[1] / ln(e), -e[0] / ln(e))
            a = [dot(v, n) for v in p]
            b = [dot(v, n) for v in q]
            gap = max(min(b) - max(a), min(a) - max(b))
            best = max(best, gap)
    return best


def box(x0, z0, x1, z1):
    return [(x0, z0), (x1, z0), (x1, z1), (x0, z1)]


def hex_inset(p):
    """Distance inside the hex border (positive inside)."""
    d = []
    for k in range(6):
        a = math.radians(60 * k)
        d.append(APOTHEM - (p[0] * math.cos(a) + p[1] * math.sin(a)))
    return min(d)


def run(cfg, verbose=True):
    hx, hz = cfg["shop"]
    xs, zb = cfg["xs"], cfg["zb"]
    # Shop footprint in Blender (Bx, By) -> world x = hx + By, z = hz + Bx.
    shop_local = {
        "shop body": (-2.56, -3.56, 2.56, 3.56),
        "chimney w": (-1.45, -4.1, 0.65, -3.5),
        "chimney e": (-1.45, 3.5, 0.65, 4.1),
        "bay": (2.5, -2.95, 3.11, 0.05),
        "basket": (3.27, -2.43, 3.73, -1.97),
        "bench": (2.64, 0.5, 2.96, 1.22),
        "doorstep": (2.5, 1.245, 2.94, 2.595),
    }
    obstacles = {k: box(hx + b0, hz + a0, hx + b1, hz + a1) for k, (a0, b0, a1, b1) in shop_local.items()}
    for name, poly in cfg.get("extra", {}).items():
        obstacles[name] = [tuple(p) for p in poly]
    spots = [(tuple(s["at"]), heading(s["yaw"])) for s in cfg["spots"]]
    spot_polys = [corners(p, d) for p, d in spots]
    j = (xs, 0.0)
    node = (xs, zb)
    intos = {}
    for name, start in (("from0", (APOTHEM, 0.0)), ("from3", (-APOTHEM, 0.0))):
        pts = lane([sub(start, (0.0 if name == "from0" else 0.0, 0.0)), j, node])
        d = sub(pts[-1], pts[-2])
        intos[name] = (pts, (pts[-1], sc(d, 1 / ln(d))))
    outs = {}
    for name, end in (("to0", (APOTHEM, 0.0)), ("to3", (-APOTHEM, 0.0))):
        pts = lane([node, j, end])
        d = sub(pts[1], pts[0])
        outs[name] = (pts, (pts[0], sc(d, 1 / ln(d))))
    report = {}
    worst = 1e9
    for k, s in enumerate(cfg["spots"]):
        pose = spots[k]
        enter = [(tuple(p["at"]), heading(p["yaw"])) for p in s.get("enter", [])]
        back = [(tuple(p["at"]), heading(p["yaw"])) for p in s.get("back", [])]
        leave = [(tuple(p["at"]), heading(p["yaw"])) for p in s.get("leave", [])]
        legs = {}
        for iname, (lpts, into) in intos.items():
            pts = hermite([into, *enter, pose], False)
            legs[f"enter/{iname}"] = (pts, False, lpts)
        bpts = hermite([pose, *back], True) if back else [pose[0]]
        legs["back"] = (bpts, True, None)
        cusp = back[-1] if back else pose
        for oname, (lpts, out) in outs.items():
            pts = hermite([cusp, *leave, out], False)
            legs[f"leave/{oname}"] = (pts, False, lpts)
        for lname, (pts, rev, lpts) in legs.items():
            path = list(pts)
            if lpts is not None:
                # lane part inside the tile near the spur (skip far points)
                if lname.startswith("enter"):
                    path = [p for p in lpts if abs(p[0]) < R] + path
                else:
                    path = path + [p for p in lpts if abs(p[0]) < R]
            mins = {}
            for a, b in zip(path, path[1:]):
                d = sub(b, a)
                if ln(d) < 1e-6:
                    continue
                d = sc(d, 1 / ln(d))
                if rev and lpts is None:
                    d = sc(d, -1)
                for t in (0.0, 0.5):
                    p = add(a, sc(sub(b, a), t))
                    c = corners(p, d)
                    for oname2, poly in obstacles.items():
                        g = sat_gap(c, poly)
                        if g < mins.get(oname2, (1e9,))[0]:
                            mins[oname2] = (g, p, d)
                    for i, sp in enumerate(spot_polys):
                        if i == k:
                            continue
                        g = sat_gap(c, sp)
                        key = f"spot {i}"
                        if g < mins.get(key, (1e9,))[0]:
                            mins[key] = (g, p, d)
                    hi = min(hex_inset(q) for q in c) if abs(p[0]) < 9.0 else 9.0
                    if hi < mins.get("hex border", (1e9,))[0]:
                        mins["hex border"] = (hi, p, d)
            lo = min(mins.items(), key=lambda kv: kv[1][0])
            report[f"spot{k} {lname}"] = {n: round(v[0], 3) for n, v in sorted(mins.items(), key=lambda kv: kv[1][0])[:3]}
            worst = min(worst, lo[1][0])
            if verbose:
                g, p, d = lo[1]
                print(f"spot{k} {lname:12s} min {lo[0]:12s} {g:+.3f} at ({p[0]:.2f},{p[1]:.2f}) dir ({d[0]:.2f},{d[1]:.2f})"
                      f"  | border {mins['hex border'][0]:+.2f}")
    # Spot-vs-spot and spot-vs-obstacle static checks.
    for k, sp in enumerate(spot_polys):
        for oname2, poly in obstacles.items():
            g = sat_gap(sp, poly)
            if g < 0.3 and verbose:
                print(f"static spot{k} vs {oname2}: {g:+.3f}")
        if verbose:
            print(f"static spot{k} border {min(hex_inset(q) for q in sp):+.3f}")
    if verbose:
        for n, (lp, into) in intos.items():
            print("into", n, [round(v, 3) for v in into[0]], round(math.degrees(math.atan2(into[1][1], into[1][0])), 2))
        for n, (lp, out) in outs.items():
            print("out", n, [round(v, 3) for v in out[0]], round(math.degrees(math.atan2(out[1][1], out[1][0])), 2))
    return worst, report


if __name__ == "__main__":
    cfg = json.load(open(sys.argv[1]))
    w, rep = run(cfg)
    print("WORST", round(w, 3))
