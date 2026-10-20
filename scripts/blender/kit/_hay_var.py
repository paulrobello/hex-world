"""Hay bale variants (PRD §6 Variety) on the approved `_hay` builder: hay_bale_round_b (a round bale
stood on its flat end, slumped, with loose straw at its foot) and hay_bale_stack_b (three square
bales, two side by side with one laid across them). Refs: assets-source/refs/farm (ref-1 and ref-2:
round bales and a small stack of rectangular bales in a yard scattered with loose straw).

`_hay.py` is reused unchanged:
- stack_b runs `_hay.build_stack` with `_hay.stack_layout` replaced by `stack_b_layout` (a module
  global it looks up at call time) and an explicit `lod0_tris` (a default argument, which a module
  global cannot reach).
- round_b cannot use `_hay.build_round` (it has no hook for the turn, the slump or the straw), so
  `build_round_b` below is that function's orchestration over the same `_hay.round_bale`,
  `straw_material`, `bake_maps` and `finish`. The lathe keeps the approved profile and its analytic
  UVs (four barrel sectors on one island, both caps on one); `_hay.SQUASH` is set to 1 (no resting
  squash) and `_hay.ZC` (fixed when `_hay` loads, so setting SINK/SQUASH would not move it) to
  ZC_B, an axis height that keeps the straw shader's ground dirt (object z < 0.10) off the bale. The
  high keeps its lathe-frame data (the shader works in object space) and carries the turn in its
  matrix; the LODs carry the same turn in their vertex data (the gate reads raw positions).

Round_b's loose straw is an apron round the foot, part of every LOD, on its own six UV islands in
the atlas band above the bale's islands. It drapes on the meadow relief under both of the farm's
round slots (data/tiles/farm.ron, where the hero variant resolver may draw it): its surface sits
above the higher of the two grounds, its outer edge 5 mm under the lower, so it never floats and
never sinks out of sight. The bake runs with every receiver lifted LIFT m, so `_hay.bake_maps`'
ground occluder at z = 0 never cuts the sunk foot or the buried straw edge."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
from mathutils import Matrix, Vector, noise

import _hay as hy
from lib import geom, lod, materials, scene, uv

# What the variants rely on in the approved builder (profile, atlas layout, bake sizes).
assert (hy.R, hy.W, hy.B, hy.DOME, hy.BULGE) == (0.75, 0.6, 0.15, 0.025, 0.02), (hy.R, hy.W, hy.B)
assert (hy.SECTORS, hy.GUTTER, hy.ATLAS, hy.SMALL, hy.MASK_SIZE) == (4, 0.012, 1024, 512, 128)
assert abs(hy.UV_K - (1 - 3 * hy.GUTTER) / (hy.SECTOR_LEN + 2 * hy.RC)) < 1e-12
BARREL_V_TOP = hy.GUTTER + hy.PROFILE_LEN * hy.UV_K  # top of the barrel island (0.568)
CAP_U0 = 2 * hy.GUTTER + hy.SECTOR_LEN * hy.UV_K  # left of the cap island (0.502)
assert BARREL_V_TOP < 0.57 and hy.GUTTER + 2 * hy.RC * hy.UV_K < 0.5, (BARREL_V_TOP,)

# ---------------------------------------------------------------------------------------- stack_b

STACK_B_LOD0 = 330  # five bales took 520 (509); three keep about the same tris per bale


def stack_b_layout(seed: str) -> list[tuple[tuple[float, float, float], Matrix]]:
    """Three bales: two side by side below, one laid across them on top. (size, local-to-world)."""
    rng = random.Random(seed)
    H = 0.37
    out = []
    for y in (-0.24, 0.24):
        size = (rng.uniform(0.93, 0.99), rng.uniform(0.47, 0.49), H)
        yaw = math.radians(rng.uniform(-3.5, 3.5))
        loc = Vector((rng.uniform(-0.04, 0.04), y + rng.uniform(-0.01, 0.01), -0.03))
        out.append((size, Matrix.Translation(loc) @ Matrix.Rotation(yaw, 4, "Z")))
    size = (rng.uniform(0.93, 0.99), rng.uniform(0.47, 0.49), H)
    yaw = math.radians(90.0 + rng.uniform(-7.0, 7.0))
    loc = Vector((rng.uniform(-0.06, 0.06), rng.uniform(-0.03, 0.03), H - 0.06))
    out.append((size, Matrix.Translation(loc) @ Matrix.Rotation(yaw, 4, "Z")))
    return out


def build_stack_b(piece: str, out: pathlib.Path, generator: str) -> None:
    hy.stack_layout = stack_b_layout
    hy.build_stack(piece, out, generator, lod0_tris=STACK_B_LOD0)
    assert hy.stack_layout is stack_b_layout


# ---------------------------------------------------------------------------------------- round_b

ZC_B = 1.0  # lathe axis height in the high's object frame: object z stays in [0.25, 1.75]
FLAT = 0.04  # the foot's flat: lathe x clamped at -W + FLAT (a contact disc about 0.70 m across)
AXIAL = 0.95  # the slumped height, as a fraction of the lathe's
FOOT_BULGE = 0.045  # radius gained at the foot, falling off as (1 - t)^2 up the bale
LEAN = 0.03  # lateral sag of the top (m), along lathe +y
SINK_B = 0.06  # the foot below the pivot's ground
LIFT = 0.5  # bake-time lift over _hay.bake_maps' ground occluder: past its AO distance (0.3 m), so
# only the straw darkens the foot (at 0.10 the plane put a dark ring on the shared barrel island)
assert LIFT > hy.AO_DIST
# The farm's round-bale hero slots (data/tiles/farm.ron: runtime tile (x, z) and yaw): the apron
# drapes on the meadow relief under both.
SLOTS = ((7.5, -0.4, 15.0), (10.4, -1.4, 30.0))
# Apron rings (m from the axis): inside the foot, the mound at the shoulder, the slope, the edge.
R_IN, R_MOUND, R_SLOPE, R_REF = 0.62, 0.80, 0.84, 0.83
T_IN, T_MOUND, T_SLOPE, EDGE_DROP = 0.03, 0.035, 0.018, 0.005
APRON_SEG = 6
APRON_STEPS = {0: 6, 1: 3, 2: 1}  # angular steps per 60-degree segment
APRON_RINGS = {0: ("in", "mound", "slope", "out"), 1: ("in", "mound", "out"), 2: ("mound", "out")}
K_A = 0.9 * hy.UV_K  # apron UV units per metre
APRON_ISL = (2 * math.pi / APRON_SEG * R_REF * K_A, (0.925 - R_IN) * K_A)  # island (w, h) in UV
APRON_V0, APRON_ROW = 0.59, APRON_ISL[1] + 0.014
assert APRON_V0 - BARREL_V_TOP > 0.02 and APRON_V0 + 2 * APRON_ROW + APRON_ISL[1] < 0.99
assert hy.GUTTER + 2 * APRON_ISL[0] + 0.014 < 1.0
ISLAND_APRON = 10  # face 'island' ids 10..15 (the lathe uses 0..5)


HIGH_IN = 0.785  # the apron high's inner edge: the slumped bale's foot radius (0.784 m)


REACH_MAX = 0.895  # hay_bale_round's scatter footprint is 0.9 m: a variant must stay inside it


def r_out(theta: float) -> float:
    """The straw's outer edge radius, irregular round the foot (about 0.86..0.895 m)."""
    return (0.878 + 0.008 * math.sin(3 * theta + 0.7) + 0.006 * math.sin(7 * theta + 2.1)
            + 0.003 * math.sin(13 * theta + 0.4))


# meadow_base.relief (scripts/blender/kit/meadow_base.py), its formula copied: meadow_base.py is a
# monolithic generator and cannot be imported. Checked against kit_scene's relief constants below.
def _dist_to_border(x: float, y: float) -> float:
    apo = 16.0 * math.sqrt(3) / 2
    return min(apo - (x * math.cos(math.radians(-60 * k)) + y * math.sin(math.radians(-60 * k)))
               for k in range(6))


def meadow_relief(x: float, y: float) -> float:
    t = min(max(_dist_to_border(x, y) / 2.0, 0.0), 1.0)
    s = (0.6 * math.sin(0.31 * x + 1.7) * math.cos(0.27 * y - 0.4)
         + 0.4 * math.sin(0.53 * x - 0.45 * y + 0.9))
    return -0.25 * (t * t * (3 - 2 * t)) * (0.5 + 0.5 * s)


assert abs(meadow_relief(-2.0, -2.5) + 0.198) < 5e-4 and abs(meadow_relief(0.0, 0.0) + 0.233) < 5e-4


def slot_ground(bx: float, by: float, slot: tuple) -> float:
    """Meadow relief under piece point (bx, by) (Blender) with the piece in hero slot `slot`: runtime
    local (x, z) = (bx, -by), turned by the slot's yaw (`Quat::from_rotation_y(-yaw)`), then the tile's
    Blender frame (x, -z)."""
    ax, az, yaw = slot
    c, s = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
    lx, lz = bx, -by
    return meadow_relief(ax + lx * c - lz * s, -(az + lx * s + lz * c))


def grounds(bx: float, by: float) -> tuple[float, float]:
    g = [slot_ground(bx, by, s) for s in SLOTS]
    return min(g), max(g)


def apron_z(r: float, theta: float, ring: str) -> float:
    x, y = r * math.cos(theta), r * math.sin(theta)
    lo, hi = grounds(x, y)
    if ring == "out":
        return lo - EDGE_DROP
    return hi + {"in": T_IN, "mound": T_MOUND, "slope": T_SLOPE}[ring]


def ring_r(ring: str, theta: float) -> float:
    return {"in": R_IN, "mound": R_MOUND, "slope": R_SLOPE}.get(ring) or r_out(theta)


def apron(name: str, level: int) -> bpy.types.Object:
    """The straw apron at LOD `level` in the piece frame, with its six UV islands and the face
    layer 'island' (ISLAND_APRON + segment)."""
    rings, steps = APRON_RINGS[level], APRON_STEPS[level]
    bm = bmesh.new()
    isl = bm.faces.layers.int.new("island")
    uvl = bm.loops.layers.uv.new("UVMap")
    dth = 2 * math.pi / APRON_SEG
    for seg in range(APRON_SEG):
        col, row = seg % 2, seg // 2
        u0, v0 = hy.GUTTER + col * (APRON_ISL[0] + 0.014), APRON_V0 + row * APRON_ROW
        grid = []
        for ring in rings:
            row_v = []
            for j in range(steps + 1):
                th = seg * dth + j * dth / steps
                r = ring_r(ring, th)
                v = bm.verts.new((r * math.cos(th), r * math.sin(th), apron_z(r, th, ring)))
                row_v.append((v, (u0 + (j * dth / steps) * R_REF * K_A, v0 + (r - R_IN) * K_A)))
            grid.append(row_v)
        for i in range(len(rings) - 1):
            for j in range(steps):
                quad = [grid[i][j], grid[i][j + 1], grid[i + 1][j + 1], grid[i + 1][j]]
                f = bm.faces.new([q[0] for q in quad])
                f[isl] = ISLAND_APRON + seg
                f.normal_update()
                if f.normal.z < 0:
                    f.normal_flip()
                uvs = {q[0]: q[1] for q in quad}
                for loop in f.loops:
                    loop[uvl].uv = uvs[loop.vert]
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return scene.link(bpy.data.objects.new(name, me))


def apron_high(name: str) -> bpy.types.Object:
    """The bake source: the same draped surface at 1 degree by under 1 cm, with straw lumps. It starts
    at the bale's foot (HIGH_IN), not under it, so the AO bake does not bury the visible foot shoulder
    (a first bake from R_IN drew a dark ring where the shoulder meets the straw)."""
    n_th, rows = 360, 16
    bm = bmesh.new()
    noise.seed_set(7)
    grid = []
    for i in range(rows + 1):
        f = i / rows
        row = []
        for j in range(n_th):
            th = 2 * math.pi * j / n_th
            ro = r_out(th)
            r = HIGH_IN + f * (ro - HIGH_IN)
            if r <= R_MOUND:
                z = apron_z(r, th, "in") + (T_MOUND - T_IN) * (r - R_IN) / (R_MOUND - R_IN)
            elif r <= R_SLOPE:
                a = (r - R_MOUND) / (R_SLOPE - R_MOUND)
                z = (1 - a) * apron_z(r, th, "mound") + a * apron_z(r, th, "slope")
            else:
                a = (r - R_SLOPE) / max(ro - R_SLOPE, 1e-6)
                z = (1 - a) * apron_z(r, th, "slope") + a * apron_z(r, th, "out")
            p = Vector((r * math.cos(th), r * math.sin(th), 0.0))
            z += 0.007 * noise.noise(p * 9.0 + Vector((3.1, -1.7, 0.4))) * min(1.0, (ro - r) / 0.04)
            row.append(bm.verts.new((p.x, p.y, z)))
        grid.append(row)
    for i in range(rows):
        for j in range(n_th):
            k = (j + 1) % n_th
            f = bm.faces.new([grid[i][j], grid[i][k], grid[i + 1][k], grid[i + 1][j]])
            f.normal_update()
            if f.normal.z < 0:
                f.normal_flip()
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return scene.link(bpy.data.objects.new(name, me))


def loose_straw_material(name: str) -> bpy.types.Material:
    """Loose straw on trampled earth: thin strands in four crossing directions over a straw cover that
    thins toward the apron's edge, in `_hay`'s round-bale straw colours."""
    m = bpy.data.materials.new(name)
    n = hy._Nodes(m)
    c = materials.srgb
    obj = n.tc.outputs["Object"]
    px, py, pz = n.xyz(obj)
    lo, hi, light, earth = c("#b98f45"), c("#d3b064"), c("#e6cf8c"), c("#6f5636")
    mottle = n.tex("ShaderNodeTexNoise", obj, 5.0, 3.0)
    col = n.mix(n.rng(mottle.outputs["Fac"], 0.35, 0.65), lo, hi)
    strands = []
    for k, a in enumerate((0.3, 1.1, 1.9, 2.7)):
        ca, sa = math.cos(a), math.sin(a)
        u = n.math("ADD", n.math("MULTIPLY", px, ca), n.math("MULTIPLY", py, sa))
        v = n.math("SUBTRACT", n.math("MULTIPLY", py, ca), n.math("MULTIPLY", px, sa))
        vec = n.combine(n.math("MULTIPLY", u, 6.0), n.math("MULTIPLY", v, 90.0), float(k) * 3.7)
        t = n.tex("ShaderNodeTexNoise", vec, 1.0, 2.0, Roughness=0.5)
        strands.append(n.rng(t.outputs["Fac"], 0.58, 0.68))
    strand = n.math("MAXIMUM", n.math("MAXIMUM", strands[0], strands[1]),
                    n.math("MAXIMUM", strands[2], strands[3]))
    rad = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", px, px), n.math("MULTIPLY", py, py)))
    patch = n.tex("ShaderNodeTexNoise", obj, 7.0, 2.0)
    cover = n.math("MULTIPLY", n.rng(rad, 0.91, 0.80), n.rng(patch.outputs["Fac"], 0.32, 0.5))
    straw = n.math("MAXIMUM", cover, strand)
    col = n.mix(n.math("MULTIPLY", strand, 0.5), col, light)
    col = n.mix(n.math("SUBTRACT", 1.0, straw), col, earth)
    n.link(col, n.bsdf.inputs["Base Color"])
    n.link(n.math("ADD", n.math("MULTIPLY", straw, -0.06), 0.9), n.bsdf.inputs["Roughness"])
    bump = n.N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.4
    bump.inputs["Distance"].default_value = 0.005
    n.link(n.math("ADD", n.math("MULTIPLY", strand, 0.6), n.math("MULTIPLY", cover, 0.4)),
           bump.inputs["Height"])
    n.link(bump.outputs["Normal"], n.bsdf.inputs["Normal"])
    return m


def slump(me: bpy.types.Mesh) -> None:
    """The settled bale in the lathe frame (axis along x at height ZC_B): the foot flattened and
    bulged, the height lowered, the top sagging a little to +y."""
    x0, x1 = -hy.W - hy.DOME, hy.W + hy.DOME
    for v in me.vertices:
        x, y, z = v.co
        t = (x - x0) / (x1 - x0)
        x = max(x, -hy.W + FLAT)
        x = -hy.W + (x + hy.W) * AXIAL
        f = 1 + FOOT_BULGE * (1 - t) ** 2
        v.co = (x, y * f + LEAN * t * t, ZC_B + (z - ZC_B) * f)


FOOT_X = -hy.W + FLAT * AXIAL  # lathe x of the flat foot after the slump


def stand(lift: float = 0.0) -> Matrix:
    """Lathe frame to piece frame: the axis turned upright (lathe x -> z), the foot SINK_B under the
    pivot's ground (plus `lift`)."""
    return (Matrix.Translation((0.0, 0.0, -FOOT_X - SINK_B + lift)) @ Matrix.Rotation(math.radians(-90), 4, "Y")
            @ Matrix.Translation((0.0, 0.0, -ZC_B)))


def build_round_b(piece: str, out: pathlib.Path, generator: str) -> None:
    skip = os.environ.get("HEXW_SKIP_BAKE") == "1"
    hy.SQUASH, hy.ZC = 1.0, ZC_B
    assert hy._rest(0.2, -0.3) == (0.2, ZC_B - 0.3)
    m = stand()
    assert abs((m @ Vector((FOOT_X, 0.0, ZC_B))).z + SINK_B) < 1e-6
    assert (m @ Vector((0.0, 0.0, ZC_B + 1.0)) - Vector((-1.0, 0.0, -FOOT_X - SINK_B))).length < 1e-6
    scene.reset_scene(piece)
    mat = materials.pbr_material(f"{piece}_straw", materials.srgb("#c8a65c"), 0.86)
    high = hy.round_bale(f"{piece}_high", "high", 160, relief=0.012, seed=5)
    slump(high.data)
    high.data.materials.append(hy.straw_material(f"{piece}_high", "round"))
    high.matrix_world = stand(LIFT)
    ahigh = apron_high(f"{piece}_apron_high")
    ahigh.data.materials.append(loose_straw_material(f"{piece}_apron_high"))
    ahigh.matrix_world = Matrix.Translation((0.0, 0.0, LIFT))
    lods = []
    for i, (level, cols) in enumerate((("lod0", 32), ("lod1", 12), ("lod2", 8))):
        b = hy.round_bale(f"{piece}_bale{i}", level, cols)
        slump(b.data)
        b.data.transform(m)
        a = apron(f"{piece}_apron{i}", i)
        o = geom.join([b, a], f"{piece}_lod{i}")
        o.data.materials.clear()
        o.data.materials.append(mat)
        lod.triangulate(o)
        lods.append(o)
    texel = uv.measure_texel_density(lods[0], hy.ATLAS)
    # Receiver: one barrel sector, the top (+x lathe) cap and the six apron islands of LOD0, lifted.
    receiver = lods[0].copy()
    receiver.data = lods[0].data.copy()
    receiver.name = receiver.data.name = f"{piece}_receiver"
    scene.link(receiver)
    bm = bmesh.new()
    bm.from_mesh(receiver.data)
    isl = bm.faces.layers.int["island"]
    keep = {hy.SECTORS - 1, hy.SECTORS + 1, *range(ISLAND_APRON, ISLAND_APRON + APRON_SEG)}
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[isl] not in keep], context="FACES")
    bm.transform(Matrix.Translation((0.0, 0.0, LIFT)))
    bm.to_mesh(receiver.data)
    bm.free()
    zs = [v.co.z for v in receiver.data.vertices]
    assert min(zs) > 0.0, f"the lifted receiver dips under the bake's ground occluder: {min(zs)}"
    stats = hy.bake_maps(piece, out, [high, ahigh], receiver, skip)
    for o in (high, ahigh):
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    for o in lods:
        if "island" in o.data.attributes:
            o.data.attributes.remove(o.data.attributes["island"])
    reach = max(math.hypot(v.co.x, v.co.y) for o in lods for v in o.data.vertices)
    edge_min = min(r_out(2 * math.pi * k / 360) for k in range(360))
    print(f"{piece}: reach {reach:.4f} m, straw edge min {edge_min:.4f} m")
    assert reach <= REACH_MAX and edge_min > R_SLOPE + 0.01, (reach, edge_min)
    floats = contact_floats(lods)
    print(f"{piece}: contact floats per slot (max over LODs, m) {floats}")
    assert max(floats.values()) < 0.02, floats
    hy.finish(piece, out, generator, lods, texel, stats, receiver, skip,
              profile={"radiusM": hy.R, "halfWidthM": hy.W, "shoulderM": hy.B, "sinkM": SINK_B,
                       "flatM": FLAT, "axial": AXIAL, "footBulge": FOOT_BULGE, "leanM": LEAN},
              apron={"rings": [R_IN, R_MOUND, R_SLOPE, "r_out 0.87..0.92"], "slots": [list(s) for s in SLOTS]},
              contactFloatM=floats)
    bpy.data.objects.remove(receiver, do_unlink=True)


def contact_floats(lods: list) -> dict:
    """Per farm slot, the largest height (m) of a contact vertex over the meadow relief: the apron's
    outer edge and the bale's foot, in every LOD (a vertex below the ground does not float)."""
    out = {}
    for slot in SLOTS:
        worst = -1.0
        for o in lods:
            for v in o.data.vertices:
                x, y, z = v.co
                r = math.hypot(x, y)
                edge = r > R_SLOPE + 1e-3 and z < 0.0
                foot = z <= -SINK_B + 1e-4
                if edge or foot:
                    worst = max(worst, z - slot_ground(x, y, slot))
        out[f"{slot[0]},{slot[1]}"] = round(worst, 4)
    return out
