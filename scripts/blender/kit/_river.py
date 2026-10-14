"""Shared river-family geometry: the bed profile, the channel-cut bases and the river-mouth stub.

Water bed contract (docs/kit/water-contract.md, W3c), Blender frame (Z up, edge 0 faces +X):
- Water at rest z = WATER (-1.4); meadow border ring at z = 0; tile base z = -LEVEL_STEP.
- Bed profile across a river lane, d = distance to the centre line (the look-dev `carve`, w = 4.33):
  bed(d) = -1.5 max(0, 1 - (d/w)^2) - 0.05, bank(d) = smoothstep(0.7 w, w + 2.5, d),
  h(d) = min(1.4, bed + (1.4 - bed) bank), z = WATER + h(d). Waterline at d = 4.0, bank top at
  BANK_TOP = 6.83 (z = 0), centre z = -2.95. The engine computes every water input from this
  analytic profile, so `profile_errors` samples the built bed against it (contract tolerance:
  0.03 m where the water is under 2.5 m deep, which is the whole river, and 0.10 m elsewhere).
- Centre line: straight between opposite lane edges; for river_bend (lanes 0 and 2) the 24 m arc
  centred on the edge-1 neighbour's centre, Blender (APOTHEM, -24) (runtime (13.856, 24)).

Decomposition (one tile = channel-cut base + centre + a river_edge at every River edge):
- The river_edge stub owns the last STUB_L metres of the channel at its edge and the column wall
  under the channel (the cut column, PRD section 6). Its section at the border is the seam profile:
  OFFS, symmetric, at most 0.5 m apart, identical on every River edge (contract section 6).
- A channel-cut base (river_straight_base, river_bend_base) is meadow_base with the channel carved
  in: one continuous surface (meadow cap + banks + bed), the soil skirt everywhere except the
  channel spans of its lane edges, and a notch STUB_L deep at each lane edge where the stub sits.
  The joint is the plane x = APOTHEM - STUB_L on each lane edge, where both carry exactly the
  profile at OFFS. The straight stub against the 24 m bend arc differs by STUB_L^2 / 48 = 0.021 m
  in centre-line position, under 0.011 m in height.
- The channel itself is a structured strip: every vertex carries the exact profile at its row
  offset. Joint rows carry OFFS (the seam). Interior rows carry a per-LOD offset set: the bend adds
  the profile's two kinks (KINKS, where the bed parabola and the bank smoothstep start), since
  linear interpolation across the kink at d = w misses it by 0.027 m at the seam spacing. LODs
  change the row count and, for the bend, the interior offsets (`river_bend_base.py`).
- Contract tolerance on EVERY LOD (`profile_errors`): 0.03 m where the profile lies below y = -1.0
  (water under 2.5 m deep, and the swash band), 0.10 m on the drier bank and the meadow.
- LOD2 of a lane-cut base may reach LANE_CUT_LOD2_RATIO (0.20) of LOD0, an extension of the
  "LOD floor for border pieces" rule (gates.md): the ring and the one-row skirt fix about 300 tris,
  and the bend's arc needs 16 rows to stay in tolerance.
"""
import math
import os
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv

WATER = -1.4
W = 4.33
BANK_TOP = W + 2.5  # 6.83: the profile reaches z = 0 here
STUB_L = 1.0  # river_edge length along the lane, from the joint to the border
JOINT_X = hexgeo.APOTHEM - STUB_L
N_HALF = 14  # profile points per side: 6.83 / 14 = 0.488 m apart (contract: at most 0.5 m)
OFFS = [BANK_TOP * i / N_HALF for i in range(-N_HALF, N_HALF + 1)]
KINKS = (0.7 * W, W)  # bank smoothstep start (3.031) and the bed parabola's end (4.33)
WET_Z = -1.0  # contract: tighter tolerance below this height
LANE_CUT_LOD2_RATIO = 0.20


def symmetric(half):
    """Offsets mirrored about the lane centre, sorted, from a half-profile list (0 included)."""
    return sorted({round(-h, 9) for h in half} | {round(h, 9) for h in half})


OFFS_FINE = symmetric([o for o in OFFS if o >= 0] + list(KINKS))
GRASS_Z = WATER + 0.95  # grass above, river bed slot below (contract grass line 0.82-1.08 m)
RELIEF = 0.10  # meadow swell depth on river bases (contract: within 0.10 m of the profile)
BEND_R = 24.0
BEND_C = (hexgeo.APOTHEM, -24.0)  # Blender frame, edge-1 neighbour centre
MASK_SIZE = 1024

# Slots (assets/kit/manifest.json). Grass and soil match meadow_base; the bed is the gravel set.
GRASS, BED, SKIRT = "river_grass", "river_bed", "river_skirt"
SLOTS = (GRASS, BED, SKIRT)
BED_COLOR = "#8d7a5e"


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def profile_z(d):
    """Bed height (Blender z) at distance d from the centre line."""
    d = abs(d)
    if d >= BANK_TOP:
        return 0.0
    bed = -1.5 * max(0.0, 1.0 - (d / W) ** 2) - 0.05
    bank = smoothstep(0.7 * W, W + 2.5, d)
    return WATER + min(1.4, bed + (1.4 - bed) * bank)


def undulation(x, y):
    """meadow_base's swell in [0, 1] (same formula, so river tiles read as the same meadow)."""
    s = 0.6 * math.sin(0.31 * x + 1.7) * math.cos(0.27 * y - 0.4) + 0.4 * math.sin(0.53 * x - 0.45 * y + 0.9)
    return 0.5 + 0.5 * s


def relief(x, y, d):
    """Meadow cap height beyond the bank top: 0 at the border and at the bank top (value and slope),
    at most RELIEF deep."""
    fb = smoothstep(0.0, 2.0, hexgeo.dist_to_border(x, y))
    fc = smoothstep(BANK_TOP, BANK_TOP + 2.0, d)
    return -RELIEF * fb * fc * undulation(x, y)


# --- centre lines ---------------------------------------------------------------------------------

def dist_straight(x, y):
    return abs(y)


def dist_bend(x, y):
    return abs(math.hypot(x - BEND_C[0], y - BEND_C[1]) - BEND_R)


def joint_row(edge, flip=False):
    """The profile at the joint plane of lane `edge`, as Blender (x, y) for each of OFFS (order
    reversed with flip)."""
    offs = [-o for o in OFFS] if flip else OFFS
    return [hexgeo.rotate_xy((JOINT_X, o), edge) for o in offs]


def straight_rows(n, inner=None):
    """Rows of the straight channel (lanes 0 and 3), joint to joint: n segments, each row a list of
    (x, y, offset). Every row carries OFFS (`inner` is ignored: a straight strip has no sag)."""
    rows = []
    for s in range(n + 1):
        x = JOINT_X - 2.0 * JOINT_X * s / n
        rows.append([(x, o, o) for o in OFFS])
    return rows


def bend_rows(n, inner=None):
    """Rows of the bend channel (lanes 0 and 2) as lists of (x, y, offset): straight joint rows at
    the ends (OFFS, the seam), radial rows of the 24 m arc between them at offsets `inner` (default
    OFFS_FINE; radial offset o maps to Blender y = o at edge 0)."""
    inner = OFFS_FINE if inner is None else inner
    cx, cy = BEND_C
    a0 = math.atan2(0.0 - cy, JOINT_X - cx)  # arc angle where it crosses the edge-0 joint plane
    ex, ey = hexgeo.rotate_xy((JOINT_X, 0.0), 2)
    a1 = math.atan2(ey - cy, ex - cx)
    rows = [[(x, y, o) for (x, y), o in zip(joint_row(0), OFFS)]]
    for s in range(1, n):
        a = a0 + (a1 - a0) * s / n
        rows.append([(cx + (BEND_R + o) * math.cos(a), cy + (BEND_R + o) * math.sin(a), o) for o in inner])
    rows.append([(x, y, o) for (x, y), o in zip(joint_row(2, flip=True), OFFS)])
    return rows


LAYOUTS = {
    "straight": {"lanes": (0, 3), "dist": dist_straight, "rows": straight_rows},
    "bend": {"lanes": (0, 2), "dist": dist_bend, "rows": bend_rows},
}


# --- the channel-cut base -------------------------------------------------------------------------

def border_loop(lanes):
    """The hex border ring (BORDER_SUBDIV) with a notch at each lane edge: border down to the bank
    top, in to the joint, across the joint row, out to the border again. Returns [(x, y, z)]."""
    ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
    sub = hexgeo.BORDER_SUBDIV
    out = []
    for k in range(6):
        seg = ring[k * sub:(k + 1) * sub]
        if k not in lanes:
            out += [(x, y, 0.0) for x, y in seg]
            continue
        # Edge k in its own frame is edge 0 rotated: points run from o = +8 down to o = -8.
        local = [hexgeo.rotate_xy(p, -k) for p in seg]
        notch = ([(hexgeo.APOTHEM, BANK_TOP, 0.0)]
                 + [(JOINT_X, o, profile_z(o)) for o in reversed(OFFS)]
                 + [(hexgeo.APOTHEM, -BANK_TOP, 0.0)])
        pts = [(x, y, 0.0) for x, y in local if y > BANK_TOP + 1e-6]
        pts += notch
        pts += [(x, y, 0.0) for x, y in local if y < -BANK_TOP - 1e-6]
        out += [(*hexgeo.rotate_xy((x, y), k), z) for x, y, z in pts]
    return out


def channel_cap(name, layout, n_rows, spacing, inner=None):
    """One continuous surface: meadow cap (lattice at `spacing`, None for the ring only) plus the
    channel strip, triangulated together by constrained Delaunay inside the notched border loop.
    Strip vertices carry the exact profile at their offset; rows are constrained along their length,
    and between neighbouring rows wherever both carry an offset. Meadow vertices carry `relief`."""
    lay = LAYOUTS[layout]
    loop = border_loop(lay["lanes"])
    rows = lay["rows"](n_rows, inner)
    pts = [(x, y) for x, y, _ in loop]
    zs = [z for _, _, z in loop]
    index = {}
    for x, y, _ in loop:
        index[(round(x, 5), round(y, 5))] = len(index)

    def add(x, y, z):
        key = (round(x, 5), round(y, 5))
        if key not in index:
            index[key] = len(pts)
            pts.append((x, y))
            zs.append(z)
        return index[key]

    edges = []
    grid = []
    for row in rows:
        grid.append({round(o, 9): add(x, y, profile_z(o)) for x, y, o in row})
    for g in grid:
        ks = sorted(g)
        edges += [(g[a], g[b]) for a, b in zip(ks, ks[1:])]
    for g, h in zip(grid, grid[1:]):
        edges += [(g[o], h[o]) for o in sorted(g) if o in h]
    if spacing:
        for x, y in hexgeo.lattice_points(spacing, spacing * 0.5):
            d = lay["dist"](x, y)
            if d > BANK_TOP + spacing * 0.5 and all(
                    abs(hexgeo.rotate_xy((x, y), -k)[0] - JOINT_X) > 0.3 or abs(hexgeo.rotate_xy((x, y), -k)[1]) > BANK_TOP + 0.3
                    for k in lay["lanes"]):
                add(x, y, relief(x, y, d))
    nb = len(loop)
    coords = [Vector(p) for p in pts]
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt(coords, edges, [list(range(nb))], 1, 1e-6)
    verts = []
    for i, v in enumerate(out_v):
        src = [j for j in orig_v[i]]
        if not src:
            raise AssertionError(f"{name}: CDT added a Steiner vertex at {tuple(v)}")
        verts.append((v.x, v.y, zs[src[0]]))
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        p.use_smooth = True
        p.material_index = SLOTS.index(GRASS if p.center.z >= GRASS_Z else BED)
    return obj


def open_skirt(name, loop, lanes, depth, rows):
    """Soil walls under the border ring, skipping the channel span of each lane edge (the river_edge
    mouth wall fills it). Faces outward."""
    ring = [(x, y) for x, y, z in loop if abs(hexgeo.dist_to_border(x, y)) < 1e-6 and abs(z) < 1e-9]
    n = len(ring)
    verts, faces = [], []
    for i in range(n):
        a, b = ring[i], ring[(i + 1) % n]
        mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
        if any(abs(hexgeo.rotate_xy((mx, my), -k)[1]) < BANK_TOP - 1e-6
               and abs(hexgeo.rotate_xy((mx, my), -k)[0] - hexgeo.APOTHEM) < 1e-4 for k in lanes):
            continue
        base = len(verts)
        for r in range(rows + 1):
            z = -depth * r / rows
            verts += [(a[0], a[1], z), (b[0], b[1], z)]
        for r in range(rows):
            p = base + 2 * r
            faces += [[p, p + 2, p + 3], [p, p + 3, p + 1]]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        if p.normal.x * p.center.x + p.normal.y * p.center.y < 0:
            p.flip()
    obj.data.update()
    return obj


def profile_errors(obj, layout, strict=True, tol_wet=0.03, tol_dry=0.10):
    """Max deviation of the built surface from the analytic profile, sampled at vertices and face
    centroids, split as the water contract does: wet where the profile lies below WET_Z (water under
    2.5 m deep, plus the swash band), dry above it (upper bank, meadow). With `strict` (every LOD),
    raises when over the contract tolerance."""
    dist = LAYOUTS[layout]["dist"]
    mesh = obj.data
    pts = [v.co for v in mesh.vertices if v.co.z > -LEVEL_EPS_SKIRT]
    pts += [p.center for p in mesh.polygons if abs(p.normal.z) > 0.2]
    wet = dry = 0.0
    for c in pts:
        if abs(hexgeo.dist_to_border(c.x, c.y)) < 1e-4 and c.z < -1e-6:
            continue  # skirt and mouth wall
        d = dist(c.x, c.y)
        err = abs(c.z - profile_z(d))
        if profile_z(d) < WET_Z:
            wet = max(wet, err)
        else:
            dry = max(dry, err)
    assert not strict or (wet <= tol_wet and dry <= tol_dry), f"{obj.name}: profile error wet {wet:.4f} dry {dry:.4f}"
    return {"wetMaxM": round(wet, 4), "dryMaxM": round(dry, 4)}


LEVEL_EPS_SKIRT = 3.5  # surface vertices sit above -3.5 m (bed bottom -2.95); skirt rows go to -4


def bake_masks(low, out, piece, size=MASK_SIZE, wet_fn=None):
    """AO bake on LOD0 (planar UV0 over the 2R square), packed into masks (R snow 1, G wetness,
    B night 0, A AO). Returns stats over the hex footprint."""
    bake.setup_cycles(samples=64)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(piece)
    scn.world.light_settings.distance = 6.0
    ao = bake.new_image(f"{piece}.ao", size, non_color=True)
    bake.bake_to(low, "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
    c = (np.arange(size, dtype=np.float64) + 0.5) / size * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    wet = (wet_fn(xs, ys) if wet_fn else np.full(xs.shape, 0.7)).ravel().astype(np.float32)
    masks_png = bake.make_masks(size, 1.0, wet, 0.0, ao_png, str(out / f"{piece}.masks.png"))
    os.remove(ao_png)
    _, px = bake.load_png(masks_png)
    inside = (np.max([xs * math.cos(math.radians(-60 * k)) + ys * math.sin(math.radians(-60 * k))
                      for k in range(6)], axis=0) < hexgeo.APOTHEM - 0.5).ravel()
    a = px[inside, 3]
    return {"texels": int(inside.sum()), "min": float(a.min()), "mean": float(a.mean()),
            "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}


def make_materials():
    return {
        GRASS: materials.pbr_material(GRASS, materials.srgb("#4f7d32"), 0.9),
        BED: materials.pbr_material(BED, materials.srgb(BED_COLOR), 0.85),
        SKIRT: materials.pbr_material(SKIRT, materials.srgb("#6b4a2f"), 0.95),
    }


def lane_sockets(root, lanes, kind="river", width=8.0, **extras):
    """Lane sockets at the edge midpoints. A base passes cut=BANK_TOP: the gate then reads it as
    lane-cut (its border ring leaves out |offset| < cut on these edges)."""
    for k in lanes:
        c = hexgeo.lane_frame(k, 0.0, width)["center"]
        export.add_socket(f"socket-lane-{k}", (*c, 0.0), root, kind=kind, edge=k, offset=0.0, width=width,
                          **extras)


def build_base(piece, layout, n_rows, spacing, skirt_rows, generator, inner=(None, None, None)):
    """A channel-cut base: three LODs of cap + channel + open skirt, masks bake, hull collider,
    lane sockets (they mark the cut lanes for the gate), GLB and reports. `inner[i]` is LOD i's
    interior-row offset set (None: the layout's default)."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = make_materials()
    lanes = LAYOUTS[layout]["lanes"]
    loop = border_loop(lanes)
    lods, caps = [], []
    for i, (n, sp, rows, offs) in enumerate(zip(n_rows, spacing, skirt_rows, inner)):
        cap = channel_cap(f"{piece}_lod{i}_cap", layout, n, sp, offs)
        geom.set_border_normals_up(cap)
        uv.planar_cap_uv(cap)
        sk = open_skirt(f"{piece}_lod{i}_skirt", loop, lanes, hexgeo.LEVEL_STEP, rows)
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.005, 0.005)
        for o, slot in ((cap, None), (sk, SKIRT)):
            for name in SLOTS:
                o.data.materials.append(mats[name])
            if slot:
                for p in o.data.polygons:
                    p.material_index = SLOTS.index(slot)
        caps.append(cap)
        body = geom.join([cap, sk], f"{piece}_lod{i}")
        lod.triangulate(body)
        body.parent = root
        lods.append(body)
    lod.assert_border_identical(lods)
    errs = [profile_errors(o, layout) for o in lods]
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= LANE_CUT_LOD2_RATIO * tris[0], tris
    print(f"{piece}: tris per LOD {tris}, profile error per LOD {errs}")
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.parent = root
    lane_sockets(root, lanes, cut=BANK_TOP)
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    stats = None
    if not skip_bake:
        dist = LAYOUTS[layout]["dist"]

        def wet_fn(xs, ys):
            d = np.vectorize(dist)(xs, ys)
            return np.where(d < BANK_TOP, 0.9, 0.7)
        stats = bake_masks(lods[0], out, piece, wet_fn=wet_fn)
        print(f"{piece}: masks over the hex footprint {stats}")
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           aoDistanceM=6.0, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, maskStats=stats, profileErrorM=errs,
                           profileOffsets=OFFS, interiorOffsets=[list(o) if o else None for o in inner],
                           lod2MaxRatio=LANE_CUT_LOD2_RATIO, skipped=skip_bake)
    return root, lods


# --- the river-mouth stub -------------------------------------------------------------------------

def stub_mesh(name, n):
    """The channel from the joint (x = JOINT_X) to the edge-0 border in n segments, plus the mouth
    wall under the border section down to the tile base. Bed faces up, wall faces +X."""
    verts, faces = [], []
    m = len(OFFS)
    for s in range(n + 1):
        x = JOINT_X + STUB_L * s / n
        verts += [(x, o, profile_z(o)) for o in OFFS]
    for s in range(n):
        for i in range(m - 1):
            a, b = s * m + i, (s + 1) * m + i
            faces.append([a, b, b + 1, a + 1])
    top = len(verts)
    verts += [(hexgeo.APOTHEM, o, -hexgeo.LEVEL_STEP) for o in OFFS]
    edge = n * m
    for i in range(m - 1):
        faces.append([edge + i, edge + i + 1, top + i + 1, top + i])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        wall = abs(p.normal.z) < 0.5
        if wall and p.normal.x < 0 or not wall and p.normal.z < 0:
            p.flip()
    obj.data.update()
    for p in obj.data.polygons:
        p.use_smooth = abs(p.normal.z) >= 0.5
        if abs(p.normal.z) < 0.5:
            p.material_index = SLOTS.index(SKIRT)
        else:
            p.material_index = SLOTS.index(GRASS if p.center.z >= GRASS_Z else BED)
    return obj

