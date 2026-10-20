"""Fallen-log variants of log_fallen (PRD §6 Variety): the same spruce log on the forest floor, its
bark the tileable `bark` set times the trunks' tint with moss and broken end-grain baked into a
unique atlas (refs: assets-source/refs/forest, "fallen log"), in other lengths, breaks and moss
patterns. `log_fallen.py` is frozen with the approved original (provenance hashes it), so its
parametric surface, metric atlas layout, high-poly materials, bake and collider steps are carried
over here unchanged in look, with the shape and the moss pattern as parameters (`Spec`).

As in log_fallen: the pivot is on the ground at the log's middle, the log lies along Blender x sunk
`bury` below the pivot, the butt (root flare and snapped roots) is at -x and the break at +x. One
parametric surface is sampled at every density: the high poly for the bake and LOD0/1/2 at fewer
sides and rings, so all LODs share one metric UV layout (body as two half-length islands, caps, the
break and every stub in rows below) and sample LOD0's atlas exactly, with no decimation. The break
is either log_fallen's hollow (rim annulus, inner wall and floor) or a solid snapped end (a fan to a
splintered peak). Baked selected-to-active: albedo through EMIT, tangent normal, AO with a ground
plane as occluder into ORM.R, roughness into ORM.G, the moss mask into masks.G. Normal and ORM ship
at 512² (box-filtered)."""
from __future__ import annotations

import math
import os
import pathlib
from dataclasses import dataclass, field

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv
from _shared import island_texels

ATLAS, SMALL, MASK_SIZE = 1024, 512, 128
REPO = pathlib.Path(__file__).resolve().parents[3]
BARK = REPO / "assets/kit/p1/materials/bark"
BARK_TINT = (0.92, 0.8, 0.72)  # tree_forest_bark's manifest colour, as log_fallen
DENSITY = 276.0  # log_fallen's layout density (px/m)
K = DENSITY / ATLAS  # UV units per metre
ATLAS_M = ATLAS / DENSITY  # the atlas side in metres
GAP = 0.06  # metres between islands: 18 px at DENSITY, at least 2 x the bake margin + 2 px
CAGE, MAX_RAY, MARGIN, SAMPLES, AO_DIST = 0.05, 0.14, 8, 24, 0.3


@dataclass
class Moss:
    """Where moss grows, as log_fallen's mask with its parameters exposed. A flank term: faces whose
    normal turns toward `toward` (dot from lo to hi), patchy by a low noise of `scale` on object
    coordinates multiplied by `stretch` (x < 1 draws bands along the grain) and thresholded at
    `cut`. Plus optional top cushions: up-facing faces (n.z from `top`) where a second noise of
    `cushion_scale` passes `cushion_cut`. Ragged everywhere by the fine noise (scale 18)."""
    toward: tuple[float, float, float] = (0.0, 0.0, 1.0)
    lo_hi: tuple[float, float] = (0.15, 0.6)
    scale: float = 2.2
    stretch: tuple[float, float, float] = (1.0, 1.0, 1.0)
    cut: tuple[float, float] = (0.58, 0.72)
    top: tuple[float, float] | None = None
    cushion_scale: float = 6.0
    cushion_cut: tuple[float, float] = (0.66, 0.74)
    offset: tuple[float, float, float] = (0.0, 0.0, 0.0)


@dataclass
class Spec:
    """One log. Stubs are (x on the body, angle around (0 = +y, pi/2 = up), lean toward -x (rad),
    length, base and tip radius); `lod1_stubs` are the stub indices LOD1 keeps. `rim` is the break's
    jagged end as [(amplitude m, frequency, phase)] about half_l. `hollow` keeps log_fallen's hollow
    tip (inner radius fraction, depth m), else the break is a solid snapped end whose centre peaks
    `peak` (x, y, z m) off the rim's mean point, through a ring of `splinters` at half the radius."""
    half_l: float
    r0: float
    bury: float
    stubs: list[tuple[float, float, float, float, float, float]]
    lod1_stubs: list[int]
    rim: list[tuple[float, float, float]]
    taper: float = 0.016
    flare: tuple[float, float] = (0.075, 0.5)  # (radius gain m, length m) of the butt's root flare
    lump: float = 0.07
    bend: tuple[float, float] = (0.05, 0.85)  # (amplitude m, frequency) of the plan-view bend
    seed_off: tuple[float, float, float] = (13.1, -7.7, 4.2)
    hollow: tuple[float, float] | None = (0.6, 0.38)
    peak: tuple[float, float, float] = (0.06, 0.0, 0.0)
    splinters: list[tuple[float, float, float]] = field(default_factory=list)  # (amplitude m, frequency, phase)
    rings: tuple[int, int, int, int] = (18, 8, 4, 140)  # LOD0, LOD1, LOD2, high poly
    moss: Moss = field(default_factory=Moss)


class Log:
    """The parametric surface and the atlas layout of one Spec (log_fallen's module-level steps)."""

    def __init__(self, s: Spec):
        self.s = s
        assert all(r % 2 == 0 for r in s.rings), s.rings  # each body island holds exactly half the length
        self.off = Vector(s.seed_off)
        self.c_ref = 2 * math.pi * s.r0 * 1.06  # circumference the body islands are laid out at (m)
        rim_max = s.half_l + sum(abs(a) for a, _, _ in s.rim)
        body_h = (rim_max + s.half_l) / 2 + 0.01  # each half island holds at most half the length
        self.body_a = self.rect(GAP, ATLAS_M - GAP - body_h, self.c_ref, body_h)
        self.body_b = self.rect(2 * GAP + self.c_ref, ATLAS_M - GAP - body_h, self.c_ref, body_h)
        n = len(s.stubs)
        row_a, row_b = list(range(min(3, n))), list(range(min(3, n), n))
        rects, u = {}, GAP
        for k in row_a:
            w, h = self.stub_box(k)
            rects[k] = self.rect(u, GAP, w, h)
            u += w + GAP
        cap_w = 2 * (s.r0 + s.flare[0] * 1.2) * 1.08  # the flared butt (radius + flare, lumpy section)
        rim_w = 2 * s.r0 * 1.1 * (1.0 if s.hollow else 1.12)
        self.cap_butt = self.rect(u, GAP, cap_w, cap_w)
        u += cap_w + GAP
        self.rim_box = self.rect(u, GAP, rim_w, rim_w)
        top_a = GAP + max([self.stub_box(k)[1] for k in row_a] + [cap_w, rim_w])
        u = GAP
        for k in row_b:
            w, h = self.stub_box(k)
            rects[k] = self.rect(u, top_a + GAP, w, h)
            u += w + GAP
        self.rects = [self.cap_butt, self.rim_box, self.body_a, self.body_b]
        if s.hollow:
            self.inner_c = 2 * math.pi * s.r0 * s.hollow[0]  # the hollow's circumference at its mouth
            self.inner_wall = self.rect(u, top_a + GAP, self.inner_c, s.hollow[1] + 0.04)
            u += self.inner_c + GAP
            self.inner_floor = self.rect(u, top_a + GAP, 0.3, 0.3)
            self.rects += [self.inner_wall, self.inner_floor]
        self.stub_rects = [rects[k] for k in range(n)]
        self.rects += self.stub_rects
        for i, a in enumerate(self.rects):
            for b in self.rects[i + 1:]:
                g = GAP * K * 0.99
                assert (a[2] + g <= b[0] or b[2] + g <= a[0] or a[3] + g <= b[1] or b[3] + g <= a[1]), \
                    ("uv rects closer than GAP", a, b)

    @staticmethod
    def rect(u_m, v_m, w_m, h_m):
        """An atlas rectangle at (u_m, v_m) of w_m x h_m, all in metres; asserts it stays inside."""
        r = (u_m * K, v_m * K, (u_m + w_m) * K, (v_m + h_m) * K)
        assert 0.0 <= r[0] and r[2] <= 1.0 and 0.0 <= r[1] and r[3] <= 1.0, r
        return r

    def stub_box(self, k):
        """(width, height) in metres of stub k's island: the tube unrolled, then its broken tip cap."""
        _, _, _, L, r0, r1 = self.s.stubs[k]
        return max(2 * math.pi * r0, 2.6 * r1) + 0.02, 0.01 + L + 1.2 * r0 + 0.03 + 2.6 * r1 + 0.01

    def radius(self, x, th):
        """Outer radius (m) at x along the log and angle th around it: a gentle taper, the root
        flare over the butt's last flare[1] m, and a lumpy cross-section."""
        s = self.s
        r = s.r0 - s.taper * (x / s.half_l)
        f = min(max((-(s.half_l - s.flare[1]) - x) / s.flare[1], 0.0), 1.0)
        r += s.flare[0] * f * f
        return r * (1.0 + s.lump * noise.noise(Vector((x * 1.1, math.cos(th) * 0.9, math.sin(th) * 0.9)) + self.off))

    def axis(self, x):
        """Centre line: sunk bury below the pivot under the mean radius, with a slight bend in plan."""
        return Vector((x, self.s.bend[0] * math.sin(x * self.s.bend[1]), self.s.r0 - self.s.bury))

    @staticmethod
    def around(th):
        return Vector((0.0, math.cos(th), math.sin(th)))

    def rim_x(self, th):
        """The break's jagged end (x), low-frequency so the LOD0 sides hold it."""
        return self.s.half_l + sum(a * math.sin(f * th + p) for a, f, p in self.s.rim)

    def surf(self, x, th):
        return self.axis(x) + self.around(th) * self.radius(x, th)

    def build(self, name, sides, rings, stub_sides, stub_segs, inner_segs, stubs, high=False):
        """One mesh of the log. UVMap: the atlas; on the high poly also `bark` (metric, for the set)
        and material index 0 bark, 1 end-grain wood."""
        s, half_l = self.s, self.s.half_l
        bm = bmesh.new()
        uvl = bm.loops.layers.uv.new("UVMap")
        bkl = bm.loops.layers.uv.new("bark") if high else None

        def face(vs, uvs, barks=None, mat=0):
            f = bm.faces.new(vs)
            f.smooth = True
            f.material_index = mat
            for lp, t in zip(f.loops, uvs):
                lp[uvl].uv = t
            if bkl is not None and barks is not None:
                for lp, t in zip(f.loops, barks):
                    lp[bkl].uv = t
            return f

        def planar(p, centre, box_c):
            d = p - centre
            return (box_c[0] + d.y * K, box_c[1] + d.z * K)

        ths = [-math.pi / 2 + 2 * math.pi * j / sides for j in range(sides)]
        # Body: rings from the butt (x = -half_l) to the jagged rim, column j ending at rim_x(th_j).
        grid, xs = [], []
        for i in range(rings + 1):
            t = i / rings
            row, xrow = [], []
            for th in ths:
                x = -half_l + t * (self.rim_x(th) + half_l)
                row.append(bm.verts.new(self.surf(x, th)))
                xrow.append(x)
            grid.append(row)
            xs.append(xrow)
        assert rings % 2 == 0, rings
        mid = rings // 2
        c_ref = self.c_ref
        for i in range(rings):
            box = self.body_a if i < mid else self.body_b
            for j in range(sides):
                j1 = (j + 1) % sides
                cols = (j, j + 1)
                quad = (grid[i][j], grid[i][j1], grid[i + 1][j1], grid[i + 1][j])
                uvs, barks = [], []
                for ii, jj, cj in ((i, j, cols[0]), (i, j1, cols[1]), (i + 1, j1, cols[1]), (i + 1, j, cols[0])):
                    x0 = xs[mid][jj] if i >= mid else -half_l
                    uvs.append((box[0] + cj / sides * c_ref * K, box[1] + (xs[ii][jj] - x0) * K))
                    barks.append((cj / sides * c_ref / 2.0, (xs[ii][jj] + half_l) / 2.0))
                face(quad, uvs, barks, 0)
        # Butt cap: a fan to a slightly domed centre, planar (y, z) UVs.
        c = bm.verts.new(self.axis(-half_l) + Vector((-0.025, 0, 0)))
        ring = [bm.verts.new(v.co) for v in grid[0]]
        cb = self.cap_butt
        cu, cv = (cb[0] + cb[2]) / 2, (cb[1] + cb[3]) / 2
        ac = self.axis(-half_l)
        for j in range(sides):
            a, b = ring[(j + 1) % sides], ring[j]
            face((c, a, b), [planar(v.co, ac, (cu, cv)) for v in (c, a, b)], None, 1)
        rb = self.rim_box
        ru, rv = (rb[0] + rb[2]) / 2, (rb[1] + rb[3]) / 2
        end = [bm.verts.new(v.co) for v in grid[-1]]
        at = self.axis(half_l)
        if s.hollow is None:
            # Solid snapped end: the jagged rim, a ring of splinters at half the radius that juts out
            # past the rim or falls back from it (`splinters`), then a fan to a peak off the rim's mean.
            mean = sum((v.co for v in end), Vector()) / len(end)
            mid_ring = []
            for j, th in enumerate(ths):
                x = self.rim_x(th) + sum(a * math.sin(f * th + p) for a, f, p in s.splinters)
                p = self.axis(x) + self.around(th) * self.radius(x, th) * 0.5
                mid_ring.append(bm.verts.new(p))
            for j in range(sides):
                j1 = (j + 1) % sides
                q = (end[j], end[j1], mid_ring[j1], mid_ring[j])
                face(q, [planar(v.co, at, (ru, rv)) for v in q], None, 1)
            tip = bm.verts.new(mean + Vector(s.peak))
            for j in range(sides):
                a, b = mid_ring[j], mid_ring[(j + 1) % sides]
                face((tip, a, b), [planar(v.co, at, (ru, rv)) for v in (tip, a, b)], None, 1)
        else:
            inner_r, inner_d = s.hollow
            # Hollow tip: a rim annulus from the outer end ring to the inner ring, the wall, the floor.
            inner_top = []
            for j, th in enumerate(ths):
                x = self.rim_x(th) - 0.035
                inner_top.append(bm.verts.new(self.axis(x) + self.around(th) * self.radius(x, th) * inner_r))
            for j in range(sides):
                j1 = (j + 1) % sides
                q = (end[j], end[j1], inner_top[j1], inner_top[j])
                face(q, [planar(v.co, at, (ru, rv)) for v in q], None, 1)
            wall = [inner_top]
            for k in range(1, inner_segs + 1):
                row = []
                for j, th in enumerate(ths):
                    x = self.rim_x(th) - 0.035 - (inner_d * k / inner_segs if inner_segs else 0.0)
                    row.append(bm.verts.new(self.axis(x) + self.around(th) * self.radius(x, th) * inner_r
                                            * (1 - 0.25 * k / max(inner_segs, 1))))
                wall.append(row)
            iw = self.inner_wall
            for k in range(inner_segs):
                for j in range(sides):
                    j1 = (j + 1) % sides
                    cj = (j, j + 1)
                    q = (wall[k][j1], wall[k][j], wall[k + 1][j], wall[k + 1][j1])
                    uvs = [(iw[0] + cc / sides * self.inner_c * K, iw[1] + (ss / max(inner_segs, 1)) * inner_d * K)
                           for ss, cc in ((k, cj[1]), (k, cj[0]), (k + 1, cj[0]), (k + 1, cj[1]))]
                    face(q, uvs, None, 1)
            fl = [bm.verts.new(v.co) for v in wall[-1]]
            fc = bm.verts.new(sum((v.co for v in fl), Vector()) / len(fl) + Vector((0.03, 0, 0)))
            ifl = self.inner_floor
            fu, fv = (ifl[0] + ifl[2]) / 2, (ifl[1] + ifl[3]) / 2
            for j in range(sides):
                a, b = fl[j], fl[(j + 1) % sides]
                face((fc, a, b), [planar(v.co, fc.co, (fu, fv)) for v in (fc, a, b)], None, 1)
        # Stubs: tapered tubes from inside the body out along a leaning radial direction, a broken tip.
        for k in stubs:
            sx, sth, lean, L, r0, r1 = s.stubs[k]
            box = self.stub_rects[k]
            d = (self.around(sth) * math.cos(lean) + Vector((-math.sin(lean), 0, 0))).normalized()
            base = self.surf(sx, sth) - d * r0 * 1.2
            ref = Vector((1, 0, 0)) if abs(d.x) < 0.9 else Vector((0, 0, 1))
            e1 = d.cross(ref).normalized()
            e2 = d.cross(e1).normalized()
            sth_ = [2 * math.pi * j / stub_sides for j in range(stub_sides)]
            rows = []
            for g in range(stub_segs + 1):
                t = g / stub_segs
                length = (L + r0 * 1.2) * t
                rr = r0 + (r1 - r0) * t
                ctr = base + d * length + Vector((0, 0, 0.04 * t * t))
                rows.append([bm.verts.new(ctr + (e1 * math.cos(a) + e2 * math.sin(a)) * rr *
                                          (1 + (0.25 * math.sin(3 * a + k) if g == stub_segs else 0.0)) +
                                          (d * 0.04 * math.sin(2 * a + k) if g == stub_segs else Vector()))
                             for a in sth_])
            circ = 2 * math.pi * r0
            tube = L + r0 * 1.2
            for g in range(stub_segs):
                for j in range(stub_sides):
                    j1 = (j + 1) % stub_sides
                    cj = (j, j + 1)
                    q = (rows[g][j], rows[g][j1], rows[g + 1][j1], rows[g + 1][j])
                    uvs = [(box[0] + 0.01 * K + cc / stub_sides * circ * K, box[1] + (0.01 + ss / stub_segs * tube) * K)
                           for ss, cc in ((g, cj[0]), (g, cj[1]), (g + 1, cj[1]), (g + 1, cj[0]))]
                    barks = [(cc / stub_sides * circ / 2, ss / stub_segs * tube / 2 + k * 0.37)
                             for ss, cc in ((g, cj[0]), (g, cj[1]), (g + 1, cj[1]), (g + 1, cj[0]))]
                    face(q, uvs, barks, 0)
            tipr = [bm.verts.new(v.co) for v in rows[-1]]
            tc = bm.verts.new(sum((v.co for v in tipr), Vector()) / len(tipr) + d * 0.02)
            tu = (box[0] + box[2]) / 2
            tv = box[1] + (0.01 + tube + 0.03 + 1.3 * r1) * K
            for j in range(stub_sides):
                a, b = tipr[j], tipr[(j + 1) % stub_sides]
                uvs = []
                for v in (tc, a, b):
                    dd = v.co - tc.co
                    uvs.append((tu + dd.dot(e1) * K, tv + dd.dot(e2) * K))
                face((tc, a, b), uvs, None, 1)
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        me.uv_layers.active = me.uv_layers["UVMap"]
        return scene.link(bpy.data.objects.new(name, me))


def _nodes(m):
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links

    def mix(fac, a, b):
        n = N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        if isinstance(fac, float):
            ins["Factor_Float"].default_value = fac
        else:
            L.new(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                L.new(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    def rng_(src, a, b):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.clamp = True
        L.new(src, r.inputs["Value"])
        return r.outputs["Result"]

    def math_(op, a, b):
        n = N.new("ShaderNodeMath")
        n.operation = op
        for i, x in enumerate((a, b)):
            if isinstance(x, float):
                n.inputs[i].default_value = x
            else:
                L.new(x, n.inputs[i])
        return n.outputs[0]

    return nt, N, L, mix, rng_, math_


def bark_high_material(piece: str, name: str, moss_spec: Moss):
    """log_fallen's high-poly bark: the tileable set on the metric `bark` UV (2 m tile) times the
    trunks' tint, moss where `moss_spec` puts it, a little damp darkening near the ground."""
    m = bpy.data.materials.new(name)
    nt, N, L, mix, rng_, math_ = _nodes(m)
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    uvn = N.new("ShaderNodeUVMap")
    uvn.uv_map = "bark"

    def tex(ch, non_color):
        n = N.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(str(BARK / f"bark.{ch}.png"), check_existing=True)
        n.image.name = f"{piece}_bark_{ch}"
        if non_color:
            n.image.colorspace_settings.name = "Non-Color"
        L.new(uvn.outputs["UV"], n.inputs["Vector"])
        return n

    alb = tex("albedo", False)
    tint = N.new("ShaderNodeMix")
    tint.data_type = "RGBA"
    tint.blend_type = "MULTIPLY"
    ti = {s.identifier: s for s in tint.inputs}
    ti["Factor_Float"].default_value = 1.0
    L.new(alb.outputs["Color"], ti["A_Color"])
    ti["B_Color"].default_value = (*BARK_TINT, 1.0)
    bark_col = next(s for s in tint.outputs if s.identifier == "Result_Color")
    orm = tex("orm", True)
    sep = N.new("ShaderNodeSeparateColor")
    L.new(orm.outputs["Color"], sep.inputs["Color"])
    tc = N.new("ShaderNodeTexCoord")
    geo = N.new("ShaderNodeNewGeometry")
    ms = moss_spec
    # Object coordinates shifted and stretched for the patch noise (x < 1: bands along the grain).
    shift = N.new("ShaderNodeVectorMath")
    shift.operation = "ADD"
    L.new(tc.outputs["Object"], shift.inputs[0])
    shift.inputs[1].default_value = ms.offset
    stretch = N.new("ShaderNodeVectorMath")
    stretch.operation = "MULTIPLY"
    L.new(shift.outputs["Vector"], stretch.inputs[0])
    stretch.inputs[1].default_value = ms.stretch
    mn = N.new("ShaderNodeTexNoise")
    mn.inputs["Scale"].default_value, mn.inputs["Detail"].default_value = ms.scale, 4.0
    L.new(stretch.outputs["Vector"], mn.inputs["Vector"])
    fine = N.new("ShaderNodeTexNoise")
    fine.inputs["Scale"].default_value, fine.inputs["Detail"].default_value = 18.0, 3.0
    L.new(shift.outputs["Vector"], fine.inputs["Vector"])
    facing = N.new("ShaderNodeVectorMath")
    facing.operation = "DOT_PRODUCT"
    L.new(geo.outputs["Normal"], facing.inputs[0])
    facing.inputs[1].default_value = Vector(ms.toward).normalized()
    # Moss on faces turned toward `toward`, patchy by the low noise, ragged by the fine one.
    moss = math_("MULTIPLY", rng_(facing.outputs["Value"], *ms.lo_hi),
                 rng_(math_("ADD", mn.outputs["Fac"], math_("MULTIPLY", fine.outputs["Fac"], 0.35)), *ms.cut))
    if ms.top is not None:
        upz = N.new("ShaderNodeSeparateXYZ")
        L.new(geo.outputs["Normal"], upz.inputs[0])
        cn = N.new("ShaderNodeTexNoise")
        cn.inputs["Scale"].default_value, cn.inputs["Detail"].default_value = ms.cushion_scale, 3.0
        L.new(shift.outputs["Vector"], cn.inputs["Vector"])
        cushion = math_("MULTIPLY", rng_(upz.outputs["Z"], *ms.top),
                        rng_(math_("ADD", cn.outputs["Fac"], math_("MULTIPLY", fine.outputs["Fac"], 0.25)),
                             *ms.cushion_cut))
        moss = math_("MAXIMUM", moss, cushion)
    mcol = mix(rng_(fine.outputs["Fac"], 0.4, 0.65), (*materials.srgb("#4d6a1e")[:3], 1.0),
               (*materials.srgb("#8a9a34")[:3], 1.0))
    # Damp, darker bark low on the log, toward the litter.
    sepz = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sepz.inputs[0])
    damp = rng_(sepz.outputs["Z"], 0.12, -0.05)
    col = mix(math_("MULTIPLY", damp, 0.35), bark_col, (0.05, 0.035, 0.02, 1.0))
    col = mix(moss, col, mcol)
    L.new(col, bsdf.inputs["Base Color"])
    L.new(math_("ADD", math_("MULTIPLY", sep.outputs["Green"], math_("SUBTRACT", 1.0, moss)),
                math_("MULTIPLY", moss, 0.92)), bsdf.inputs["Roughness"])
    nmap = N.new("ShaderNodeNormalMap")
    nmap.uv_map = "bark"
    L.new(tex("normal", True).outputs["Color"], nmap.inputs["Color"])
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.35
    bump.inputs["Distance"].default_value = 0.006
    L.new(math_("MULTIPLY", fine.outputs["Fac"], moss), bump.inputs["Height"])
    L.new(nmap.outputs["Normal"], bump.inputs["Normal"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    L.new(moss, bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


def wood_high_material(name: str, spec: Spec):
    """log_fallen's broken end-grain: pale weathered wood at the break, dark rot deeper in a hollow
    (none on a solid snapped end)."""
    m = bpy.data.materials.new(name)
    nt, N, L, mix, rng_, math_ = _nodes(m)
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = N.new("ShaderNodeTexCoord")
    grain = N.new("ShaderNodeTexWave")
    grain.wave_type = "RINGS"
    grain.inputs["Scale"].default_value = 9.0
    grain.inputs["Distortion"].default_value = 4.0
    L.new(tc.outputs["Object"], grain.inputs["Vector"])
    ramp = N.new("ShaderNodeValToRGB")
    els = ramp.color_ramp.elements
    els[0].position, els[0].color = 0.0, materials.srgb("#6d5c47")
    els[1].position, els[1].color = 1.0, materials.srgb("#9c8a6c")
    L.new(grain.outputs["Fac"], ramp.inputs["Fac"])
    sx = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sx.inputs[0])
    h = spec.half_l
    if spec.hollow is not None:
        # Rot only inside the hollow: 0 at the rim, 1 deep in, 0 at the butt (log_fallen: 1.72..1.5
        # and 1.2..1.3 on a 1.8 m half length).
        rot = math_("MULTIPLY", rng_(sx.outputs["X"], h - 0.08, h - 0.3), rng_(sx.outputs["X"], h - 0.6, h - 0.5))
    else:
        rot = 0.0
    col = mix(rot, ramp.outputs["Color"], materials.srgb("#2b2016"))
    L.new(col, bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.88
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.3
    bump.inputs["Distance"].default_value = 0.004
    L.new(grain.outputs["Fac"], bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Emission Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


def build(piece: str, out: pathlib.Path, generator: str, spec: Spec,
          length: tuple[float, float], top: tuple[float, float]) -> None:
    """log_fallen.py's pipeline for variant `piece` (see the module docstring). `length` bounds the
    LOD0's x extent and `top` its highest point (m)."""
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    noise.seed_set(11)
    log = Log(spec)
    allk = list(range(len(spec.stubs)))
    r0, r1, r2, rh = spec.rings
    inner = (2, 1, 0) if spec.hollow else (0, 0, 0)
    cfgs = ((14, r0, 6, 2, inner[0], allk), (10, r1, 5, 1, inner[1], spec.lod1_stubs), (6, r2, 4, 1, inner[2], []))
    lods = [log.build(f"{piece}_lod{i}", *cfg) for i, cfg in enumerate(cfgs)]
    wood = materials.pbr_material(f"{piece}_wood", materials.srgb("#6a5440"), 0.85)
    for o in lods:
        o.data.materials.append(wood)
        for p in o.data.polygons:
            p.material_index = 0
    for o in lods:
        lod.triangulate(o)
    tris = [geom.tri_count(o) for o in lods]
    texel = uv.measure_texel_density(lods[0], ATLAS)
    zs = [v.co.z for v in lods[0].data.vertices]
    xs_ = [v.co.x for v in lods[0].data.vertices]
    print(f"{piece}: tris per LOD {tris}, texel {texel}, x {min(xs_):.3f}..{max(xs_):.3f}, "
          f"z {min(zs):.3f}..{max(zs):.3f}")
    budgets.check(tris, "prop", piece)
    assert texel["median"] >= 256, texel
    assert length[0] <= max(xs_) - min(xs_) <= length[1] and top[0] <= max(zs) <= top[1], (min(xs_), max(xs_), max(zs))
    # Every face's UVs lie inside one island rectangle in every LOD, so no island runs into another's
    # gutter (log_fallen friction: an odd ring count pushed a coarse LOD off the atlas).
    for o in lods:
        uvd = o.data.uv_layers["UVMap"].data
        for poly in o.data.polygons:
            pts = [uvd[li].uv for li in poly.loop_indices]
            assert any(all(r[0] - 1e-4 <= p.x <= r[2] + 1e-4 and r[1] - 1e-4 <= p.y <= r[3] + 1e-4 for p in pts)
                       for r in log.rects), (o.name, poly.index, [tuple(p) for p in pts])

    albedo = np.zeros((ATLAS * ATLAS, 4), np.float32)
    albedo[:] = (*materials.srgb("#6a5440")[:3], 1.0)
    normal = np.zeros((ATLAS * ATLAS, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.zeros((ATLAS * ATLAS, 4), np.float32)
    orm[:] = (1.0, 0.85, 0.0, 1.0)
    moss_px = np.zeros(ATLAS * ATLAS, np.float32)
    bake_stats = {}
    if not skip_bake:
        high = log.build(f"{piece}_high", 72, rh, 20, 8, 8 if spec.hollow else 0, allk, high=True)
        high.data.materials.append(bark_high_material(piece, f"{piece}_bark_high", spec.moss))
        high.data.materials.append(wood_high_material(f"{piece}_wood_high", spec))
        bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{piece}_world")
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        lod0 = lods[0]
        imgs = {k: bake.new_image(f"{piece}.{k}", ATLAS, non_color=k != "albedo")
                for k in ("albedo", "normal", "ao", "rough", "moss")}
        bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], **kw)
        bake.bake_emit_input([high], lod0, "Emission Color", imgs["moss"], **kw)
        bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], **kw)
        bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], **kw)
        gme = bpy.data.meshes.new("ground_occluder")
        gme.from_pydata([(-4, -3, 0), (4, -3, 0), (4, 3, 0), (-4, 3, 0)], [], [[0, 1, 2, 3]])
        ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
        ground.data.materials.append(bpy.data.materials[f"{piece}_wood_high"])
        bpy.context.scene.world.light_settings.distance = AO_DIST
        # The litter as an occluder only (never a bake source): it darkens the contact line.
        bake.bake_to(lod0, "AO", imgs["ao"], highs=[high], occluders=[ground], **kw)
        bpy.data.objects.remove(ground, do_unlink=True)
        bpy.data.objects.remove(high, do_unlink=True)
        bpy.context.view_layer.update()
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        albedo[:] = px["albedo"]
        normal[:] = px["normal"]
        orm[:, 0] = px["ao"][:, 0]
        orm[:, 1] = px["rough"][:, 0]
        moss_px[:] = px["moss"][:, 0]
        for a in (albedo, normal, orm):
            a[:, 3] = 1.0
        flat = island_texels(lod0, ATLAS).ravel()
        # Texels of faces above the ground (what a viewer sees on level ground), for honest AO stats.
        up = bpy.data.meshes.new("above")
        tmp = bmesh.new()
        tmp.from_mesh(lod0.data)
        tmp.faces.ensure_lookup_table()
        bmesh.ops.delete(tmp, geom=[f for f in tmp.faces if max(v.co.z for v in f.verts) <= 0.0], context="FACES")
        tmp.to_mesh(up)
        tmp.free()
        uo = scene.link(bpy.data.objects.new("above", up))
        vflat = island_texels(uo, ATLAS).ravel()
        bpy.data.objects.remove(uo, do_unlink=True)

        def st(a):
            return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                    "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

        bake_stats["ao"] = st(orm[flat, 0])
        bake_stats["aoAboveGround"] = st(orm[vflat, 0])
        bake_stats["roughness"] = st(orm[flat, 1])
        bake_stats["albedoLuma"] = st(albedo[flat, :3].mean(1))
        bake_stats["mossFraction"] = float((moss_px[vflat] > 0.5).mean())
        bake_stats["normalMissFraction"] = float((normal[flat, 2] < 0.05).mean())

    def shrink(a, n):
        f = ATLAS // n
        return a.reshape(n, f, n, f, 4).mean(axis=(1, 3)).reshape(-1, 4)

    bake.write_png(ATLAS, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    n_small = shrink(normal, SMALL)
    v3 = n_small[:, :3] * 2.0 - 1.0
    v3 /= np.maximum(np.linalg.norm(v3, axis=1, keepdims=True), 1e-6)
    n_small[:, :3] = v3 * 0.5 + 0.5
    n_small[:, 3] = 1.0
    bake.write_png(SMALL, n_small, str(out / f"{piece}.normal.png"))
    bake.write_png(SMALL, shrink(orm, SMALL), str(out / f"{piece}.orm.png"))
    # masks (linear), as log_fallen: R snow capacity 1, G porosity (bark 0.4, moss 0.85), B no night
    # emissive, A = 1 (AO is in the unique ORM.R).
    f = ATLAS // MASK_SIZE
    moss_small = moss_px.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3)).ravel()
    bake.make_masks(MASK_SIZE, 1.0, 0.4 + 0.45 * moss_small, 0.0, None, str(out / f"{piece}.masks.png"))

    # Collider: the body's box (no stubs), so it blocks the walker and clears only the trunk's ground.
    body = log.build("collider_src", 6, 4, 4, 1, 0, [])
    col = collider.collider_from(body, "box", f"{piece}_collider")
    bpy.data.objects.remove(body, do_unlink=True)
    bpy.context.view_layer.update()
    cpts = [col.matrix_world @ v.co for v in col.data.vertices]
    col_reach = max(math.hypot(p.x, p.y) for p in cpts)
    col_top = max(p.z for p in cpts)
    print(f"{piece}: bake {bake_stats}")
    print(f"{piece}: collider reach {col_reach:.4f} top {col_top:.4f}")

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": SMALL, "colorspace": "linear"},
                                     "orm": {"size": SMALL, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, bakeStats=bake_stats, extentZ=[min(zs), max(zs)],
                           buryM=spec.bury, lengthM=[min(xs_), max(xs_)],
                           barkSet={"set": "bark", "tint": BARK_TINT, "tileM": 2.0},
                           colliderReachM=col_reach, colliderTopM=col_top, skipped=skip_bake)
