"""house_pad: the house_cluster centre piece (PRD §5.4 tile 3, §6 "building pads"). Scripted route.

The ground treatment under and between the three houses (data/tiles/house_cluster.ron): a level
gravel yard framed by the houses, a stone garden edging where the yard meets the lawn, stone
doorsteps, garden hedges, and an asphalt driveway that leaves the yard at the car destination node
"houses" (runtime (2, 0)) and meets the edge-0 road stub at x = +6 on the road contract.

Frame: Blender (x, y, z) exports as runtime (x, z, -y), so a runtime (x, z) placement is Blender
(x, -z). The layout below is symmetric about y = 0, so the sign only matters for documentation.

Joint contract (kit_gate `lane_joint_errors`): the driveway is `_road.build` swept from x = 2 to
x = +6 with the shared, LOD-invariant PROFILE, plus `_road.paint` clipped at x = 6, so the x = 6
section is the edge piece's section in every LOD. The piece declares `socket-lane-0` (edge 0,
offset 0, width 6), which makes the gate check that joint instead of the straight centre's ±6
pair. No other vertex may sit on the planes x = ±6 (asserted below).

Pad levels: gravel top 5 cm, 1 cm under the road verge top (6 cm) and the asphalt (8-11 cm), so
nothing coincides. Every edge drops to -0.32 m, below the meadow cap's deepest relief (-0.25 m), so
no rim floats over a cap dip at any rotation of the base."""
import math
import os
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import bake, collider, export, geom, hexgeo, materials, scene, uv  # noqa: E402

import _road  # noqa: E402

PIECE = "house_pad"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)

GRAVEL, EDGING, HEDGE, STONE = (f"{PIECE}_gravel", f"{PIECE}_edging", f"{PIECE}_hedge", f"{PIECE}_stone")
SLOTS = (*_road.SLOTS, GRAVEL, EDGING, HEDGE, STONE)
IDX = {n: i for i, n in enumerate(SLOTS)}

DRIVE_X0 = 2.0  # driveway end = the "houses" car destination node at runtime (2, 0)
ROAD_SEGMENTS = (2, 1, 1)
# x0, x1, half-width (Blender y): the house fronts sit on x0 and y = ±4.8, and the yard stops where
# the driveway starts, so no gravel face lies buried under the road (planar UVs would share texels).
YARD = (-5.0, DRIVE_X0, 4.8)
YARD_R = 0.5
YARD_Z, EDGE_TOP, EDGE_W, DROP = 0.05, 0.09, 0.12, -0.32
CORNER_SEGS = (4, 2, 1)
# Doorsteps in front of each house door (house fronts: house_cottage at (-0.5, ±7) facing the yard
# across y = ±4.8, house_twostorey at (-7.5, 0) facing +x across x = -5). (cx, cy, sx, sy).
DOORSTEPS = ((-0.5, 4.35, 1.4, 0.9), (-0.5, -4.35, 1.4, 0.9), (-4.55, 0.0, 0.9, 1.6))
STEP_TOP = 0.08
# Garden hedges (x0, y0, x1, y1): beside the driveway, behind the cottages, and either side of the
# two-storey's back garden. Lengthwise vertex stations avoid x = ±6 (joint planes).
HEDGES = ((3.1, 5.45, 7.6, 5.45), (3.1, -5.45, 7.6, -5.45),
          (-4.0, 10.6, 3.0, 10.6), (-4.0, -10.6, 3.0, -10.6),
          (-11.6, 5.0, -5.7, 5.0), (-11.6, -5.0, -5.7, -5.0))
HEDGE_W, HEDGE_H = 0.65, 1.05
HEDGE_STEP = (0.5, 1.5, None)  # lengthwise station spacing per LOD (None: one span)
HEDGE_RING = (10, 6, 4)  # cross-section vertices per LOD
MASK_SIZE = 512  # one masks bake over the 2R square: 16 px/m, as the road pieces
SAMPLES = 64


def new_obj(name, bm):
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for n in SLOTS:
        obj.data.materials.append(MATS[n])
    return obj


def rounded_rect(x0, x1, hw, r, segs):
    """CCW outline of the yard, starting on the +x side at y = 0 so the +x run (under the
    driveway) is contiguous at both ends of the list."""
    cs = [(x1 - r, hw - r, 0.0), (x0 + r, hw - r, 90.0), (x0 + r, -hw + r, 180.0), (x1 - r, -hw + r, 270.0)]
    pts = [(x1, 0.0)]
    for cx, cy, a0 in cs:
        for s in range(segs + 1):
            a = math.radians(a0 + 90.0 * s / segs)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def outward(pts, i):
    """Outward unit normal at vertex i of a CCW polyline (average of the adjacent edge normals)."""
    n = len(pts)
    ax, ay = pts[i - 1] if i > 0 else pts[i]
    bx, by = pts[(i + 1) % n] if i < n - 1 else pts[i]
    dx, dy = bx - ax, by - ay
    ln = math.hypot(dx, dy)
    return dy / ln, -dx / ln


def yard(name, segs):
    pts = rounded_rect(*YARD, YARD_R, segs)
    bm = bmesh.new()
    top = [bm.verts.new((x, y, YARD_Z)) for x, y in pts]
    bot = [bm.verts.new((x, y, DROP)) for x, y in pts]
    bm.faces.new(top).material_index = IDX[GRAVEL]
    n = len(pts)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((top[i], bot[i], bot[j], top[j])).material_index = IDX[GRAVEL]
    return new_obj(name, bm)


def edging(name, segs):
    """Stone edging strip along the yard outline wherever it meets lawn (x < DRIVE_X0; the +x side
    meets the driveway end). Inner face down to the gravel, outer face down to DROP."""
    pts = rounded_rect(*YARD, YARD_R, segs)
    run = [i for i, (x, _) in enumerate(pts) if x < DRIVE_X0 - 1e-6]
    bm = bmesh.new()
    rows = []
    for i in run:
        x, y = pts[i]
        nx, ny = outward(pts, i)
        ox, oy = x + nx * EDGE_W, y + ny * EDGE_W
        rows.append([bm.verts.new(v) for v in ((x, y, YARD_Z), (x, y, EDGE_TOP), (ox, oy, EDGE_TOP), (ox, oy, DROP))])
    for a, b in zip(rows, rows[1:]):
        for k in range(3):
            bm.faces.new((a[k], b[k], b[k + 1], a[k + 1])).material_index = IDX[EDGING]
    for r, flip in ((rows[0], True), (rows[-1], False)):
        f = bm.faces.new(r if flip else list(reversed(r)))
        f.material_index = IDX[EDGING]
    bm.normal_update()
    return new_obj(name, bm)


def box(bm, cx, cy, sx, sy, z0, z1, mat):
    hx, hy = sx / 2, sy / 2
    v = [bm.verts.new((cx + dx, cy + dy, z)) for z in (z0, z1) for dx, dy in ((-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy))]
    for f in ((4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)):
        bm.faces.new([v[i] for i in f]).material_index = mat


def doorsteps(name):
    bm = bmesh.new()
    for cx, cy, sx, sy in DOORSTEPS:
        box(bm, cx, cy, sx, sy, DROP, STEP_TOP, IDX[STONE])
    return new_obj(name, bm)


def hedge_section(m, rng):
    """Closed cross-section (lateral offset, height) of a clipped hedge: straight sides that bulge
    slightly, a rounded crown, the base buried at DROP. m vertices, CCW seen from the +length end."""
    pts = []
    for k in range(m):
        a = 2 * math.pi * k / m - math.pi / 2  # k = 0 at the bottom centre
        sx, sz = math.cos(a), math.sin(a)
        t = HEDGE_W / 2 * math.copysign(abs(sx) ** 0.45, sx)
        z = (HEDGE_H - DROP) / 2 * math.copysign(abs(sz) ** 0.6, sz) + (HEDGE_H + DROP) / 2
        if rng is not None and z > 0.0:
            t *= 1.0 + rng.uniform(-0.08, 0.1)
            z += rng.uniform(-0.05, 0.04) * (z / HEDGE_H)
        pts.append((t, max(z, DROP)))
    return pts


def hedges(name, lod):
    bm = bmesh.new()
    m = HEDGE_RING[lod]
    for h, (x0, y0, x1, y1) in enumerate(HEDGES):
        rng = random.Random(f"{PIECE}-hedge-{h}") if lod == 0 else None
        ln = math.hypot(x1 - x0, y1 - y0)
        dx, dy = (x1 - x0) / ln, (y1 - y0) / ln
        nx, ny = -dy, dx
        step = HEDGE_STEP[lod]
        n = max(1, round(ln / step)) if step else 1
        rows = []
        for s in range(n + 1):
            f = s / n
            # Ends pulled in a little so the hedge has rounded, clipped ends.
            sec = hedge_section(m, rng)
            px, py = x0 + dx * ln * f, y0 + dy * ln * f
            rows.append([bm.verts.new((px + nx * t, py + ny * t, z)) for t, z in sec])
        for a, b in zip(rows, rows[1:]):
            for k in range(m):
                j = (k + 1) % m
                bm.faces.new((a[k], a[j], b[j], b[k])).material_index = IDX[HEDGE]
        bm.faces.new(list(reversed(rows[0]))).material_index = IDX[HEDGE]
        bm.faces.new(rows[-1]).material_index = IDX[HEDGE]
    bm.normal_update()
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj(name, bm)


def drive_end(name):
    """End cap of the driveway sweep at x = DRIVE_X0: the profile closed along z = DROP, facing -x."""
    bm = bmesh.new()
    prof = _road.profile(0)
    loop = [(t, z) for t, z in prof] + [(prof[-1][0], DROP), (prof[0][0], DROP)]
    v = [bm.verts.new((DRIVE_X0, t, z)) for t, z in loop]
    f = bm.faces.new(v)
    f.material_index = IDX[_road.KERB]
    bm.normal_update()
    if f.normal.x > 0:
        f.normal_flip()
    return new_obj(name, bm)


MATS = {
    _road.ASPHALT: materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
    _road.VERGE_SLOT: materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
    _road.KERB: materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
    _road.PAINT: materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6),
    GRAVEL: materials.pbr_material(GRAVEL, materials.srgb("#9a9183"), 0.85),
    # Linear, = the manifest colours, so Blender renders show the runtime look.
    EDGING: materials.pbr_material(EDGING, (0.36, 0.34, 0.30, 1.0), 0.85),
    HEDGE: materials.pbr_material(HEDGE, materials.srgb("#2f5222"), 0.9),
    STONE: materials.pbr_material(STONE, (0.42, 0.40, 0.36, 1.0), 0.8),
}

root = scene.empty(PIECE)
bodies, paints = [], []
for i, segs in enumerate(ROAD_SEGMENTS):
    bed = _road.build(f"{PIECE}_lod{i}_bed", DRIVE_X0, _road.CENTER_HALF, segs, i,
                      lambda _i, t0, t1: IDX[_road.span_slot(t0, t1)])
    for n in SLOTS:
        bed.data.materials.append(MATS[n])
    parts = [bed, drive_end(f"{PIECE}_lod{i}_end"), yard(f"{PIECE}_lod{i}_yard", CORNER_SEGS[i]),
             hedges(f"{PIECE}_lod{i}_hedges", i)]
    if i < 2:
        parts.append(edging(f"{PIECE}_lod{i}_edging", CORNER_SEGS[i]))
    pt = _road.paint(f"{PIECE}_lod{i}_paint", DRIVE_X0, _road.CENTER_HALF, IDX[_road.PAINT])
    for n in SLOTS:
        pt.data.materials.append(MATS[n])
    for o in (*parts, pt):
        uv.planar_cap_uv(o)
    body = geom.join(parts, f"{PIECE}_lod{i}")
    bodies.append(body)
    # Paint and doorsteps sit on top of the bed and the gravel and share their planar UV texels, so
    # they join after the AO bake (pitfalls.md, "Overlapping decals join after the bake").
    late = [pt]
    if i == 0:
        late.append(doorsteps(f"{PIECE}_lod{i}_steps"))
        uv.planar_cap_uv(late[-1])
    paints.append(late)

# Joint planes: only the driveway sweep may put vertices on x = +6, and nothing on x = -6.
for i, body in enumerate(bodies):
    on6 = sorted({(round(v.co.y, 4), round(v.co.z, 4)) for v in body.data.vertices if abs(v.co.x - 6.0) < 2e-3})
    want = sorted({(round(t, 4), round(z, 4)) for t, z in _road.profile(i)})
    assert on6 == want, (i, on6, want)
    assert not any(abs(v.co.x + 6.0) < 2e-3 for v in body.data.vertices), i

signature = bake.receiver_signature([bodies[0]])
texel = uv.measure_texel_density(bodies[0], MASK_SIZE)
mask_stats = None
if not skip_bake:
    bake.setup_cycles(samples=SAMPLES)
    # The hedges stand 1 m tall: an AO distance of 1.5 m shades the gravel and lawn edge beside them
    # without the whole yard reading as one dark bowl.
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(PIECE)  # reset_scene leaves no world
    scn.world.light_settings.distance = 1.5
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(bodies[0], "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    # Texel centres in Blender pixel order (row 0 = V 0): x = 32U - 16, y = 32V - 16.
    # Porosity (G): asphalt and stone 0.3, gravel 0.6 (free-draining), hedges and lawn 0.7. Snow
    # capacity (R) 1.0 everywhere. Night emissive (B) 0: the pad has no lights.
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    in_yard = (xs >= YARD[0]) & (xs <= YARD[1]) & (np.abs(ys) <= YARD[2])
    gravel_core = (xs >= YARD[0] + 0.5) & (xs <= YARD[1] - 0.5) & (np.abs(ys) <= YARD[2] - 0.5)
    in_road = (xs >= DRIVE_X0) & (np.abs(ys) <= _road.ROAD_WIDTH / 2 + _road.KERB_W)
    wet = np.where(in_road, 0.3, np.where(in_yard, 0.6, 0.7)).ravel().astype(np.float32)
    masks_png = bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
    _, px = bake.load_png(masks_png)
    a = px[gravel_core.ravel(), 3]
    mask_stats = {"yardAoMean": float(a.mean()), "yardAoP05": float(np.percentile(a, 5)),
                  "yardAoMin": float(a.min())}
    print(f"{PIECE}: masks {mask_stats}")

lods = []
for i, (body, pt) in enumerate(zip(bodies, paints)):
    body = geom.join([body, *pt], f"{PIECE}_lod{i}")
    body.data.shade_smooth()
    body.data.set_sharp_from_angle(angle=math.radians(35.0))
    body.parent = root
    lods.append(body)
    names = [m.name for m in body.data.materials]
    assert set(names) <= set(SLOTS), names
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris}")

col = collider.collider_from(lods[2], "cap", f"{PIECE}_collider")
col.parent = root
c = hexgeo.lane_frame(0, 0.0, _road.ROAD_WIDTH)["center"]
export.add_socket("socket-lane-0", (*c, 0.0), root, kind="road", edge=0, offset=0.0, width=_road.ROAD_WIDTH)
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=12,
                       aoDistanceM=1.5, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, maskStats=mask_stats, skipped=skip_bake)
