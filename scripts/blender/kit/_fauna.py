"""Shared builder for the boid fauna pieces bird_starling and fish_trout (PRD §7.2 boids).

Frame: a creature is authored in its own frame (x lateral, f forward, z up; metres, real scale) and
placed in Blender at (x, -f, z), so runtime (x, y, z) = (x, z, f): it faces runtime +z with +y up, the
local frame of the boid forms in `assets/shaders/boid_material.wesl`. The pivot is the body centre on
the wing or fin hinge line, where the boid's simulated position sits. Kind `prop` (LOD0 <= 1,500 tris,
LOD1 <= 50 %, LOD2 <= 15 %, KTX2 class `piece`).

UV0 is analytic, so every LOD maps the same surface to the same texels: LODs are rebuilt from one spec
at lower ring counts, never decimated. The left and right halves share mirrored islands. TEXCOORD_1
(Blender UV map `motion`, created after UVMap so UVMap stays TEXCOORD_0) carries per-vertex motion
weights for the boid vertex shader, U and V each in [0, 1]. The glTF exporter writes V as 1 - v, so V is
stored pre-flipped and the GLB holds the weight itself.

Maps are drawn in numpy, not baked from a colour shader: every LOD0 triangle is rasterized in UV space
with its interpolated creature-frame position, and the piece's paint function returns sRGB albedo,
roughness, metallic and a height field per texel. The normal map is that height's gradient in tangent
space (R along +u, G along +v, as Blender bakes). Only AO is baked, by Cycles on LOD0 onto itself.
One slot, `<id>_body`, unique maps plus masks."""
import json
import math
import os
import pathlib

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, collider, export, geom, materials, scene, uv

SAMPLES, AO_DIST, MARGIN = 32, 0.08, 4


def to_blender(x, f, z):
    return (x, -f, z)


class Builder:
    """bmesh accumulator: UVMap and motion per loop, a part id per face."""

    def __init__(self):
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")
        self.mo = self.bm.loops.layers.uv.new("motion")
        self.part = self.bm.faces.layers.int.new("part")

    def vert(self, p):
        """A vertex at creature-frame p = (x, f, z); keeps p for the UV and motion functions."""
        v = self.bm.verts.new(to_blender(*p))
        return v, tuple(p)

    def face(self, vs, part, uv_fn, mo_fn, inside):
        """vs: [(BMVert, p)]. Oriented so its normal points away from creature point `inside`.
        uv_fn(p) -> (u, v); mo_fn(p) -> (U, V) weights (V is stored as 1 - V)."""
        co = [Vector(to_blender(*p)) for _, p in vs]
        n = Vector()
        for i in range(len(co)):  # Newell normal, robust for ngons
            a, b = co[i], co[(i + 1) % len(co)]
            n += Vector(((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y)))
        c = sum(co, Vector()) / len(co)
        if n.dot(c - Vector(to_blender(*inside))) < 0:
            vs = list(reversed(vs))
        f = self.bm.faces.new([v for v, _ in vs])
        f[self.part] = part
        f.smooth = True
        for loop, (_, p) in zip(f.loops, vs):
            loop[self.uv].uv = uv_fn(p)
            mu, mv = mo_fn(p)
            loop[self.mo].uv = (mu, 1.0 - mv)
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


def ellipse_arc(w, h, theta):
    """Arc length from the dorsal point (theta 0) of the ellipse (w sin t, h cos t) to theta <= pi."""
    t = np.linspace(0.0, math.pi, 257)
    ds = np.sqrt((w * np.cos(t)) ** 2 + (h * np.sin(t)) ** 2)
    s = np.concatenate([[0.0], np.cumsum((ds[1:] + ds[:-1]) / 2 * np.diff(t))])
    return float(np.interp(theta, t, s))


def loft(b, stations, n, part, body_uv, mo_fn, cap_uv):
    """Closed body of revolution along f. stations: (f, w, h, zc) ordered front to back; w = 0 is a
    pole. Ring vertex k sits at theta = 2 pi k / n from the dorsal line, x = w sin theta. body_uv(f, arc)
    maps the station and the arc from the dorsal line (mirrored on the -x side) to UV0. The last
    station, if a ring, is closed by a planar cap mapped by cap_uv(p)."""
    rings = []
    for f, w, h, zc in stations:
        if w == 0:
            rings.append([(b.vert((0.0, f, zc)), 0.0, f, zc)])
            continue
        r = []
        for k in range(n):
            th = 2 * math.pi * k / n
            p = (w * math.sin(th), f, zc + h * math.cos(th))
            mirrored = th if th <= math.pi else 2 * math.pi - th
            r.append((b.vert(p), ellipse_arc(w, h, mirrored), f, zc))
        rings.append(r)
    uvs = {}
    for r in rings:
        for (v, p), arc, f, _ in r:
            uvs[p] = body_uv(f, arc)
    uvf = lambda p: uvs[p]  # noqa: E731
    for a, c in zip(rings, rings[1:]):
        # Orientation reference: the axis point between the two stations.
        centre = (0.0, (a[0][2] + c[0][2]) / 2, (a[0][3] + c[0][3]) / 2)
        if len(a) == 1 or len(c) == 1:
            pole, ring = (a, c) if len(a) == 1 else (c, a)
            for k in range(n):
                vs = [pole[0][0], ring[k][0], ring[(k + 1) % n][0]]
                b.face(vs, part, uvf, mo_fn, centre)
            continue
        for k in range(n):
            vs = [a[k][0], a[(k + 1) % n][0], c[(k + 1) % n][0], c[k][0]]
            b.face(vs, part, uvf, mo_fn, centre)
    if len(rings[-1]) > 1:
        last = rings[-1]
        inside = (0.0, last[0][2] + 0.01, stations[-1][3])
        b.face([e[0] for e in last], part, cap_uv, mo_fn, inside)


def plate(b, outline, place, part, uv_fn, mo_fn, normal, eps, sides=(1, -1)):
    """A thin double-sided plate: one sheet per side in `sides`, each offset by eps along its own
    outward normal (creature-frame `normal` for side 1, its negation for -1), with its own vertices so
    each side shades flat. outline: 2D points (a, b); place(a, b) -> creature point. uv_fn(a, b, side).
    part is an int or {side: int}."""
    for s in sides:
        pid = part if isinstance(part, int) else part[s]
        nx, nf, nz = (c * s for c in normal)
        pts = []
        for a, bb in outline:
            x, f, z = place(a, bb)
            pts.append(((x + nx * eps, f + nf * eps, z + nz * eps), (a, bb)))
        vs = [b.vert(p) for p, _ in pts]
        ab = {p: q for p, q in pts}
        inside = tuple(sum(p[i] for p, _ in pts) / len(pts) - (nx, nf, nz)[i] * 0.01 for i in range(3))
        b.face(vs, pid, lambda p, s=s: uv_fn(*ab[p], s), mo_fn, inside)


def rasterize(obj, size):
    """Every triangle of obj rasterized at texel centres in UV0. Returns (idx, P, part): idx the
    (size*size,) texel -> sample index (-1 outside every island, Blender pixel order), P the
    creature-frame (x, f, z) per sample and part the face part id per sample."""
    me = obj.data
    me.calc_loop_triangles()
    uvd = me.uv_layers["UVMap"].data
    parts = me.attributes["part"].data
    idx = -np.ones(size * size, np.int64)
    P, part = [], []
    count = 0
    for tri in me.loop_triangles:
        t = np.array([uvd[li].uv[:] for li in tri.loops]) * size
        co = np.array([me.vertices[vi].co[:] for vi in tri.vertices])
        co = np.stack([co[:, 0], -co[:, 1], co[:, 2]], 1)  # creature frame
        lo, hi = np.floor(t.min(0)).astype(int), np.ceil(t.max(0)).astype(int)
        xs, ys = np.meshgrid(np.arange(lo[0], hi[0]) + 0.5, np.arange(lo[1], hi[1]) + 0.5)
        (ax, ay), (bx, by), (cx, cy) = t
        d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(d) < 1e-12:
            continue
        l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
        l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
        l3 = 1 - l1 - l2
        inside = (l1 >= -1e-6) & (l2 >= -1e-6) & (l3 >= -1e-6)
        ix, iy = xs[inside].astype(int), ys[inside].astype(int)
        ok = (ix >= 0) & (ix < size) & (iy >= 0) & (iy < size)
        ix, iy = ix[ok], iy[ok]
        bary = np.stack([l1[inside][ok], l2[inside][ok], l3[inside][ok]], 1)
        pos = bary @ co
        k = len(pos)
        idx[iy * size + ix] = np.arange(count, count + k)
        P.append(pos)
        part.append(np.full(k, parts[tri.polygon_index].value))
        count += k
    return idx, np.concatenate(P), np.concatenate(part)


def fill_index(idx, size, width):
    """Fill every texel of columns [0, width) with its nearest island's sample (grown 4-neighbour),
    so box-filtered mips never average a background colour into an island edge: boids are drawn a
    few dozen pixels long and sample deep mips. Texels at or right of `width` stay -1."""
    g = idx.reshape(size, size)[:, :width].copy()
    while (g < 0).any():
        miss = g < 0
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nb = np.roll(g, (dy, dx), (0, 1))
            take = miss & (nb >= 0) & (g < 0)
            g[take] = nb[take]
    out = -np.ones((size, size), np.int64)
    out[:, :width] = g
    return out.ravel()


def height_to_normal(height, size, px_per_m):
    """Tangent-space normal (R +u, G +v, B out) from a (size, size) height field in metres."""
    gx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) / 2 * px_per_m
    gy = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) / 2 * px_per_m
    n = np.stack([-gx, -gy, np.ones_like(gx)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5


def bake_ao(obj, size, out_dir, piece):
    """Cycles AO of obj onto itself (UV0), as a (size*size,) float array in Blender pixel order."""
    bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{piece}_world")
    bpy.context.scene.world.light_settings.distance = AO_DIST
    bake.setup_cycles(samples=SAMPLES)
    img = bake.new_image(f"{piece}.ao", size, non_color=True)
    bake.bake_to(obj, "AO", img, margin=MARGIN)
    p = bake.save_png(img, str(pathlib.Path(out_dir) / f"{piece}.ao.tmp.png"))
    ao = bake.load_png(p)[1][:, 0].copy()
    os.remove(p)
    return ao


def paint_maps(lod0, size, paint, halves, out_dir, piece, skip_bake, mask_size, hooks):
    """Writes <piece>.{albedo,normal,orm,masks}.png. `halves` is a list of (u offset in texels, paint
    variant); every variant paints the same rasterized islands shifted by its offset (the fish's two
    palettes), or one entry (0, None). paint(P, part, variant) -> dict(rgb, rough, metal, height).
    hooks(part) -> (snow, wet) per sample. Returns bake statistics."""
    idx, P, part = rasterize(lod0, size)
    width = size if len(halves) == 1 else size // len(halves)
    filled = fill_index(idx, size, width)
    cov = idx >= 0
    texel_of = np.zeros(len(P), np.int64)
    texel_of[idx[cov]] = np.nonzero(cov)[0]
    ao_full = np.ones(size * size, np.float32)
    if not skip_bake:
        ao_full = bake_ao(lod0, size, out_dir, piece)
    alb = np.zeros((size * size, 4), np.float32)
    alb[:, 3] = 1.0
    nrm = np.zeros((size * size, 4), np.float32)
    nrm[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.zeros((size * size, 4), np.float32)
    orm[:] = (1.0, 0.6, 0.0, 1.0)
    snow = np.zeros(size * size, np.float32)
    wet = np.zeros(size * size, np.float32)
    has = filled >= 0
    src = filled[has]
    ao_s = ao_full.copy()
    sn, we = hooks(part)
    stats = {}
    for off, variant in halves:
        out = paint(P, part, variant)
        tgt = np.nonzero(has)[0]
        ty, tx = tgt // size, tgt % size + off
        assert tx.max() < size, f"{piece}: islands plus gutter spill past the half at offset {off}"
        t2 = ty * size + tx
        alb[t2, :3] = out["rgb"][src]
        # AO of the island texel each texel was filled from (the bake leaves 0 past its margin).
        orm[t2, 0] = np.where(cov[tgt], ao_s[tgt], ao_s[texel_of[src]])
        orm[t2, 1] = out["rough"][src]
        orm[t2, 2] = out["metal"][src]
        snow[t2] = sn[src]
        wet[t2] = we[src]
        h = np.zeros(size * size, np.float32)
        h[tgt] = out["height"][src]
        nn = height_to_normal(h.reshape(size, size), size, out["px_per_m"]).reshape(-1, 3)
        nrm[t2, :3] = nn[tgt]
        lum = out["rgb"][src][cov[has]].mean(1)
        stats[f"albedoLuma{'' if variant is None else '_' + variant}"] = {
            "mean": float(lum.mean()), "p05": float(np.percentile(lum, 5))}
    aoc = ao_s[cov]
    stats["ao"] = {"texels": int(cov.sum()), "mean": float(aoc.mean()), "p05": float(np.percentile(aoc, 5)),
                   "min": float(aoc.min())}
    stats["coverage"] = float(cov.mean())
    bake.write_png(size, alb, str(pathlib.Path(out_dir) / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(size, nrm, str(pathlib.Path(out_dir) / f"{piece}.normal.png"))
    bake.write_png(size, orm, str(pathlib.Path(out_dir) / f"{piece}.orm.png"))
    f = size // mask_size
    down = lambda a: a.reshape(mask_size, f, mask_size, f).mean(axis=(1, 3)).ravel()  # noqa: E731
    bake.make_masks(mask_size, down(snow), down(wet), 0.0, None, str(pathlib.Path(out_dir) / f"{piece}.masks.png"))
    return stats


def export_piece(piece, lods, out_dir, generator, extras, texel, size, mask_size, stats, skip_bake):
    """Collider (box of LOD2), root with the boid contract as extras, GLB, export and bake reports."""
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    assert texel["median"] >= 256, texel
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    root = scene.empty(piece)
    root["boid"] = json.dumps(extras, sort_keys=True)
    for o in lods:
        o.parent = root
    col.parent = root
    out = pathlib.Path(out_dir)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           aoDistanceM=AO_DIST, colourSource="numpy paint (no colour bake)",
                           channels={"albedo": {"size": size, "colorspace": "srgb"},
                                     "normal": {"size": size, "colorspace": "linear"},
                                     "orm": {"size": size, "colorspace": "linear"},
                                     "masks": {"size": mask_size, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, bakeStats=stats, lodTris=tris, boid=extras,
                           skipped=skip_bake)
    print(f"{piece}: tris per LOD {tris}, texel {texel}, stats {stats}")
    return tris


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def hexc(s):
    return np.array([int(s[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255.0


def mix(a, b, t):
    t = np.asarray(t, np.float32)[..., None]
    return a * (1 - t) + b * t


def setup(piece):
    scene.reset_scene(piece)
    return materials.pbr_material(f"{piece}_body", materials.srgb("#808080"), 0.5)


def density(obj, size):
    return uv.measure_texel_density(obj, size)
