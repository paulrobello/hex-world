"""bakery_pad: the bakery tile's centre piece (P3, owner A17: "its own tile with a road through it, so
it chains like houses"; refs assets-source/refs/bakery). Scripted route.

A straight through road from edge 0 to edge 3 (x in [-6, 6], the shared PROFILE) with a gravel
forecourt on its north side (runtime -z, Blender +y) beside the bakery_shop hero (data/tiles/bakery.ron:
runtime (-4.1, -8.5), yaw 90, its shop front to the road). The forecourt opens straight off the
asphalt: between x = MOUTH[0] and MOUTH[1] the road keeps only its south half and asphalt (PROFILE up
to the north kerb foot, (3, BED_TOP)), the north kerb, verge and sink stop in square end caps, and
the gravel continues level with the asphalt edge. A filleted T spur cannot hold two pickup spots
inside |x| < 6 (fillet radius >= 1.62 m to sweep the kerb-to-sink half profile, and every vertex off
the joint planes): measured layouts in assets-source/kit/bakery_pad/layout/.

Parking (PRD §7.1): `anchor-park-<n>` for spot n (park-<n> in bakery.ron), at the centre of a parked
5.4 x 2.0 m car at gravel height, rotated to its heading, extras {kind: "park", house: 0 (the bakery
hero), length, width, heading_deg}. Cars arrive from the "bakery" node at the mouth, nose in north,
and leave with a straight reverse to the mouth and a swing onto the road (bakery.ron).

Joint contract (kit_gate `lane_joint_errors`): the through road presents PROFILE plus the two paint
corners at x = +6 and x = -6 in every LOD (`_road_bend.joint_section_errors`, lanes 0 and 3), and the
piece declares `socket-lane-0` and `socket-lane-3`. Nothing else comes within 1 mm of x = +-6: the
forecourt's east edging ends 3 cm short of x = 6.

Levels as house_pad: gravel top at BED_TOP (the asphalt edge it continues), edging 4 cm above it,
every rim dropping to -0.32 m, below the meadow cap's deepest relief (-0.25 m)."""
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
from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene, uv  # noqa: E402

import _road  # noqa: E402
import _road_bend  # noqa: E402

PIECE = "bakery_pad"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)

GRAVEL, EDGING, HEDGE = f"{PIECE}_gravel", f"{PIECE}_edging", f"{PIECE}_hedge"
SLOTS = (*_road.SLOTS, GRAVEL, EDGING, HEDGE)
IDX = {n: i for i, n in enumerate(SLOTS)}

J = _road.CENTER_HALF
H = _road.ROAD_WIDTH / 2
MOUTH = (-3.8, 5.85)  # x where the north kerb stops (square end caps)
# Forecourt outline in Blender (x, y), counter-clockwise after the south edge (on the asphalt edge
# y = H, x from MOUTH[0] to MOUTH[1]): east edge, north edge, the strip beside the shop's east gable,
# and the shop front (the plinth face sits at y = 5.96; the gravel runs 4 cm under it).
# The west edge steps at STEP_Y: beside the shop's east chimney (x <= 0.0 for Blender y 7.85-9.95) it
# stays at WEST_X, so the edging's outer face clears it, and nearer the road it widens to WEST_NEAR so
# cars turning into spot 0 clear the edging.
NORTH, WEST_X, WEST_NEAR, STEP_Y, FRONT_Y = 12.0, 0.14, -0.4, 7.45, 6.0
FORE_TAIL = [(MOUTH[1], NORTH), (WEST_X, NORTH), (WEST_X, STEP_Y), (WEST_NEAR, STEP_Y),
             (WEST_NEAR, FRONT_Y), (MOUTH[0], FRONT_Y)]
PAVE_Z = _road.BED_TOP
EDGE_TOP, EDGE_W, DROP = PAVE_Z + 0.04, 0.12, -0.32
# Through-road length segments per LOD: west of the mouth, the mouth, east of the mouth.
ROAD_SEGMENTS = ((4, 28, 1), (2, 12, 1), (1, 1, 1))
# The hedge along the forecourt's north edge (x0, x1 at Blender y = HEDGE_Y).
HEDGE_RUN, HEDGE_Y = (0.4, 4.6), NORTH + EDGE_W + 0.43
HEDGE_W, HEDGE_H = 0.65, 0.95
HEDGE_STEP = (0.25, 0.8, None)
HEDGE_RING = (12, 6, 4)
# Parking spots in the runtime frame (x, z, heading yaw from +x toward +z), nose north.
PARK = ((1.8, -8.8, 270.0), (4.5, -8.6, 270.0))
PARK_L, PARK_W = 5.4, 2.0
MASK_SIZE = 512  # 16 px/m over the 2R square, as house_pad and the road pieces
SAMPLES = 64
AO_DIST = 1.5

MATS = {
    _road.ASPHALT: materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
    _road.VERGE_SLOT: materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
    _road.KERB: materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
    _road.PAINT: materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6),
    GRAVEL: materials.pbr_material(GRAVEL, materials.srgb("#9a9183"), 0.85),
    # Linear, = the manifest colours, so Blender renders show the runtime look.
    EDGING: materials.pbr_material(EDGING, (0.36, 0.34, 0.30, 1.0), 0.85),
    HEDGE: materials.pbr_material(HEDGE, materials.srgb("#2f5222"), 0.9),
}


def new_obj(name, bm):
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for n in SLOTS:
        obj.data.materials.append(MATS[n])
    return obj


def road(name, lod):
    """The through road: full PROFILE west and east of the mouth, the south half plus the asphalt up
    to the north kerb foot across it, and square end caps on the cut north kerb, verge and sink."""
    m = _road_bend.Mesh()
    south = _road.PROFILE[:10]
    assert south[-1] == (H, _road.BED_TOP), south[-1]
    nw, nm, ne = ROAD_SEGMENTS[lod]
    for x0, x1, n, prof in ((-J, MOUTH[0], nw, _road.PROFILE), (MOUTH[0], MOUTH[1], nm, south),
                            (MOUTH[1], J, ne, _road.PROFILE)):
        rows = [[m.add((x0 + (x1 - x0) * s / n, t, z)) for t, z in prof] for s in range(n + 1)]
        m.sweep(rows, prof)
    obj = m.obj(name, MATS, weld=True)
    for n in SLOTS[len(_road.SLOTS):]:
        obj.data.materials.append(MATS[n])
    return obj


def kerb_caps(name):
    """The cut north kerb, verge and sink closed at both mouth ends, facing into the mouth."""
    bm = bmesh.new()
    north = [p for p in _road.PROFILE if p[0] >= H]
    loop = north + [(north[-1][0], DROP), (north[0][0], DROP)]
    for x, facing in ((MOUTH[0], 1.0), (MOUTH[1], -1.0)):
        f = bm.faces.new([bm.verts.new((x, t, z)) for t, z in loop])
        f.material_index = IDX[_road.KERB]
        bm.normal_update()
        if f.normal.x * facing < 0:
            f.normal_flip()
    return new_obj(name, bm)


def outline(lod):
    """Forecourt outline (CCW): the south edge at the mouth road's row stations, then FORE_TAIL."""
    n = ROAD_SEGMENTS[lod][1]
    south = [(MOUTH[0] + (MOUTH[1] - MOUTH[0]) * s / n, H) for s in range(n + 1)]
    return south + FORE_TAIL, n + 1


def forecourt(name, lod):
    """Level gravel at PAVE_Z with walls down to DROP on every side but the asphalt edge."""
    pts, n_south = outline(lod)
    bm = bmesh.new()
    top = [bm.verts.new((x, y, PAVE_Z)) for x, y in pts]
    bot = [bm.verts.new((x, y, DROP)) for x, y in pts]
    bm.faces.new(top).material_index = IDX[GRAVEL]
    k = len(pts)
    for i in range(n_south - 1, k):
        j = (i + 1) % k
        bm.faces.new((top[i], bot[i], bot[j], top[j])).material_index = IDX[GRAVEL]
    bm.normal_update()
    assert all(f.normal.z > 0.99 for f in bm.faces if f.material_index == IDX[GRAVEL] and len(f.verts) > 4)
    return new_obj(name, bm)


def outward(pts, i):
    """Outward unit normal at vertex i of a CCW polyline (average of the adjacent edge normals)."""
    n = len(pts)
    ax, ay = pts[i - 1] if i > 0 else pts[i]
    bx, by = pts[(i + 1) % n] if i < n - 1 else pts[i]
    dx, dy = bx - ax, by - ay
    ln = math.hypot(dx, dy)
    return dy / ln, -dx / ln


def edging(name):
    """Stone edging where the gravel meets lawn: up the east edge from the verge's sink line, along
    the north edge and down the strip beside the shop's gable, and the west end from the shop front
    to the sink line. Inner face down to the gravel, outer face down to DROP."""
    sink = _road.SINK_T
    runs = ([(MOUTH[1], sink), *FORE_TAIL[:5]], [(MOUTH[0], FRONT_Y), (MOUTH[0], sink)])
    bm = bmesh.new()
    for pts in runs:
        rows = []
        for i, (x, y) in enumerate(pts):
            nx, ny = outward(pts, i)
            ox, oy = x + nx * EDGE_W, y + ny * EDGE_W
            rows.append([bm.verts.new(v) for v in ((x, y, PAVE_Z), (x, y, EDGE_TOP), (ox, oy, EDGE_TOP), (ox, oy, DROP))])
        for a, b in zip(rows, rows[1:]):
            for k in range(3):
                bm.faces.new((a[k], b[k], b[k + 1], a[k + 1])).material_index = IDX[EDGING]
        for r, flip in ((rows[0], True), (rows[-1], False)):
            bm.faces.new(r if flip else list(reversed(r))).material_index = IDX[EDGING]
    bm.normal_update()
    return new_obj(name, bm)


def hedge_section(m, rng):
    """Closed cross-section (lateral offset, height) of a clipped hedge, base buried at DROP; m
    vertices, CCW seen from the +length end (house_pad's hedge)."""
    pts = []
    for k in range(m):
        a = 2 * math.pi * k / m - math.pi / 2
        sx, sz = math.cos(a), math.sin(a)
        t = HEDGE_W / 2 * math.copysign(abs(sx) ** 0.45, sx)
        z = (HEDGE_H - DROP) / 2 * math.copysign(abs(sz) ** 0.6, sz) + (HEDGE_H + DROP) / 2
        if rng is not None and z > 0.0:
            t *= 1.0 + rng.uniform(-0.08, 0.1)
            z += rng.uniform(-0.05, 0.04) * (z / HEDGE_H)
        pts.append((t, max(z, DROP)))
    return pts


def hedge(name, lod):
    bm = bmesh.new()
    m = HEDGE_RING[lod]
    rng = random.Random(f"{PIECE}-hedge") if lod == 0 else None
    x0, x1 = HEDGE_RUN
    step = HEDGE_STEP[lod]
    n = max(1, round((x1 - x0) / step)) if step else 1
    rows = []
    for s in range(n + 1):
        x = x0 + (x1 - x0) * s / n
        rows.append([bm.verts.new((x, HEDGE_Y - t, z)) for t, z in hedge_section(m, rng)])
    for a, b in zip(rows, rows[1:]):
        for k in range(m):
            j = (k + 1) % m
            bm.faces.new((a[k], a[j], b[j], b[k])).material_index = IDX[HEDGE]
    bm.faces.new(list(reversed(rows[0]))).material_index = IDX[HEDGE]
    bm.faces.new(rows[-1]).material_index = IDX[HEDGE]
    bm.normal_update()
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj(name, bm)


root = scene.empty(PIECE)
bodies, paints = [], []
for i in range(3):
    parts = [road(f"{PIECE}_lod{i}_road", i), kerb_caps(f"{PIECE}_lod{i}_caps"),
             forecourt(f"{PIECE}_lod{i}_fore", i), hedge(f"{PIECE}_lod{i}_hedge", i)]
    if i < 2:
        parts.append(edging(f"{PIECE}_lod{i}_edging"))
    pt = _road.paint(f"{PIECE}_lod{i}_paint", -J, J, IDX[_road.PAINT])
    for n in SLOTS:
        pt.data.materials.append(MATS[n])
    for o in (*parts, pt):
        uv.planar_cap_uv(o)
    bodies.append(geom.join(parts, f"{PIECE}_lod{i}"))
    # The paint sits on the bed and shares its planar UV texels, so it joins after the AO bake
    # (pitfalls.md, "Overlapping decals join after the bake").
    paints.append(pt)

signature = bake.receiver_signature([bodies[0]])
texel = uv.measure_texel_density(bodies[0], MASK_SIZE)
mask_stats = None
if not skip_bake:
    bake.setup_cycles(samples=SAMPLES)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(PIECE)  # reset_scene leaves no world
    scn.world.light_settings.distance = AO_DIST
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(bodies[0], "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    # Texel centres in Blender pixel order (row 0 = V 0): x = 32U - 16, y = 32V - 16. Porosity (G):
    # asphalt and kerb 0.3, gravel and edging 0.6, verge, lawn and hedge 0.7. Snow (R) 1.0, night (B) 0.
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    poly = [(MOUTH[0], H), (MOUTH[1], H), *FORE_TAIL]
    in_fore = _road_bend.inside(poly, xs, ys)
    in_road = (np.abs(xs) <= J + 1.0) & (ys >= -(H + _road.KERB_W)) & (ys <= H + _road.KERB_W) & ~in_fore
    wet = np.where(in_road, 0.3, np.where(in_fore, 0.6, 0.7)).ravel().astype(np.float32)
    masks_png = bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
    _, px = bake.load_png(masks_png)
    core = _road_bend.inside([(MOUTH[0] + 0.5, H + 0.5), (MOUTH[1] - 0.5, H + 0.5), (MOUTH[1] - 0.5, NORTH - 0.5),
                              (WEST_X + 0.5, NORTH - 0.5), (WEST_X + 0.5, FRONT_Y + 0.5), (MOUTH[0] + 0.5, FRONT_Y + 0.5)],
                             xs, ys)
    a = px[core.ravel(), 3]
    mask_stats = {"forecourtAoMean": float(a.mean()), "forecourtAoP05": float(np.percentile(a, 5)),
                  "forecourtAoMin": float(a.min())}
    print(f"{PIECE}: masks {mask_stats}")

lods = []
for i, (body, pt) in enumerate(zip(bodies, paints)):
    body = geom.join([body, pt], f"{PIECE}_lod{i}")
    body.data.shade_smooth()
    body.data.set_sharp_from_angle(angle=math.radians(35.0))
    _road.verge_normals_up(body)
    body.parent = root
    lods.append(body)
    names = [m.name for m in body.data.materials]
    assert set(names) <= set(SLOTS), names
tris = [geom.tri_count(o) for o in lods]
print(f"{PIECE}: tris per LOD {tris}")
errs = [e for o in lods for e in _road_bend.joint_section_errors(o, (0, 3))]
assert not errs, "\n".join(errs)
budgets.check(tris, "center", PIECE)

col = collider.collider_from(lods[2], "cap", f"{PIECE}_collider")
col.parent = root
for k in (0, 3):
    c = hexgeo.lane_frame(k, 0.0, _road.ROAD_WIDTH)["center"]
    export.add_socket(f"socket-lane-{k}", (*c, 0.0), root, kind="road", edge=k, offset=0.0, width=_road.ROAD_WIDTH)
fore_poly = [(MOUTH[0], H), (MOUTH[1], H), *FORE_TAIL]
for n, (x, z, yaw) in enumerate(PARK):
    # Runtime (x, z) is Blender (x, -z), and a runtime heading yaw is Blender yaw -yaw. Every corner
    # of the parked car stands on the gravel.
    hx, hz = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
    for a, b in ((1, 1), (1, -1), (-1, -1), (-1, 1)):
        cx = x + hx * a * PARK_L / 2 - hz * b * PARK_W / 2
        cz = z + hz * a * PARK_L / 2 + hx * b * PARK_W / 2
        inside = _road_bend.inside(fore_poly, np.array([cx]), np.array([-cz]))[0]
        assert inside, (n, cx, cz)
    export.add_socket(f"anchor-park-{n}", (x, -z, PAVE_Z), root, yaw_deg=-yaw, kind="park", house=0,
                      length=PARK_L, width=PARK_W, heading_deg=yaw)
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=12,
                       aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, maskStats=mask_stats, profile=[list(p) for p in _road.PROFILE],
                       mouthX=list(MOUTH), skipped=skip_bake)
