"""Shared builder for the farm animal props animal_cow and animal_sheep (PRD P3; owner-approved
references assets-source/refs/farm_animals, owner decisions A17).

Frame: an animal is authored in its own frame (x lateral, f forward, z up; metres, real scale) and placed
in Blender at (x, -f, z), so runtime (x, y, z) = (x, z, f): it faces runtime +z with +y up and stands on
y = 0, pivot on the ground under the body centre (the `_fauna.py` convention of bird_starling). Facing
+z with +y up, the animal's left side is runtime +x.

Shape: every part is a loft of superellipse cross-sections along a centre path (body, neck and head,
legs, ears, tail), smooth shaded, never decimated: each LOD rebuilds the same analytic surface from a
subset of the stations at fewer ring sides. UV0 is analytic: a part's island is a rectangle, u the arc
length along its path and v the fraction around its ring times a reference perimeter, with the seam on
the part's hidden side (belly, throat, back of the legs, back of the ears). Every LOD therefore maps a
surface point to the same texel. Left and right legs and ears are mirror copies sharing one island. The
islands are skyline-packed at one scale (the largest that fits) with a gutter.

Maps are painted in numpy from each texel's creature-frame position (`_fauna.rasterize`), so colour
patches run on across part junctions. Only AO is baked, by Cycles on LOD0 onto itself, into ORM.R
(masks.A = 1, as on the fauna). Albedo 1024 px; normal and ORM box-filtered to 512 px; masks 128 px.

Motion data (TEXCOORD_1, Blender UV map `motion`, FLOAT VEC2 on every primitive of every LOD; the
contract text lives in each generator's docstring): U is the leg weight, V the leg code where U > 0 and
the head weight where U = 0. `check_motion` reads it back from the exported GLB."""
import json
import math
import os
import pathlib
import struct

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import _fauna as fa
from lib import bake, budgets, collider, export, geom, scene, uv

ATLAS, SMALL, MASK_SIZE = 1024, 512, 128
GUTTER, BORDER = 8, 4
PART = {"body": 0, "head": 1, "leg_fl": 2, "leg_fr": 3, "leg_bl": 4, "leg_br": 5, "ear_l": 6, "ear_r": 7,
        "tail": 8, "neck": 9}
LEG_CODE = {"leg_fl": 0.125, "leg_fr": 0.375, "leg_bl": 0.625, "leg_br": 0.875}
# Paired parts: the spec describes the left (+x) copy; the right copy is its mirror with the same UVs.
PAIRS = {"front_leg": ("leg_fl", "leg_fr"), "back_leg": ("leg_bl", "leg_br"), "ear": ("ear_l", "ear_r")}


def sexp(v, e):
    return np.sign(v) * np.abs(v) ** (2.0 / e)


class Part:
    """A loft. stations: [(c (x, f, z), w, ht, hb, e)], w = 0 is a pole. Cross-section point at angle
    phi from the top: c + X * w * S(sin phi) + N * h * S(cos phi), h = ht on the N side and hb on the
    other, S the superellipse power 2 / e. frame "fixed" uses the given X and N; "path" keeps X and takes
    N = X x tangent of the centre path. pref: the island's around-size in metres (default: the largest
    ring perimeter)."""

    def __init__(self, stations, frame="fixed", X=(1.0, 0.0, 0.0), N=(0.0, 0.0, 1.0), pref=None):
        self.c = np.array([s[0] for s in stations], np.float64)
        self.w = np.array([s[1] for s in stations], np.float64)
        self.ht = np.array([s[2] for s in stations], np.float64)
        self.hb = np.array([s[3] for s in stations], np.float64)
        self.e = np.array([s[4] for s in stations], np.float64)
        X = np.array(X, np.float64)
        m = len(stations)
        self.X = np.tile(X, (m, 1))
        if frame == "fixed":
            self.N = np.tile(np.array(N, np.float64), (m, 1))
        else:
            self.N = np.zeros((m, 3))
            for i in range(m):
                t = self.c[min(i + 1, m - 1)] - self.c[max(i - 1, 0)]
                n = np.cross(X, t / np.linalg.norm(t))
                self.N[i] = n / np.linalg.norm(n)
        seg = np.linalg.norm(np.diff(self.c, axis=0), axis=1)
        self.s = np.concatenate([[0.0], np.cumsum(seg)])
        self.Lc = float(self.s[-1])
        # Surface arc length between stations (mean over the ring), the island's u: a pole fan gets its
        # real length instead of the centre path's.
        ring = lambda i: np.array([self.point(i, 2 * math.pi * k / 32) for k in range(32)])  # noqa: E731
        sseg = [float(np.linalg.norm(ring(i + 1) - ring(i), axis=1).mean()) for i in range(m - 1)]
        self.su = np.concatenate([[0.0], np.cumsum(sseg)])
        self.L = float(self.su[-1])
        per = [self.perimeter(i) for i in range(m)]
        self.pref = float(pref if pref is not None else max(per))

    def point(self, i, phi):
        cs, sn = math.cos(phi), math.sin(phi)
        h = self.ht[i] if cs >= 0 else self.hb[i]
        return (self.c[i] + self.X[i] * self.w[i] * float(sexp(sn, self.e[i]))
                + self.N[i] * h * float(sexp(cs, self.e[i])))

    def perimeter(self, i):
        if self.w[i] == 0:
            return 0.0
        pts = np.array([self.point(i, 2 * math.pi * k / 128) for k in range(129)])
        return float(np.linalg.norm(np.diff(pts, axis=0), axis=1).sum())

    def local(self, P):
        """For points P (n, 3): (s along the path, superellipse value at the nearest path point (< 1
        inside), lateral and normal offsets as fractions of the local half sizes). Projects onto the
        nearest centre-path segment and interpolates the station shape."""
        best_d = np.full(len(P), np.inf)
        out_s = np.zeros(len(P))
        out_val = np.full(len(P), np.inf)
        out_x = np.zeros(len(P))
        out_n = np.zeros(len(P))
        for i in range(len(self.c) - 1):
            a, d = self.c[i], self.c[i + 1] - self.c[i]
            if d @ d < 1e-10:
                continue  # a flat cap: its pole shares the ring's centre
            t = np.clip(((P - a) @ d) / (d @ d), 0.0, 1.0)
            q = a + t[:, None] * d
            dist = np.linalg.norm(P - q, axis=1)
            lerp = lambda arr: arr[i] + (arr[i + 1] - arr[i]) * t  # noqa: E731
            w, ht, hb, e = lerp(self.w), lerp(self.ht), lerp(self.hb), lerp(self.e)
            X = self.X[i] + (self.X[i + 1] - self.X[i]) * t[:, None]
            N = self.N[i] + (self.N[i + 1] - self.N[i]) * t[:, None]
            xl = ((P - q) * X).sum(1)
            nl = ((P - q) * N).sum(1)
            h = np.where(nl > 0, ht, hb)
            val = (np.abs(xl) / np.maximum(w, 1e-4)) ** e + (np.abs(nl) / np.maximum(h, 1e-4)) ** e
            take = dist < best_d
            best_d[take] = dist[take]
            out_s[take] = (self.s[i] + t * (self.s[i + 1] - self.s[i]))[take]
            out_val[take] = val[take]
            out_x[take] = (xl / np.maximum(w, 1e-4))[take]
            out_n[take] = (nl / np.maximum(h, 1e-4))[take]
        return out_s, out_val, out_x, out_n


class Builder:
    """bmesh accumulator: UVMap and motion per loop (explicit values), a part id per face."""

    def __init__(self):
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")
        self.mo = self.bm.loops.layers.uv.new("motion")
        self.part = self.bm.faces.layers.int.new("part")

    def vert(self, p):
        return self.bm.verts.new(fa.to_blender(*p))

    def face(self, verts, pts, uvs, mos, part, inside):
        co = [Vector(fa.to_blender(*p)) for p in pts]
        n = Vector()
        for i in range(len(co)):  # Newell normal
            a, b = co[i], co[(i + 1) % len(co)]
            n += Vector(((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y)))
        c = sum(co, Vector()) / len(co)
        order = list(range(len(verts)))
        if n.dot(c - Vector(fa.to_blender(*inside))) < 0:
            order.reverse()
        f = self.bm.faces.new([verts[k] for k in order])
        f[self.part] = part
        f.smooth = True
        for loop, k in zip(f.loops, order):
            loop[self.uv].uv = uvs[k]
            mu, mv = mos[k]
            loop[self.mo].uv = (mu, 1.0 - mv)  # the glTF exporter writes V as 1 - v
        return f

    def finish(self, name, mat):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces[:], quad_method="BEAUTY", ngon_method="EAR_CLIP")
        me = bpy.data.meshes.new(name)
        self.bm.to_mesh(me)
        self.bm.free()
        me.materials.append(mat)
        obj = scene.link(bpy.data.objects.new(name, me))
        me.uv_layers["UVMap"].active = True
        me.uv_layers["UVMap"].active_render = True
        return obj


def pack(rects, size=ATLAS, gutter=GUTTER, border=BORDER):
    """Skyline bottom-left packing of island rectangles {name: (u size m, v size m)} at the largest
    px-per-metre scale that fits. Returns (scale, {name: (x0 px, y0 px, rotated)})."""
    items = sorted(rects.items(), key=lambda r: (-max(r[1]), r[0]))
    lim = size - border + gutter // 2

    def place(sig):
        sky = [[border, border, size - 2 * border]]
        out = {}
        for name, (wm, hm) in items:
            best = None
            for rot in (False, True):
                w = math.ceil((hm if rot else wm) * sig) + gutter
                h = math.ceil((wm if rot else hm) * sig) + gutter
                for i in range(len(sky)):
                    x = sky[i][0]
                    if x + w > lim:
                        continue
                    y, rem, j = 0, w, i
                    while rem > 0 and j < len(sky):
                        y = max(y, sky[j][1])
                        rem -= sky[j][2]
                        j += 1
                    if rem > 0 or y + h > lim:
                        continue
                    key = (y + h, x, rot)
                    if best is None or key < best[0]:
                        best = (key, x, y, w, h, rot)
            if best is None:
                return None
            _, x, y, w, h, rot = best
            out[name] = (x + gutter // 2, y + gutter // 2, rot)
            new = []
            for sx, sy, sw in sky:
                ex = sx + sw
                if ex <= x or sx >= x + w:
                    new.append([sx, sy, sw])
                    continue
                if sx < x:
                    new.append([sx, sy, x - sx])
                if ex > x + w:
                    new.append([x + w, sy, ex - (x + w)])
            new.append([x, y + h, w])
            new.sort()
            merged = [new[0]]
            for seg in new[1:]:
                if seg[1] == merged[-1][1] and merged[-1][0] + merged[-1][2] == seg[0]:
                    merged[-1][2] += seg[2]
                else:
                    merged.append(seg)
            sky = merged
        return out

    lo, hi, best = 20.0, 4000.0, None
    for _ in range(40):
        mid = (lo + hi) / 2
        got = place(mid)
        if got is None:
            hi = mid
        else:
            lo, best = mid, got
    sig = math.floor(lo * 100) / 100
    got = place(sig)
    assert got is not None
    boxes = []
    for name, (x0, y0, rot) in got.items():
        wm, hm = rects[name]
        w, h = ((hm, wm) if rot else (wm, hm))
        boxes.append((name, x0, y0, x0 + w * sig, y0 + h * sig))
    for i, a in enumerate(boxes):
        assert a[3] <= size - border and a[4] <= size - border, f"island {a[0]} leaves the atlas"
        for bx in boxes[i + 1:]:
            assert a[3] + gutter / 2 <= bx[1] or bx[3] + gutter / 2 <= a[1] or a[4] + gutter / 2 <= bx[2] \
                or bx[4] + gutter / 2 <= a[2], f"islands {a[0]} and {bx[0]} overlap"
    return sig, got


class Atlas:
    def __init__(self, islands, sig, place):
        self.islands, self.sig, self.place = islands, sig, place

    def uv(self, name, um, vm):
        x0, y0, rot = self.place[name]
        a, b = (vm, um) if rot else (um, vm)
        return ((x0 + a * self.sig) / ATLAS, (y0 + b * self.sig) / ATLAS)


def make_atlas(spec):
    """One island per spec part (paired parts share it): (path length, reference perimeter)."""
    rects = {name: (p.L, p.pref) for name, p in spec.items()}
    sig, place = pack(rects)
    return Atlas(rects, sig, place)


def emit(b, part, name, atlas, idxs, n, pid, motion, mirror=False):
    """Faces of `part` through stations idxs with n ring sides. motion(i, p) -> (U, V)."""
    sx = -1.0 if mirror else 1.0
    mir = lambda p: (sx * p[0], p[1], p[2])  # noqa: E731
    rings = []
    for i in idxs:
        if part.w[i] == 0:
            p = mir(part.c[i])
            rings.append((i, [(b.vert(p), p)]))
        else:
            r = []
            for k in range(n):
                p = mir(part.point(i, math.pi + 2 * math.pi * k / n))
                r.append((b.vert(p), p))
            rings.append((i, r))
    pref = part.pref
    for j in range(len(rings) - 1):
        (ia, ra), (ic, rc) = rings[j], rings[j + 1]
        if len(ra) == 1 and len(rc) == 1:
            continue
        if len(ra) == 1 or len(rc) == 1:
            (ip, rp), (ir, rr), other = ((ia, ra), (ic, rc), j + 2) if len(ra) == 1 else ((ic, rc), (ia, ra), j - 1)
            cr, cp = part.c[ir], part.c[ip]
            if 0 <= other < len(rings):
                inside = mir(cr + (part.c[rings[other][0]] - cr) * 0.5)
            else:
                inside = mir((cr + cp) / 2)
            for k in range(n):
                vs = [rp[0], rr[k], rr[(k + 1) % n]]
                uvs = [atlas.uv(name, part.su[ip], (k + 0.5) / n * pref),
                       atlas.uv(name, part.su[ir], k / n * pref), atlas.uv(name, part.su[ir], (k + 1) / n * pref)]
                mos = [motion(ip, rp[0][1]), motion(ir, rr[k][1]), motion(ir, rr[(k + 1) % n][1])]
                b.face([v for v, _ in vs], [p for _, p in vs], uvs, mos, pid, inside)
            continue
        inside = mir((part.c[ia] + part.c[ic]) / 2)
        for k in range(n):
            k1 = (k + 1) % n
            vs = [ra[k], ra[k1], rc[k1], rc[k]]
            uvs = [atlas.uv(name, part.su[ia], k / n * pref), atlas.uv(name, part.su[ia], (k + 1) / n * pref),
                   atlas.uv(name, part.su[ic], (k + 1) / n * pref), atlas.uv(name, part.su[ic], k / n * pref)]
            mos = [motion(ia, ra[k][1]), motion(ia, ra[k1][1]), motion(ic, rc[k1][1]), motion(ic, rc[k][1])]
            b.face([v for v, _ in vs], [p for _, p in vs], uvs, mos, pid, inside)


class Motion:
    """TEXCOORD_1 weights. Legs: U = (hip_z - z) / hip_z (0 at the hip pivot height, 1 at the hoof),
    V = the leg code. Neck and head: U = 0 and V a position field along the axis a from the neck pivot
    (the rotation centre, inside the shoulder) to the muzzle tip: V = clamp((t - t_base) / (t_tip -
    t_base), 0, 1) with t the projection on a, so V = 0 on and behind the plane through `base` (where the
    neck leaves the body) and 1 at the muzzle tip. The neck and head agree wherever their surfaces meet,
    and the neck root stays put while the head pitches fully. Ears take the field's value at their
    attachment point (rigid with the head). Body and tail: (0, 0)."""

    def __init__(self, hip_z, pivot, base, tip, ear_root):
        self.hip_z = hip_z
        self.pivot = np.array(pivot, np.float64)  # (f, z)
        self.base = np.array(base, np.float64)
        self.tip = np.array(tip, np.float64)
        d = self.tip - self.pivot
        self.a = d / np.linalg.norm(d)
        self.t0 = float((self.base - self.pivot) @ self.a)
        self.t1 = float((self.tip - self.pivot) @ self.a)
        self.ear_v = self.head_v((0.0, *ear_root[1:]))

    def head_v(self, p):
        t = float((np.array(p[1:], np.float64) - self.pivot) @ self.a)
        return float(np.clip((t - self.t0) / (self.t1 - self.t0), 0.0, 1.0))

    def fn(self, name, leg=None):
        if name in ("front_leg", "back_leg"):
            code = LEG_CODE[leg]

            def leg_fn(i, p):
                assert p[2] < self.hip_z - 1e-6, f"{leg} vertex z {p[2]} not below the hip pivot {self.hip_z}"
                return (float(np.clip((self.hip_z - p[2]) / self.hip_z, 0.0, 1.0)), code)
            return leg_fn
        if name in ("neck", "head"):
            return lambda i, p: (0.0, self.head_v(p))
        if name == "ear":
            return lambda i, p: (0.0, self.ear_v)
        return lambda i, p: (0.0, 0.0)


def build_lod(piece, level, spec, lods, atlas, motion, mat):
    b = Builder()
    for name, part in spec.items():
        if name not in lods[level]:
            continue
        idxs, n = lods[level][name]
        if name in PAIRS:
            for side, pname in enumerate(PAIRS[name]):
                emit(b, part, name, atlas, idxs, n, PART[pname], motion.fn(name, pname), mirror=side == 1)
        else:
            emit(b, part, name, atlas, idxs, n, PART[name], motion.fn(name))
    return b.finish(f"{piece}_lod{level}", mat)


HOSTS = [("front_leg", 0, "body"), ("back_leg", 0, "body"), ("neck", 0, "body"), ("neck", -1, "head"),
         ("ear", 0, "head"), ("tail", 0, "body")]


def check_buried(spec, lods):
    """Every open ring end (a station that is not a pole) must lie inside its host part at every LOD's
    ring count, so no hole shows. Raises AssertionError listing the misses."""
    errs = []
    for name, end, host in HOSTS:
        if name not in spec:
            continue
        p = spec[name]
        i = end % len(p.c)
        if p.w[i] == 0:
            continue
        for lv, parts in lods.items():
            if name not in parts:
                continue
            n = parts[name][1]
            pts = np.array([p.point(i, math.pi + 2 * math.pi * k / n) for k in range(n)])
            _, val, _, _ = spec[host].local(pts)
            if (val >= 0.98).any():
                errs.append(f"{name} ring {i} pokes out of {host} at LOD{lv} (max {val.max():.3f})")
    assert not errs, "; ".join(errs)


def hidden(spec, P, part):
    """Samples buried inside another solid part (leg tops, the neck start, ear roots, the tail root),
    for AO statistics over visible texels only."""
    owner = {"body": [PART["body"]], "head": [PART["head"]], "neck": [PART["neck"]],
             "front_leg": [PART["leg_fl"], PART["leg_fr"]],
             "back_leg": [PART["leg_bl"], PART["leg_br"]], "ear": [PART["ear_l"], PART["ear_r"]],
             "tail": [PART["tail"]]}
    hid = np.zeros(len(P), bool)
    Pm = P.copy()
    Pm[:, 0] = np.abs(Pm[:, 0])
    for name, p in spec.items():
        if name in ("ear",):
            continue
        test = ~np.isin(part, owner[name])
        Q = Pm if name in PAIRS else P
        for lo in range(0, len(P), 200000):
            sl = slice(lo, lo + 200000)
            t = test[sl]
            if not t.any():
                continue
            _, val, _, _ = p.local(Q[sl][t])
            sub = hid[sl]
            sub[np.nonzero(t)[0][val < 0.97]] = True
            hid[sl] = sub
    return hid


def _down(px, size, out, renormal=False):
    f = size // out
    a = px.reshape(out, f, out, f, 4).mean(axis=(1, 3))
    if renormal:
        nv = a[..., :3] * 2.0 - 1.0
        nv /= np.maximum(np.linalg.norm(nv, axis=-1, keepdims=True), 1e-6)
        a[..., :3] = nv * 0.5 + 0.5
    a[..., 3] = 1.0
    return a.reshape(-1, 4).astype(np.float32)


def paint_maps(lod0, spec, paint, out_dir, piece, skip_bake, ao_dist, sig):
    """Writes <piece>.{albedo (1024), normal (512), orm (512), masks (128)}.png. paint(P, part) ->
    dict(rgb sRGB, rough, metal, height m, snow, wet) per sample. Every texel outside the islands takes
    its nearest island sample (`_fauna.fill_index`), so deep mips keep island colours."""
    idx, P, part = fa.rasterize(lod0, ATLAS)
    filled = fa.fill_index(idx, ATLAS, ATLAS)
    cov = idx >= 0
    texel_of = np.zeros(len(P), np.int64)
    texel_of[idx[cov]] = np.nonzero(cov)[0]
    ao = np.ones(ATLAS * ATLAS, np.float32)
    if not skip_bake:
        fa.AO_DIST = ao_dist
        ao = fa.bake_ao(lod0, ATLAS, out_dir, piece)
    out = paint(P, part)
    n_px = ATLAS * ATLAS
    tgt = np.arange(n_px)
    src = filled
    alb = np.ones((n_px, 4), np.float32)
    alb[:, :3] = out["rgb"][src]
    orm = np.ones((n_px, 4), np.float32)
    orm[:, 0] = np.where(cov, ao, ao[texel_of[src]])
    orm[:, 1] = out["rough"][src]
    orm[:, 2] = out["metal"][src]
    h = out["height"][src].reshape(ATLAS, ATLAS)
    nrm = np.ones((n_px, 4), np.float32)
    nrm[:, :3] = fa.height_to_normal(h, ATLAS, sig).reshape(-1, 3)
    o = pathlib.Path(out_dir)
    bake.write_png(ATLAS, alb, str(o / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(SMALL, _down(nrm, ATLAS, SMALL, renormal=True), str(o / f"{piece}.normal.png"))
    bake.write_png(SMALL, _down(orm, ATLAS, SMALL), str(o / f"{piece}.orm.png"))
    f = ATLAS // MASK_SIZE
    down = lambda a: a.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3)).ravel()  # noqa: E731
    bake.make_masks(MASK_SIZE, down(out["snow"][src]), down(out["wet"][src]), 0.0, None,
                    str(o / f"{piece}.masks.png"))
    hid = hidden(spec, P, part)
    vis = cov.copy()
    vis[cov] = ~hid[idx[cov]]
    lum = out["rgb"][idx[cov]].mean(1)
    stats = {"albedoLuma": {"mean": float(lum.mean()), "p05": float(np.percentile(lum, 5))},
             "coverage": float(cov.mean()), "hiddenTexelFraction": float(1 - vis.sum() / cov.sum())}
    if not skip_bake:
        av, ac = ao[vis], ao[cov]
        stats["aoVisible"] = {"texels": int(vis.sum()), "mean": float(av.mean()),
                              "p05": float(np.percentile(av, 5)), "min": float(av.min())}
        stats["aoAllIslandTexels"] = {"mean": float(ac.mean()), "p05": float(np.percentile(ac, 5))}
    return stats


def _glb(path):
    data = pathlib.Path(path).read_bytes()
    off, chunks = 12, {}
    while off < len(data):
        clen, ctype = struct.unpack_from("<II", data, off)
        chunks[ctype] = data[off + 8:off + 8 + clen]
        off += 8 + clen
    js = json.loads(chunks[0x4E4F534A])
    binc = chunks[0x004E4942]

    def acc(i):
        a = js["accessors"][i]
        bv = js["bufferViews"][a["bufferView"]]
        nc = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        assert a["componentType"] == 5126, "attribute is not FLOAT"
        assert bv.get("byteStride", 4 * nc) == 4 * nc
        start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        return np.frombuffer(binc, "<f4", count=a["count"] * nc, offset=start).reshape(-1, nc)
    return js, acc


def check_motion(glb_path, piece, motion):
    """Reads TEXCOORD_1 and POSITION back from the exported GLB on every primitive of every LOD and
    asserts the motion contract. Returns per-LOD counts."""
    hip_z = motion.hip_z
    js, acc = _glb(glb_path)
    codes = np.array(sorted(LEG_CODE.values()))
    report = {}
    for lv in range(3):
        mesh = next(m for m in js["meshes"] if m["name"] == f"{piece}_lod{lv}")
        per_code = dict.fromkeys(LEG_CODE, 0)
        vmax, n_head = 0.0, 0
        for prim in mesh["primitives"]:
            at = prim["attributes"]
            assert "TEXCOORD_1" in at, f"{piece}_lod{lv}: primitive without TEXCOORD_1"
            mo, pos = acc(at["TEXCOORD_1"]), acc(at["POSITION"])
            assert ((mo >= 0) & (mo <= 1)).all(), f"{piece}_lod{lv}: motion outside [0, 1]"
            leg = mo[:, 0] > 0
            d = np.abs(mo[leg, 1][:, None] - codes[None, :])
            assert (d.min(1) < 1e-5).all(), f"{piece}_lod{lv}: leg vertex V is not a leg code"
            assert (pos[leg, 1] < hip_z).all(), f"{piece}_lod{lv}: leg vertex above the hip pivot"
            for name, code in LEG_CODE.items():
                sel = leg & (np.abs(mo[:, 1] - code) < 1e-5)
                left, front = name.endswith("l"), name.startswith("leg_f")
                assert ((pos[sel, 0] > 0) == left).all(), f"{piece}_lod{lv}: {name} on the wrong side"
                assert ((pos[sel, 2] > 0) == front).all(), f"{piece}_lod{lv}: {name} not at the {'front' if front else 'back'}"
                per_code[name] += int(sel.sum())
            head = ~leg & (mo[:, 1] > 0)
            want = np.array([motion.head_v((0.0, q[2], q[1])) for q in pos[head]])
            got = mo[head, 1]
            ok = (np.abs(got - want) < 1e-3) | (np.abs(got - motion.ear_v) < 1e-5)
            assert ok.all(), f"{piece}_lod{lv}: head weight off its position field at {pos[head][~ok][:3]}"
            n_head += int(head.sum())
            if head.any():
                vmax = max(vmax, float(mo[head, 1].max()))
        assert all(v > 0 for v in per_code.values()), f"{piece}_lod{lv}: a leg has no vertices {per_code}"
        assert abs(vmax - 1.0) < 1e-4, f"{piece}_lod{lv}: head weight max {vmax}, want 1 at the muzzle"
        report[f"lod{lv}"] = {"legVerts": per_code, "headVerts": n_head}
    return report


def make_extras(spec, motion, real, gait):
    """Root-node extras (`animal`, a JSON string): the motion contract numbers in the runtime frame."""
    hip_z = motion.hip_z
    pv, tip = (0.0, *motion.pivot), (0.0, *motion.tip)
    df, dz = tip[1] - pv[1], tip[2] - pv[2]
    graze = None
    for deg in np.arange(0.0, 150.0, 0.5):
        t = math.radians(deg)
        if pv[2] + dz * math.cos(t) - df * math.sin(t) <= 0.08:
            graze = float(deg)
            break
    hips = {}
    for name, (lft, rgt) in (("front_leg", PAIRS["front_leg"]), ("back_leg", PAIRS["back_leg"])):
        c = spec[name].c[0]
        hips[lft] = [round(float(c[0]), 4), hip_z, round(float(c[1]), 4)]
        hips[rgt] = [round(float(-c[0]), 4), hip_z, round(float(c[1]), 4)]
    return {
        "frame": "runtime +z forward, +y up, +x the animal's left; pivot on the ground under the body centre",
        "real": real,
        "tc1": {"u": "leg weight: 0 off the legs; on a leg (hipY - y) / hipY, 0 at the hip pivot height, 1 at the hoof",
                "v": "where U > 0 the leg code (0.125 front-left, 0.375 front-right, 0.625 back-left, 0.875 back-right); "
                     "where U = 0 the head weight, 0 on the body, tail and up to the neck pivot, rising along the "
                     "neck to 1 at the muzzle tip (ears take their attachment's value)"},
        "legCodes": LEG_CODE, "hipY": hip_z, "hips": hips, "gaitPhase": gait,
        "neckPivot": [0.0, round(float(pv[2]), 4), round(float(pv[1]), 4)],
        "neckBase": [0.0, round(float(motion.base[1]), 4), round(float(motion.base[0]), 4)],
        "muzzleTip": [0.0, round(float(tip[2]), 4), round(float(tip[1]), 4)],
        "vPoll": round(motion.head_v((0.0, *spec["head"].c[0][1:])), 4),
        "grazeDegToGround": graze,
        "walk": "U > 0: rotate about the axis parallel to x through hips[leg] by swing(t + gaitPhase[leg]); "
                "every leg vertex sits below hipY, so U > 0 selects the leg and U can soften the hip",
        "graze": "U = 0 and V > 0: rotate about the axis parallel to x through neckPivot, nose down, by theta x V "
                 "(V = 0 on the plane through neckBase normal to neckPivot -> muzzleTip, 1 at the muzzle tip)",
    }


def export_piece(piece, lods, out_dir, generator, extras, texel, sig, stats, skip_bake, ao_dist, motion):
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}, targets {budgets.lod_targets(tris[0], 'prop', piece)}, "
          f"texel {texel}, scale {sig} px/m")
    budgets.check(tuple(tris), "prop", piece)
    assert texel["median"] >= 256, texel
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    root = scene.empty(piece)
    root["animal"] = json.dumps(extras, sort_keys=True)
    for o in lods:
        o.parent = root
    col.parent = root
    out = pathlib.Path(out_dir)
    glb = out / f"{piece}.glb"
    export.export_glb(root, str(glb))
    motion = check_motion(glb, piece, motion)
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=fa.SAMPLES,
                           marginPx=fa.MARGIN, aoDistanceM=ao_dist,
                           colourSource="numpy paint from creature-frame position (no colour bake)",
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": SMALL, "colorspace": "linear"},
                                     "orm": {"size": SMALL, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, packScalePxPerM=sig, bakeStats=stats, lodTris=tris,
                           animal=extras, motionCheck=motion, skipped=skip_bake)
    print(f"{piece}: motion {motion}; stats {stats}")
    return tris


# Paint helpers.

def vnoise(P, scale, seed):
    """3D value noise in [0, 1] at P / scale (smoothstep interpolation, integer hash)."""
    q = P / scale
    i = np.floor(q).astype(np.int64)
    f = q - i
    u = f * f * (3 - 2 * f)

    def h(dx, dy, dz):
        x = ((i[:, 0] + dx).astype(np.uint64) * np.uint64(73856093)) ^ \
            ((i[:, 1] + dy).astype(np.uint64) * np.uint64(19349663)) ^ \
            ((i[:, 2] + dz).astype(np.uint64) * np.uint64(83492791)) ^ np.uint64(seed * 2654435761 % 2**32)
        x = (x ^ (x >> np.uint64(13))) * np.uint64(1274126177)
        x = x ^ (x >> np.uint64(16))
        return (x & np.uint64(0xFFFF)).astype(np.float64) / 65535.0

    r = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                wgt = (u[:, 0] if dx else 1 - u[:, 0]) * (u[:, 1] if dy else 1 - u[:, 1]) * (u[:, 2] if dz else 1 - u[:, 2])
                r = r + wgt * h(dx, dy, dz)
    return r


def fbm(P, scale, seed, octaves=3):
    tot, amp, norm = 0.0, 1.0, 0.0
    for o in range(octaves):
        tot = tot + amp * vnoise(P, scale / 2 ** o, seed + o * 7)
        norm += amp
        amp *= 0.5
    return tot / norm


def ellipsoid_field(P, centre, radii):
    """1 at the centre falling to 0 on the ellipsoid surface, negative outside."""
    d = (P - np.array(centre)) / np.array(radii)
    return 1.0 - np.linalg.norm(d, axis=1)


def density(obj):
    return uv.measure_texel_density(obj, ATLAS)
