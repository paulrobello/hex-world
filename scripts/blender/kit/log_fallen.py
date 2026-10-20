"""log_fallen: a fallen spruce log on the forest floor, 3.6 m long and about 0.45 m thick, a broken
hollow tip, a flared root end with snapped root stubs, two broken branch stubs, the trees' bark and
moss over its top (refs: assets-source/refs/forest, "fallen log"). Scripted route (PRD §11.1), prop
kind (LOD0 <= 1,500 tris). Usage: make kit-build PIECE=log_fallen.

The pivot is on the ground at the log's middle. The log lies along Blender x, sunk BURY below the
pivot: the forest floor relief drops at most 0.127 m from a log's centre to its ends (20,000 seeded
placements of forest_base.relief), so the log never floats where the floor dips.

One parametric surface (`Shape`) is sampled at every density: the high poly (72 sides) for the bake,
LOD0/1/2 at 14/10/6 sides, so all LODs share one metric UV layout and sample LOD0's atlas exactly,
with no decimation (pitfalls: "A lathed prop can share UV islands"). The atlas is laid out in metres
at DENSITY px/m: the body as two half-length cylinder islands (u around from the hidden underside, v
along), the end caps, the hollow tip's rim, wall and floor, and every stub in a strip below them.

Bark matches the spruces: the high poly samples the tileable `bark` set on metric UVs (2 m tile) times
the trunks' manifest tint, so the log reads as one of the forest's trees, and a unique bake carries
what a tileable slot cannot (slots cannot blend): moss creeping over the top, pale broken end-grain,
and the ground-contact AO. Baked selected-to-active: albedo through EMIT, tangent normal (the set's
bark relief plus the moss bump), AO with a ground plane as occluder into ORM.R, roughness into ORM.G,
the moss mask into masks.G. Normal and ORM ship at 512² (box-filtered), as rock_small_b does."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector, noise

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv  # noqa: E402
from _shared import island_texels  # noqa: E402

PIECE, ATLAS, SMALL, MASK_SIZE = "log_fallen", 1024, 512, 128
REPO = pathlib.Path(__file__).resolve().parents[3]
BARK = REPO / "assets/kit/p1/materials/bark"
BARK_TINT = (0.92, 0.8, 0.72)  # tree_forest_bark's manifest colour
HALF_L, BURY = 1.8, 0.13
R0 = 0.24  # mean outer radius at the middle (m): 0.48 m thick, the brief's upper end
DENSITY = 276.0
K = DENSITY / ATLAS  # UV units per metre
C_REF = 2 * math.pi * R0 * 1.06  # circumference the body islands are laid out at (m)
CAGE, MAX_RAY, MARGIN, SAMPLES, AO_DIST = 0.05, 0.14, 8, 24, 0.3
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
noise.seed_set(11)
OFF = Vector((13.1, -7.7, 4.2))


def radius(x, th):
    """Outer radius (m) at x along the log and angle th around it: a gentle taper, the root flare
    over the butt's last 0.5 m, and a lumpy cross-section."""
    r = R0 - 0.016 * (x / HALF_L)
    f = min(max((-1.3 - x) / 0.5, 0.0), 1.0)
    r += 0.075 * f * f
    return r * (1.0 + 0.07 * noise.noise(Vector((x * 1.1, math.cos(th) * 0.9, math.sin(th) * 0.9)) + OFF))


def axis(x):
    """Centre line: sunk BURY below the pivot under the mean radius, with a slight bend in plan."""
    return Vector((x, 0.05 * math.sin(x * 0.85), R0 - BURY))


def around(th):
    return Vector((0.0, math.cos(th), math.sin(th)))


def rim_x(th):
    """The broken tip's jagged end (x), low-frequency so 14 samples hold it."""
    return HALF_L + 0.07 * math.sin(3 * th + 1.0) + 0.045 * math.sin(5 * th + 2.3)


def surf(x, th):
    return axis(x) + around(th) * radius(x, th)


# Stubs: (x on the body, angle around, lean toward -x in rad, length, base and tip radius, uv rect).
# Four snapped roots at the butt, two broken branches along the top.
STUBS = [(-1.62, math.radians(75), 0.45, 0.5, 0.085, 0.045), (-1.66, math.radians(125), 0.35, 0.42, 0.08, 0.04),
         (-1.6, math.radians(30), 0.55, 0.36, 0.075, 0.04), (-1.68, math.radians(165), 0.3, 0.3, 0.07, 0.035),
         (-0.45, math.radians(80), -0.5, 0.28, 0.05, 0.03), (0.75, math.radians(115), -0.6, 0.24, 0.045, 0.028)]
INNER_R, INNER_D = 0.6, 0.38  # hollow tip: inner radius fraction and depth (m)


ATLAS_M = ATLAS / DENSITY  # the atlas side in metres
GAP = 0.06  # metres between islands: 18 px at DENSITY, at least 2 x the bake margin + 2 px


def rect(u_m, v_m, w_m, h_m):
    """An atlas rectangle at (u_m, v_m) of w_m x h_m, all in metres; asserts it stays inside."""
    r = (u_m * K, v_m * K, (u_m + w_m) * K, (v_m + h_m) * K)
    assert 0.0 <= r[0] and r[2] <= 1.0 and 0.0 <= r[1] and r[3] <= 1.0, r
    return r


def stub_box(k):
    """(width, height) in metres of stub k's island: the tube unrolled, then its broken tip cap."""
    _, _, _, L, r0, r1 = STUBS[k]
    return max(2 * math.pi * r0, 2.6 * r1) + 0.02, 0.01 + L + 1.2 * r0 + 0.03 + 2.6 * r1 + 0.01


BODY_H = 1.87  # each half island holds at most (max rim_x + HALF_L) / 2 = 1.858 m
BODY_V = ATLAS_M - GAP - BODY_H
BODY_A = rect(GAP, BODY_V, C_REF, BODY_H)
BODY_B = rect(2 * GAP + C_REF, BODY_V, C_REF, BODY_H)
# Row A (bottom): the three largest root stubs, the butt cap, the hollow tip's rim.
_rects, _u = {}, GAP
for _k in (0, 1, 2):
    _w, _h = stub_box(_k)
    _rects[_k] = rect(_u, GAP, _w, _h)
    _u += _w + GAP
CAP_W = 2 * (R0 + 0.091) * 1.08  # the flared butt (radius + flare, lumpy section)
RIM_W = 2 * R0 * 1.1
CAP_BUTT = rect(_u, GAP, CAP_W, CAP_W)
_u += CAP_W + GAP
RIM = rect(_u, GAP, RIM_W, RIM_W)
ROW_A = GAP + max(stub_box(k)[1] for k in (0, 1, 2))
# Row B: the other stubs, the hollow's wall and floor.
_u = GAP
for _k in (3, 4, 5):
    _w, _h = stub_box(_k)
    _rects[_k] = rect(_u, ROW_A + GAP, _w, _h)
    _u += _w + GAP
INNER_C = 2 * math.pi * R0 * INNER_R  # the hollow's circumference at its mouth (m)
INNER_WALL = rect(_u, ROW_A + GAP, INNER_C, INNER_D + 0.04)
_u += INNER_C + GAP
INNER_FLOOR = rect(_u, ROW_A + GAP, 0.3, 0.3)
STUB_RECTS = [_rects[k] for k in range(len(STUBS))]
_all = [BODY_A, BODY_B, CAP_BUTT, RIM, INNER_WALL, INNER_FLOOR, *STUB_RECTS]
for _i, _a in enumerate(_all):
    for _b in _all[_i + 1:]:
        g = GAP * K * 0.99
        assert (_a[2] + g <= _b[0] or _b[2] + g <= _a[0] or _a[3] + g <= _b[1] or _b[3] + g <= _a[1]), \
            ("uv rects closer than GAP", _a, _b)


def build(name, sides, rings, stub_sides, stub_segs, inner_segs, stubs, high=False):
    """One mesh of the log. UVMap: the atlas; on the high poly also `bark` (metric, for the set) and
    material index 0 bark, 1 end-grain wood."""
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

    ths = [-math.pi / 2 + 2 * math.pi * j / sides for j in range(sides)]
    # Body: rings from the butt (x = -HALF_L) to the jagged rim, column j ending at rim_x(th_j).
    grid, xs = [], []
    for i in range(rings + 1):
        t = i / rings
        row, xrow = [], []
        for th in ths:
            x = -HALF_L + t * (rim_x(th) + HALF_L)
            row.append(bm.verts.new(surf(x, th)))
            xrow.append(x)
        grid.append(row)
        xs.append(xrow)
    assert rings % 2 == 0, rings  # each body island holds exactly half the length
    mid = rings // 2
    for i in range(rings):
        box = BODY_A if i < mid else BODY_B
        for j in range(sides):
            j1 = (j + 1) % sides
            cols = (j, j + 1)
            quad = (grid[i][j], grid[i][j1], grid[i + 1][j1], grid[i + 1][j])
            uvs, barks = [], []
            for ii, jj, cj in ((i, j, cols[0]), (i, j1, cols[1]), (i + 1, j1, cols[1]), (i + 1, j, cols[0])):
                x0 = xs[mid][jj] if i >= mid else -HALF_L
                uvs.append((box[0] + cj / sides * C_REF * K, box[1] + (xs[ii][jj] - x0) * K))
                barks.append((cj / sides * C_REF / 2.0, (xs[ii][jj] + HALF_L) / 2.0))
            face(quad, uvs, barks, 0)
    # Butt cap: a fan to a slightly domed centre, planar (y, z) UVs.
    c = bm.verts.new(axis(-HALF_L) + Vector((-0.025, 0, 0)))
    ring = [bm.verts.new(v.co) for v in grid[0]]
    cu, cv = (CAP_BUTT[0] + CAP_BUTT[2]) / 2, (CAP_BUTT[1] + CAP_BUTT[3]) / 2

    def planar(p, centre, box_c):
        d = p - centre
        return (box_c[0] + d.y * K, box_c[1] + d.z * K)

    ac = axis(-HALF_L)
    for j in range(sides):
        a, b = ring[(j + 1) % sides], ring[j]
        face((c, a, b), [planar(v.co, ac, (cu, cv)) for v in (c, a, b)], None, 1)
    # Hollow tip: a rim annulus from the outer end ring to the inner ring, the inner wall, the floor.
    ru, rv = (RIM[0] + RIM[2]) / 2, (RIM[1] + RIM[3]) / 2
    end = [bm.verts.new(v.co) for v in grid[-1]]
    inner_top = []
    for j, th in enumerate(ths):
        x = rim_x(th) - 0.035
        inner_top.append(bm.verts.new(axis(x) + around(th) * radius(x, th) * INNER_R))
    at = axis(HALF_L)
    for j in range(sides):
        j1 = (j + 1) % sides
        q = (end[j], end[j1], inner_top[j1], inner_top[j])
        face(q, [planar(v.co, at, (ru, rv)) for v in q], None, 1)
    wall = [inner_top]
    for s in range(1, inner_segs + 1):
        row = []
        for j, th in enumerate(ths):
            x = rim_x(th) - 0.035 - (INNER_D * s / inner_segs if inner_segs else 0.0)
            row.append(bm.verts.new(axis(x) + around(th) * radius(x, th) * INNER_R * (1 - 0.25 * s / max(inner_segs, 1))))
        wall.append(row)
    for s in range(inner_segs):
        for j in range(sides):
            j1 = (j + 1) % sides
            cj = (j, j + 1)
            q = (wall[s][j1], wall[s][j], wall[s + 1][j], wall[s + 1][j1])
            uvs = [(INNER_WALL[0] + cc / sides * INNER_C * K, INNER_WALL[1] + (ss / max(inner_segs, 1)) * INNER_D * K)
                   for ss, cc in ((s, cj[1]), (s, cj[0]), (s + 1, cj[0]), (s + 1, cj[1]))]
            face(q, uvs, None, 1)
    fl = [bm.verts.new(v.co) for v in wall[-1]]
    fc = bm.verts.new(sum((v.co for v in fl), Vector()) / len(fl) + Vector((0.03, 0, 0)))
    fu, fv = (INNER_FLOOR[0] + INNER_FLOOR[2]) / 2, (INNER_FLOOR[1] + INNER_FLOOR[3]) / 2
    for j in range(sides):
        a, b = fl[j], fl[(j + 1) % sides]
        face((fc, a, b), [planar(v.co, fc.co, (fu, fv)) for v in (fc, a, b)], None, 1)
    # Stubs: tapered tubes from inside the body out along a leaning radial direction, a broken tip.
    for k in stubs:
        sx, sth, lean, L, r0, r1 = STUBS[k]
        box = STUB_RECTS[k]
        d = (around(sth) * math.cos(lean) + Vector((-math.sin(lean), 0, 0))).normalized()
        base = surf(sx, sth) - d * r0 * 1.2
        ref = Vector((1, 0, 0)) if abs(d.x) < 0.9 else Vector((0, 0, 1))
        e1 = d.cross(ref).normalized()
        e2 = d.cross(e1).normalized()
        sth_ = [2 * math.pi * j / stub_sides for j in range(stub_sides)]
        rows = []
        for s in range(stub_segs + 1):
            t = s / stub_segs
            length = (L + r0 * 1.2) * t
            rr = r0 + (r1 - r0) * t
            ctr = base + d * length + Vector((0, 0, 0.04 * t * t))
            rows.append([bm.verts.new(ctr + (e1 * math.cos(a) + e2 * math.sin(a)) * rr *
                                      (1 + (0.25 * math.sin(3 * a + k) if s == stub_segs else 0.0)) +
                                      (d * 0.04 * math.sin(2 * a + k) if s == stub_segs else Vector()))
                         for a in sth_])
        circ = 2 * math.pi * r0
        tube = L + r0 * 1.2
        for s in range(stub_segs):
            for j in range(stub_sides):
                j1 = (j + 1) % stub_sides
                cj = (j, j + 1)
                q = (rows[s][j], rows[s][j1], rows[s + 1][j1], rows[s + 1][j])
                uvs = [(box[0] + 0.01 * K + cc / stub_sides * circ * K, box[1] + (0.01 + ss / stub_segs * tube) * K)
                       for ss, cc in ((s, cj[0]), (s, cj[1]), (s + 1, cj[1]), (s + 1, cj[0]))]
                barks = [(cc / stub_sides * circ / 2, ss / stub_segs * tube / 2 + k * 0.37)
                         for ss, cc in ((s, cj[0]), (s, cj[1]), (s + 1, cj[1]), (s + 1, cj[0]))]
                face(q, uvs, barks, 0)
        tip = [bm.verts.new(v.co) for v in rows[-1]]
        tc = bm.verts.new(sum((v.co for v in tip), Vector()) / len(tip) + d * 0.02)
        tu = (box[0] + box[2]) / 2
        tv = box[1] + (0.01 + tube + 0.03 + 1.3 * r1) * K
        for j in range(stub_sides):
            a, b = tip[j], tip[(j + 1) % stub_sides]
            uvs = []
            for v in (tc, a, b):
                dd = v.co - tc.co
                uvs.append((tu + dd.dot(e1) * K, tv + dd.dot(e2) * K))
            face((tc, a, b), uvs, None, 1)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.uv_layers.active = me.uv_layers["UVMap"]
    o = scene.link(bpy.data.objects.new(name, me))
    return o


def bark_high_material(name):
    """The high poly's bark: the tileable set on the metric `bark` UV (2 m tile) times the trunks'
    tint, moss on the up-facing top broken by noise, a little damp darkening near the ground."""
    m = bpy.data.materials.new(name)
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    uvn = N.new("ShaderNodeUVMap")
    uvn.uv_map = "bark"

    def tex(ch, non_color):
        n = N.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(str(BARK / f"bark.{ch}.png"), check_existing=True)
        n.image.name = f"{PIECE}_bark_{ch}"
        if non_color:
            n.image.colorspace_settings.name = "Non-Color"
        L.new(uvn.outputs["UV"], n.inputs["Vector"])
        return n

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
    upz = N.new("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], upz.inputs[0])
    mn = N.new("ShaderNodeTexNoise")
    mn.inputs["Scale"].default_value, mn.inputs["Detail"].default_value = 2.2, 4.0
    L.new(tc.outputs["Object"], mn.inputs["Vector"])
    fine = N.new("ShaderNodeTexNoise")
    fine.inputs["Scale"].default_value, fine.inputs["Detail"].default_value = 18.0, 3.0
    L.new(tc.outputs["Object"], fine.inputs["Vector"])
    # Moss: on faces turned up (n.z 0.15..0.6), patchy by a low noise, ragged by a fine one.
    moss = math_("MULTIPLY", rng_(upz.outputs["Z"], 0.15, 0.6),
                 rng_(math_("ADD", mn.outputs["Fac"], math_("MULTIPLY", fine.outputs["Fac"], 0.35)), 0.58, 0.72))
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


def wood_high_material(name):
    """Broken end-grain: pale weathered wood at the break, dark rot deeper in the hollow."""
    m = bpy.data.materials.new(name)
    nt = materials.ensure_nodes(m)
    N, L = nt.nodes, nt.links
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
    # Rot only inside the tip's hollow (x 1.38..1.72): 0 at the rim, 1 deep in, 0 at the butt.
    deep = N.new("ShaderNodeMapRange")
    deep.inputs["From Min"].default_value, deep.inputs["From Max"].default_value = 1.72, 1.5
    deep.clamp = True
    L.new(sx.outputs["X"], deep.inputs["Value"])
    tipward = N.new("ShaderNodeMapRange")
    tipward.inputs["From Min"].default_value, tipward.inputs["From Max"].default_value = 1.2, 1.3
    tipward.clamp = True
    L.new(sx.outputs["X"], tipward.inputs["Value"])
    rot = N.new("ShaderNodeMath")
    rot.operation = "MULTIPLY"
    L.new(deep.outputs["Result"], rot.inputs[0])
    L.new(tipward.outputs["Result"], rot.inputs[1])
    mx = N.new("ShaderNodeMix")
    mx.data_type = "RGBA"
    ins = {s.identifier: s for s in mx.inputs}
    L.new(rot.outputs[0], ins["Factor_Float"])
    L.new(ramp.outputs["Color"], ins["A_Color"])
    ins["B_Color"].default_value = materials.srgb("#2b2016")
    L.new(next(s for s in mx.outputs if s.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.88
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.3
    bump.inputs["Distance"].default_value = 0.004
    L.new(grain.outputs["Fac"], bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Emission Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


ALL = list(range(len(STUBS)))
LODS = ((14, 18, 6, 2, 2, ALL), (10, 8, 5, 1, 1, ALL[:4]), (6, 4, 4, 1, 0, []))
lods = [build(f"{PIECE}_lod{i}", *cfg) for i, cfg in enumerate(LODS)]
wood = materials.pbr_material(f"{PIECE}_wood", materials.srgb("#6a5440"), 0.85)
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
print(f"{PIECE}: tris per LOD {tris}, texel {texel}, x {min(xs_):.3f}..{max(xs_):.3f}, z {min(zs):.3f}..{max(zs):.3f}")
budgets.check(tris, "prop", PIECE)
assert texel["median"] >= 256, texel
# Every face's UVs lie inside one island rectangle, so no island runs into another's gutter.
for o in lods:
    uvd = o.data.uv_layers["UVMap"].data
    for poly in o.data.polygons:
        pts = [uvd[li].uv for li in poly.loop_indices]
        assert any(all(r[0] - 1e-4 <= p.x <= r[2] + 1e-4 and r[1] - 1e-4 <= p.y <= r[3] + 1e-4 for p in pts)
                   for r in _all), (o.name, poly.index, [tuple(p) for p in pts])

albedo = np.zeros((ATLAS * ATLAS, 4), np.float32)
albedo[:] = (*materials.srgb("#6a5440")[:3], 1.0)
normal = np.zeros((ATLAS * ATLAS, 4), np.float32)
normal[:] = (0.5, 0.5, 1.0, 1.0)
orm = np.zeros((ATLAS * ATLAS, 4), np.float32)
orm[:] = (1.0, 0.85, 0.0, 1.0)
moss_px = np.zeros(ATLAS * ATLAS, np.float32)
bake_stats = {}
if not skip_bake:
    high = build(f"{PIECE}_high", 72, 140, 20, 8, 8, ALL, high=True)
    high.data.materials.append(bark_high_material(f"{PIECE}_bark_high"))
    high.data.materials.append(wood_high_material(f"{PIECE}_wood_high"))
    bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{PIECE}_world")
    bake.setup_cycles(samples=SAMPLES)
    kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
    lod0 = lods[0]
    imgs = {k: bake.new_image(f"{PIECE}.{k}", ATLAS, non_color=k != "albedo")
            for k in ("albedo", "normal", "ao", "rough", "moss")}
    bake.bake_emit_input([high], lod0, "Base Color", imgs["albedo"], **kw)
    bake.bake_emit_input([high], lod0, "Emission Color", imgs["moss"], **kw)
    bake.bake_to(lod0, "NORMAL", imgs["normal"], highs=[high], **kw)
    bake.bake_to(lod0, "ROUGHNESS", imgs["rough"], highs=[high], **kw)
    gme = bpy.data.meshes.new("ground_occluder")
    gme.from_pydata([(-4, -3, 0), (4, -3, 0), (4, 3, 0), (-4, 3, 0)], [], [[0, 1, 2, 3]])
    ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
    ground.data.materials.append(bpy.data.materials[f"{PIECE}_wood_high"])
    bpy.context.scene.world.light_settings.distance = AO_DIST
    # The litter as an occluder only (never a bake source): it darkens the contact line.
    bake.bake_to(lod0, "AO", imgs["ao"], highs=[high], occluders=[ground], **kw)
    bpy.data.objects.remove(ground, do_unlink=True)
    bpy.data.objects.remove(high, do_unlink=True)
    bpy.context.view_layer.update()
    px = {}
    for k, im in imgs.items():
        p = bake.save_png(im, str(out / f"{PIECE}.{k}.tmp.png"))
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


bake.write_png(ATLAS, albedo, str(out / f"{PIECE}.albedo.png"), non_color=False)
n_small = shrink(normal, SMALL)
v3 = n_small[:, :3] * 2.0 - 1.0
v3 /= np.maximum(np.linalg.norm(v3, axis=1, keepdims=True), 1e-6)
n_small[:, :3] = v3 * 0.5 + 0.5
n_small[:, 3] = 1.0
bake.write_png(SMALL, n_small, str(out / f"{PIECE}.normal.png"))
bake.write_png(SMALL, shrink(orm, SMALL), str(out / f"{PIECE}.orm.png"))
# masks (linear): R snow capacity 1 (the top holds snow; the shader adds up-facing coverage), G
# porosity (bark 0.4, moss 0.85), B no night emissive, A = 1 (AO is in the unique ORM.R).
f = ATLAS // MASK_SIZE
moss_small = moss_px.reshape(MASK_SIZE, f, MASK_SIZE, f).mean(axis=(1, 3)).ravel()
bake.make_masks(MASK_SIZE, 1.0, 0.4 + 0.45 * moss_small, 0.0, None, str(out / f"{PIECE}.masks.png"))

# Collider: the body's box (no stubs), so it blocks the walker and clears only the trunk's ground.
body = build("collider_src", 6, 4, 4, 1, 0, [])
col = collider.collider_from(body, "box", f"{PIECE}_collider")
bpy.data.objects.remove(body, do_unlink=True)
bpy.context.view_layer.update()
print(f"{PIECE}: bake {bake_stats}")

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                       cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": SMALL, "colorspace": "linear"},
                                 "orm": {"size": SMALL, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, bakeStats=bake_stats, extentZ=[min(zs), max(zs)],
                       buryM=BURY, lengthM=[min(xs_), max(xs_)], barkSet={"set": "bark", "tint": BARK_TINT, "tileM": 2.0},
                       skipped=skip_bake)
