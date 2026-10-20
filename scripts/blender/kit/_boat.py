"""Shared builder for the P3 boats and their landing stage (owner-approved refs
assets-source/refs/boat_rowing ref-1..3 and boat_sail ref-1..2, D4 2026-10-20): `boat_rowing`,
`boat_sail` and `landing_stage`. Scripted route.

Frame (all three): Blender (x, y, z) exports as runtime (x, z, -y). The origin plane z = 0 is the
water surface at rest: the runtime stands the origin at `water::tiles::WATER_Y` (-1.4 m, water
contract §1) and lets the hull ride the live surface. A boat's bow faces +x and its pivot is the
middle of its hull length on the centreline; the landing stage's origin is the bank's waterline
point (4.0 m from the channel centre line, §2) with +x toward the centre line.

Animatable parts: a part that moves as a rigid transform (oars, sails) is a node and mesh
`<id>_<part>_lod<n>` beside each `<id>_lod<n>`, in every LOD, with `extras.kind` naming it ("oar",
"sail"). Its node sits on the pivot with the parent's orientation (oars) or with local +Z along the
swing axis (the jib's forestay), and its mesh is authored in the node frame, so the runtime draws it
at placement x node and swings it about the node's local axes. No part is named `<id>_rotor_lod<n>`:
both runtimes spin that name nonstop. Each LOD's budget is its body plus its parts (folded into
`lodTris` here, as `windmill_b` folds its rotor).

Look: every surface is a colour-only slot plus the piece's `masks` (R snow, G porosity, B night,
A baked AO), every body and part of every LOD on its own islands of one 1024² atlas, with sub-texel
strips filled from their slot (`_house.fill_unbaked`). The mesh accumulator and the bake sequence
are `windmill_b`'s (a monolithic generator, so copied, pitfalls.md Variants)."""
import json
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from _house import centroid_samples, fill_unbaked
from lib import bake, budgets, collider, export, geom, materials, scene, uv

MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.5
SHARP_DEG = 40.0
UV_MARGIN = 0.010
BAKE_MARGIN = 4
WATER_Y = -1.4  # water contract §1, runtime y of the origin plane


class Mesh:
    """bmesh accumulator with per-face slot indices (windmill_b's). Parts never share vertices unless
    built together, so no global weld runs (double-sided boards keep their own back sheets)."""

    def __init__(self, prefix, slots):
        self.bm = bmesh.new()
        self.prefix = prefix
        self.slots = list(slots)
        self.idx = {s: i for i, s in enumerate(self.slots)}

    def v(self, p):
        return self.bm.verts.new(Vector(p))

    def face(self, pts, slot, expect=None):
        vs = [v if isinstance(v, bmesh.types.BMVert) else self.bm.verts.new(Vector(v)) for v in pts]
        f = self.bm.faces.new(vs)
        f.material_index = self.idx[slot]
        if expect is not None:
            f.normal_update()
            if f.normal.dot(Vector(expect)) < 0:
                f.normal_flip()
        return f

    def board(self, pts, n, slot, thick=0.012):
        """A double-sided board: the front polygon on pts facing n and a back one `thick` behind it on
        its own vertices (a colour-only slot is culled single-sided)."""
        n = Vector(n).normalized()
        self.face([Vector(p) for p in pts], slot, n)
        self.face([Vector(p) - n * thick for p in pts], slot, -n)

    def box(self, lo, hi, slot, skip=()):
        """Axis-aligned box; `skip` names faces to leave out ("+x", "-z", ...)."""
        vs = {(sx, sy, sz): self.v((lo[0] if sx < 0 else hi[0], lo[1] if sy < 0 else hi[1],
                                    lo[2] if sz < 0 else hi[2]))
              for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)}
        for axis, name in ((0, "x"), (1, "y"), (2, "z")):
            for s in (1, -1):
                if f"{'+' if s > 0 else '-'}{name}" in skip:
                    continue
                corners = [k for k in vs if k[axis] == s]
                o = [k for k in (0, 1, 2) if k != axis]
                corners.sort(key=lambda k: math.atan2(k[o[1]], k[o[0]]))
                d = [0.0, 0.0, 0.0]
                d[axis] = s
                self.face([vs[k] for k in corners], slot, d)

    def tube(self, a, b, r, n, slot, rb=None, caps=(False, False), up=(0.0, 0.0, 1.0), phase=0.5):
        """An n-sided prism from a to b, radius r at a and rb at b."""
        a, b = Vector(a), Vector(b)
        t = (b - a).normalized()
        u = Vector(up) - t * Vector(up).dot(t)
        if u.length < 1e-6:
            u = Vector((1.0, 0.0, 0.0)) - t * t.x
        u.normalize()
        w = t.cross(u)
        rb = r if rb is None else rb

        def ring(c, rr):
            return [self.v(c + (u * math.cos(2 * math.pi * (k + phase) / n) + w * math.sin(2 * math.pi * (k + phase) / n)) * rr)
                    for k in range(n)]

        ra, rbv = ring(a, r), ring(b, rb)
        for k in range(n):
            j = (k + 1) % n
            mid = (ra[k].co + ra[j].co + rbv[k].co + rbv[j].co) / 4
            axis = a + t * (mid - a).dot(t)
            self.face([ra[k], ra[j], rbv[j], rbv[k]], slot, mid - axis)
        if caps[0]:
            self.face(ra, slot, -t)
        if caps[1]:
            self.face(rbv, slot, t)

    def obj(self, name, location=(0.0, 0.0, 0.0), rotation=None):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        for f in self.bm.faces:
            f.smooth = True
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        mesh.set_sharp_from_angle(angle=math.radians(SHARP_DEG))
        o = scene.link(bpy.data.objects.new(name, mesh))
        o.location = location
        if rotation is not None:
            o.rotation_mode = "QUATERNION"
            o.rotation_quaternion = rotation
        for s in self.slots:
            o.data.materials.append(bpy.data.materials[f"{self.prefix}_{s}"])
        return o


# ---------------------------------------------------------------------------------------------
# Hull loft. A hull spec h holds: L (length), HB (max half breadth at the sheer), UM (station of
# max beam), WT (transom half breadth / HB), PB (bow waterline exponent), F (freeboard at UF),
# SB / SS (sheer rise at bow / stern), UF (station of lowest sheer), T (keel depth below the
# water at UK), KB / KS (keel rise at the bow / stern), UK, N (section exponent: 2 = ellipse),
# TH (skin thickness), FLOOR (inner bottom above the keel).

def u_of(h, x):
    return (x + h["L"] / 2) / h["L"]


def x_of(h, u):
    return -h["L"] / 2 + h["L"] * u


def hb_of(h, u):
    um = h["UM"]
    if u <= um:
        f = h["WT"] + (1 - h["WT"]) * (1 - ((um - u) / um) ** 2)
    else:
        f = 1 - ((u - um) / (1 - um)) ** h["PB"]
    return h["HB"] * max(f, 0.0)


def zs_of(h, u):
    uf = h["UF"]
    if u >= uf:
        return h["F"] + h["SB"] * ((u - uf) / (1 - uf)) ** 2
    return h["F"] + h["SS"] * ((uf - u) / uf) ** 2


def zk_of(h, u):
    uk = h["UK"]
    if u >= uk:
        return -h["T"] + h["KB"] * ((u - uk) / (1 - uk)) ** 2.5
    return -h["T"] + h["KS"] * ((uk - u) / uk) ** 2


def section_point(hb, zs, zk, n, theta):
    """Point on a superelliptic section, theta 0 at the keel to pi/2 at the sheer (port side, y >= 0)."""
    s, c = math.sin(theta), math.cos(theta)
    return hb * s ** (2.0 / n), zs - (zs - zk) * c ** (2.0 / n)


def theta_at_depth(zs, zk, n, d):
    """theta of the point d below the sheer."""
    s = min(max(d / max(zs - zk, 1e-6), 0.0), 0.999)
    return math.acos(s ** (n / 2.0))


def stations(n, p=1.25):
    """n + 1 station u values from 0 (transom) to 1 (stem), denser toward the stem."""
    return [1 - (1 - i / n) ** p for i in range(n + 1)]


def ring_thetas(h, u, top, lower, outer=True):
    """Thetas of one section from the sheer down to the keel: `top` absolute depths below the sheer
    (slot boundaries), then `lower` even steps down to the keel."""
    zs, zk = zs_of(h, u), zk_of(h, u)
    if not outer:
        zk += h["FLOOR"]
    th = [math.pi / 2] + [theta_at_depth(zs, zk, h["N"], d) for d in top]
    last = th[-1]
    th += [last * (1 - k / lower) for k in range(1, lower + 1)]
    return th


def rake(h, u, z):
    """Forward shift of the bow sections with height, so the stem rakes RAKE forward from the
    forefoot (z = zk(1)) to the stem head; 0 aft of u = 0.7 and at the outer keel line."""
    zs, zk = zs_of(h, u), zk_of(h, u)
    t = min(max((z - zk) / max(zs - zk, 1e-6), 0.0), 1.0)
    w = min(max((u - 0.7) / 0.3, 0.0), 1.0)
    return h.get("RAKE", 0.0) * w * w * (3 - 2 * w) * t ** 1.5


def outer_ring(h, u, thetas):
    hb, zs, zk = hb_of(h, u), zs_of(h, u), zk_of(h, u)
    x = x_of(h, u)
    pts = [section_point(hb, zs, zk, h["N"], t) for t in thetas]  # sheer -> keel
    return [(x + rake(h, u, z), y, z) for y, z in pts]


def inner_ring(h, u, thetas, x=None):
    hb = max(hb_of(h, u) - h["TH"], 0.004)
    zs, zk = zs_of(h, u), zk_of(h, u) + h["FLOOR"]
    x = x_of(h, u) if x is None else x
    pts = [section_point(hb, zs, zk, h["N"], t) for t in thetas]
    return [(x + rake(h, u, z), y, z) for y, z in pts]


def inner_half_width(h, u, z):
    """Half width of the inner skin at height z (0 below its bottom)."""
    hb = max(hb_of(h, u) - h["TH"], 0.004)
    zs, zk = zs_of(h, u), zk_of(h, u) + h["FLOOR"]
    if z <= zk:
        return 0.0
    if z >= zs:
        return hb
    c = ((zs - z) / (zs - zk)) ** (h["N"] / 2.0)
    s = math.sqrt(max(1 - c * c, 0.0))
    return hb * s ** (2.0 / h["N"])


def hull(m, h, lod, inner_slot, cap_slot, transom_slot, keel_slot=None):
    """The planked hull: outer skin (lod["top"] = [(depth below the sheer, slot of the band above
    it), ...], then lod["below"] down to the keel in lod["lower"] rows), inner skin, gunwale strip,
    transom faces and cap, a bow deck over the inner skin's end with a bulkhead under its aft edge,
    and (lod["keel"]) a keel strip with a stem post. Returns the station u values."""
    us = stations(lod["nu"])
    tops = [d for d, _ in lod["top"]]
    slots_rows = [s for _, s in lod["top"]] + [lod["below"]] * lod["lower"]
    rings = []
    for i, u in enumerate(us):
        th = ring_thetas(h, u, tops, lod["lower"])
        pts = outer_ring(h, u, th)
        port = [m.v(p) for p in pts]
        if i == len(us) - 1:  # the stem: both sides share one line
            stbd = list(port)
        else:
            stbd = [m.v((p[0], -p[1], p[2])) for p in pts[:-1]] + [port[-1]]  # keel vertex shared
        rings.append((port, stbd))
    # Outer skin.
    for i in range(len(us) - 1):
        for side in (0, 1):
            a, b = rings[i][side], rings[i + 1][side]
            sgn = 1 if side == 0 else -1
            for j in range(len(a) - 1):
                q = [a[j], a[j + 1], b[j + 1], b[j]]
                c = sum((v.co for v in q), Vector()) / 4
                exp = Vector((0.0, c.y if abs(c.y) > 1e-4 else sgn * 1e-3, c.z - (zs_of(h, us[i]) + 1.0)))
                m.face(q, slots_rows[min(j, len(slots_rows) - 1)], exp)
    # Inner skin from the transom (moved in by TH) to the bow deck station.
    ibd = lod["bow_deck"]
    ith = None
    inner = []
    for i, u in enumerate(us[: ibd + 1]):
        ith = ring_thetas(h, u, [], lod["inner"], outer=False)
        x = x_of(h, u) + (h["TH"] if i == 0 else 0.0)
        pts = inner_ring(h, u, ith, x)
        port = [m.v(p) for p in pts]
        stbd = [m.v((p[0], -p[1], p[2])) for p in pts[:-1]] + [port[-1]]
        inner.append((port, stbd))
    for i in range(len(inner) - 1):
        for side in (0, 1):
            a, b = inner[i][side], inner[i + 1][side]
            sgn = 1 if side == 0 else -1
            for j in range(len(a) - 1):
                q = [a[j], a[j + 1], b[j + 1], b[j]]
                c = sum((v.co for v in q), Vector()) / 4
                exp = -Vector((0.0, c.y if abs(c.y) > 1e-4 else sgn * 1e-3, c.z - (zs_of(h, us[i]) + 1.0)))
                m.face(q, inner_slot, exp)
    # Gunwale strips (outer sheer to inner sheer) from the transom to the bow deck.
    for i in range(ibd):
        for side in (0, 1):
            q = [rings[i][side][0], rings[i + 1][side][0], inner[i + 1][side][0], inner[i][side][0]]
            m.face(q, cap_slot, (0, 0, 1))
    # Transom: outer face, inner face and cap.
    o_port, o_stbd = rings[0]
    loop = o_port + list(reversed(o_stbd[:-1]))
    m.face(loop, transom_slot, (-1, 0, 0))
    i_port, i_stbd = inner[0]
    m.face(i_port + list(reversed(i_stbd[:-1])), inner_slot, (1, 0, 0))
    m.face([o_port[0], o_stbd[0], i_stbd[0], i_port[0]], cap_slot, (0, 0, 1))
    # Bow deck over the inner skin's end, and a bulkhead under its aft edge.
    b_port, b_stbd = inner[-1]
    m.face(b_port + list(reversed(b_stbd[:-1])), inner_slot, (-1, 0, 0))
    deck = [rings[i][0][0] for i in range(ibd, len(us))]
    deck += [rings[i][1][0] for i in range(len(us) - 2, ibd - 1, -1)]
    m.face(deck, inner_slot, (0, 0, 1))
    if lod.get("keel"):
        keel(m, h, us, keel_slot or transom_slot, lod)
    return us


def keel(m, h, us, slot, lod):
    """A keel strip under the centreline from the transom to the forefoot, and the stem post up the
    bow to above the sheer."""
    kw, kd = h["KEEL_W"], h["KEEL_D"]
    xb = h["L"] / 2
    line = [(x_of(h, u), zk_of(h, u)) for u in us[:-1]] + [(xb - 0.02, zk_of(h, 1.0))]
    prev = None
    for x, zk in line:
        ring = [m.v((x, kw, zk + 0.01)), m.v((x, kw, zk - kd)), m.v((x, -kw, zk - kd)), m.v((x, -kw, zk + 0.01))]
        if prev is None:
            m.face(ring, slot, (-1, 0, 0))
        else:
            m.face([prev[0], prev[1], ring[1], ring[0]], slot, (0, 1, 0))
            m.face([prev[1], prev[2], ring[2], ring[1]], slot, (0, 0, -1))
            m.face([prev[2], prev[3], ring[3], ring[2]], slot, (0, -1, 0))
        prev = ring
    top = zs_of(h, 1.0) + h["STEM_HEAD"]
    m.tube((xb - 0.01, 0.0, zk_of(h, 1.0) - kd), (xb + h.get("RAKE", 0.0) + h["STEM"] * 0.5, 0.0, top),
           h["STEM"], 4, slot, caps=(False, True), up=(1.0, 0.0, 0.0), phase=0.0)


def frames(m, h, us_frames, slot, rows=4, width=0.04, proud=0.014, z_stop=None):
    """Ribs on the inner skin: a ribbon facing inward from under the gunwale down to the keel, or to
    height z_stop (the floorboards) on each side, at each station u."""
    for u in us_frames:
        x = x_of(h, u)
        th = ring_thetas(h, u, [0.04], rows, outer=False)[1:]  # stop under the gunwale
        if z_stop is not None:
            zs, zk = zs_of(h, u), zk_of(h, u) + h["FLOOR"]
            th = [theta_at_depth(zs, zk, h["N"], 0.04 + (zs - z_stop - 0.04) * k / rows) for k in range(rows + 1)]
        for side in (1, -1):
            pts = []
            zs = zs_of(h, u)
            for t in th:
                ua, ub = u_of(h, x - width / 2), u_of(h, x + width / 2)
                pa = inner_ring(h, ua, [t])[0]
                pb = inner_ring(h, ub, [t])[0]
                pa = Vector((x - width / 2, side * pa[1], pa[2]))
                pb = Vector((x + width / 2, side * pb[1], pb[2]))
                for p in (pa, pb):
                    d = Vector((0.0, -p.y, zs + 0.3 - p.z)).normalized()
                    p += d * proud
                pts.append((m.v(pa), m.v(pb)))
            for (a0, b0), (a1, b1) in zip(pts, pts[1:]):
                c = (a0.co + b0.co + a1.co + b1.co) / 4
                m.face([a0, b0, b1, a1], slot, Vector((0.0, -c.y, zs + 0.3 - c.z)))


def thwart(m, h, x, z_top, wx, thick, slot, lod_skip=()):
    """A seat across the boat, its ends buried 1 cm into the inner skin."""
    hw = min(inner_half_width(h, u_of(h, x - wx / 2), z_top - thick),
             inner_half_width(h, u_of(h, x + wx / 2), z_top - thick)) + 0.01
    m.box((x - wx / 2, -hw, z_top - thick), (x + wx / 2, hw, z_top), slot, skip=("+y", "-y") + tuple(lod_skip))
    return hw


def floorboards(m, h, ys, z, u_range, slot, wy=0.09, thick=0.02, step=0.02, ends=True):
    """Slats along x at height z, each as long as the inner skin is wider than the slat there."""
    for y in ys:
        lo = hi = None
        u = u_range[0]
        while u <= u_range[1]:
            if inner_half_width(h, u, z - thick) > abs(y) + wy / 2 + 0.01:
                lo = u if lo is None else lo
                hi = u
            u += step / h["L"]
        if lo is None:
            continue
        skip = () if ends else ("+x", "-x")
        m.box((x_of(h, lo), y - wy / 2, z - thick), (x_of(h, hi), y + wy / 2, z), slot, skip=("-z",) + skip)


# ---------------------------------------------------------------------------------------------
# Collider, bake and export.

def box_collider(src, name):
    """AABB of `src` with an oblique planar UV, flat shaded (`_train.py`: a hull or box over a
    coarse LOD gave zero tangents on the engine without it)."""
    col = collider.collider_from(src, "box", name)
    col.data.materials.clear()
    for c in list(col.data.color_attributes):
        col.data.color_attributes.remove(c)
    col.data.shade_flat()
    if "UVMap" not in col.data.uv_layers:
        col.data.uv_layers.new(name="UVMap")
    for poly in col.data.polygons:
        for li in poly.loop_indices:
            co = col.data.vertices[col.data.loops[li].vertex_index].co
            col.data.uv_layers["UVMap"].data[li].uv = (0.05 * (co.x + 0.37 * co.z), 0.05 * (co.y + 0.61 * co.z + 0.23 * co.x))
    return col


def make_materials(prefix, slots):
    for s, (col, rough, _) in slots.items():
        materials.pbr_material(f"{prefix}_{s}", (*col, 1.0), rough)


def finish(P, slots, bodies, parts, collider_obj, out, generator, report_extra=None, bake_extra=None):
    """UV atlas, masks bake (AO + hooks), export and reports. `parts` is a list per LOD of part
    objects (already parented nowhere); every object bakes alone so no part's shadow moves with it."""
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    names = [f"{P}_{s}" for s in slots]
    objs = list(bodies) + [o for lod in parts for o in lod]
    tris = [geom.tri_count(b) + sum(geom.tri_count(o) for o in parts[i]) for i, b in enumerate(bodies)]
    split = {o.name: geom.tri_count(o) for o in objs}
    print(f"{P}: tris per LOD {tris} (body + parts) {split}")
    budgets.check(tuple(tris), "prop", P)

    uv.atlas_unwrap(objs, margin=UV_MARGIN)
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in objs}
    print(f"{P}: masks {MASK_SIZE}² density {density}")
    signature = bake.receiver_signature(objs)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(P)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{P}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(objs):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{P}.ao.tmp.png"))
        mats = [bpy.data.materials[n] for n in names]
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for s, (_, _, hk) in slots.items():
            bpy.data.materials[f"{P}_{s}"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*hk, 1.0)
        hooks = bake.new_image(f"{P}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(objs):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{P}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{P}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        px, filled = fill_unbaked(px, objs, MASK_SIZE, [hk for (_, _, hk) in slots.values()])
        bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        samples = centroid_samples(objs, px, MASK_SIZE, names, P)
        stats = {"aoMean": float(a.mean()), "aoP05": float(np.percentile(a, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": {o.name: samples[f"lod{i}"] for i, o in enumerate(objs)}}
        print(f"{P}: masks {stats}")
        bad = {o.name: samples[f"lod{i}"]["unbakedTris"] for i, o in enumerate(objs)
               if samples[f"lod{i}"]["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

    root = bpy.data.objects[P]
    for o in objs:
        o.parent = root
    collider_obj.parent = root
    export.export_glb(root, str(out / f"{P}.glb"))
    report = export.write_export_report(str(out / "export-report.json"), root, P, generator)
    report["lodTris"] = {f"{P}_lod{i}": t for i, t in enumerate(tris)}
    report["partTris"] = split
    report.update(report_extra or {})
    (out / "export-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{P}_lod0"], masksDensityPxPerM=density,
                           uvMargin=UV_MARGIN, maskStats=stats, skipped=skip_bake, **(bake_extra or {}))
    return tris


INTERIOR_MIN_Z = 0.03  # the water surface is drawn through the hull, so nothing inboard dips below it


def interior_low(o, slot_names):
    """Lowest vertex of the faces in `slot_names` (the inboard surfaces): it must stay above the water
    surface at rest, or the engine's water plane shows inside the boat."""
    idx = {i for i, m in enumerate(o.data.materials) if m.name.split(".")[0].split("_")[-1] in slot_names}
    zs = [o.data.vertices[v].co.z for p in o.data.polygons if p.material_index in idx for v in p.vertices]
    return min(zs) if zs else 1e9


def out_dir():
    return pathlib.Path(sys.argv[sys.argv.index("--") + 1])


def quat_z_to(axis):
    """Rotation taking local +Z to `axis` (shortest arc)."""
    return Vector((0.0, 0.0, 1.0)).rotation_difference(Vector(axis).normalized())


def world_points(o, mat=None):
    """Vertex positions of o in its parent frame, after an optional extra local transform."""
    co = [Vector(v.co) for v in o.data.vertices]
    m = o.matrix_basis if mat is None else o.matrix_basis @ mat
    return [m @ c for c in co]


def rot_z(deg):
    return Matrix.Rotation(math.radians(deg), 4, "Z")


def rot_x(deg):
    return Matrix.Rotation(math.radians(deg), 4, "X")
