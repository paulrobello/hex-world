"""wheat_field: a field of ripe golden wheat in neat parallel rows on ploughed dark-brown earth, the
centre piece of the "Wheat field" tile (owner request 2026-10-19). Owner-approved references:
assets-source/refs/farm (ref-1: ploughed furrows beside rows of crops; approval.json: "wheat crop").
Scripted route, kind `prop` (a centre with no lanes is a prop, pitfalls.md: LOD0 <= 1,500 tris,
LOD1 <= 50 %, LOD2 <= 15 %), KTX2 class `piece`. Usage: make kit-build PIECE=wheat_field.

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the hex centre at y = 0; the
piece stands on meadow_base at rotation 0 in the tile frame, so it follows meadow_base's cap relief
(`_terrain.meadow_relief`, shared). The field is a hexagon of
apothem FIELD_A inside the tile, leaving a grass margin of APOTHEM - FIELD_A on every edge, so all six
edges stay Grass sockets. Rows run along Blender x (runtime x) at pitch PITCH, which divides the 24 m
between tile centres one row apart, so neighbouring fields continue each other's rows; the cards face
runtime +z, the play and thumbnail cameras' side.

Earth (slot `wheat_field_soil`): the world-space `plaster` set tinted dark brown, as the barn's
walls. A constrained Delaunay mesh with a constraint line at every ridge and furrow, so the V profile
(ridge LIFT_RIDGE, furrow LIFT_FURROW above the relief) is exact between lines, and a bank BANK wide
whose outer ring dives DIP below the relief, under the meadow cap, so grass meets the earth at a
clean edge. Every LOD is checked against the analytic relief: the earth stays at least MARGIN above it
inside the bank (the meadow cap must never poke through), and the LOD2 earth sits above y = 0
(meadow_base LOD2 is flat; base and centre switch LOD together, same origin, `use_aabb: false`).

Wheat (slot `wheat_field_wheat`): alpha-tested cards drawn in numpy (`_wheat.py`, no Cycles bake).
Per row, one exactly vertical side card (its top vertices at the exact x, z of its bottom ones, so
its triangles have no ground footprint) and one near-horizontal top card carrying the ear canopy
for the strategic camera and for views along the rows. The runtime keeps every wheat triangle out of
the walk and scatter ground (`scatter::NOT_GROUND`): the walker walks through the field on the earth.
Side cards join end to end on a global column grid, with one tile per row (tiles wrap horizontally),
so no seam shows; the last card of a row maps a proportional slice of its tile.

Wind (TEXCOORD_1 `wind`, project.md "Plant wind data"): U = 0 at each card's buried bottom and up to
sqrt(H / 2 m) at the ear tops (H = the stalk height above the ridge), so the shader's U² bend moves
the ears as the meadow grass moves; the top card takes the U of the ear line. V = a smooth phase
field over the field (one value per position), so cards butted at a joint move together and the
phase runs across the field as a slow wave. Every LOD carries it."""
import math
import os
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, budgets, collider, export, geom, hexgeo, lod, materials, scene, uv  # noqa: E402
import _props  # noqa: E402
import _terrain  # noqa: E402
import _wheat  # noqa: E402

PIECE = "wheat_field"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)

# Field layout (m).
FIELD_A = 10.8  # field apothem: a grass margin of APOTHEM - FIELD_A (3.06 m) on every edge
PITCH = 1.2  # row spacing; 20 pitches = 24 m, the z offset between tile centres one row apart
BANK = 0.35  # bank width: the earth dives under the meadow cap across it
DIP = 0.06  # the bank's outer ring sits this far below the relief
LIFT_RIDGE, LIFT_FURROW = 0.16, 0.04  # earth above the relief at the ridge (row) and the furrow
MARGIN = 0.02  # the earth stays at least this far above the relief inside the bank
LOD2_Z = 0.04  # LOD2 earth height (flat; meadow_base LOD2 is flat at 0)
COLS = (3.6, 5.4, None)  # earth column spacing along the rows per LOD (None: no interior points)
# Cards (m).
CARD_L = (3.6, 7.2, None)  # side and top card length per LOD (None: one card per row)
CARD_INSET = 0.12  # cards stop this far inside the bank's inner edge
LEAN = math.radians(22.0)  # the V pair's outward lean from vertical
FOOT = 0.07  # each V card's foot sits this far off the row line
CENTRE = (True, False, False)  # LOD0 adds a vertical card on the row line
STALK_H = 0.95  # ear tops above the ridge
SINK = 0.2  # card bottom below the ridge
CARD_H = STALK_H + SINK + 0.05  # card height along the card (the tile's top 0.05 m is clear)
# Atlas (px): ROWS row tiles of ATLAS // ROWS px over CARD_L[0] x CARD_H.
ATLAS, ROWS = 1024, 3
HALF = ATLAS // 2  # normal and ORM ship at half size (low-frequency: folds, root AO, roughness)
MASK_SIZE = 64
WIND_AMP = 0.5  # manifest `wind` (the reeds' value)
EPS = 0.5 / ATLAS

COS30 = math.sqrt(3.0) / 2.0
relief = _terrain.meadow_relief


def half_width(y, a):
    """Half width along x of the pointy-top hexagon of apothem a at height y (0 outside)."""
    return max(min(a, 2.0 * (a - COS30 * abs(y))), 0.0)


def field_dist(x, y, a=FIELD_A):
    """Distance to the border of the field hexagon of apothem a, positive inside."""
    return a - max(x * n[0] + y * n[1] for n in (hexgeo.edge_normal(k) for k in range(6)))


def hex_corners(a):
    r = a / COS30
    return [(r * math.cos(math.radians(30 + 60 * i)), r * math.sin(math.radians(30 + 60 * i))) for i in range(6)]


ROW_Y = [(i - 9.5) * PITCH for i in range(20)]  # row centre lines (ridges)
FURROW_Y = [(i - 10) * PITCH for i in range(21)]  # furrow lines between and around the rows


def profile(y):
    """Earth lift above the relief at y: LIFT_RIDGE on a row line, LIFT_FURROW halfway between."""
    t = abs(((y / PITCH) - 0.5) - round((y / PITCH) - 0.5))  # 0 on a row line, 0.5 on a furrow
    return LIFT_RIDGE + (LIFT_FURROW - LIFT_RIDGE) * (t / 0.5)


def earth_lod(i):
    """Earth mesh for LOD i (vertices, faces): CDT of the outer field hexagon with the inner bank
    hexagon and the ridge/furrow lines as constraints."""
    inner_a = FIELD_A - BANK
    pts, edges = [], []

    def add(p):
        pts.append(Vector(p))
        return len(pts) - 1

    outer = [add(c) for c in hex_corners(FIELD_A)]
    inner = [add(c) for c in hex_corners(inner_a)]
    edges += [(inner[k], inner[(k + 1) % 6]) for k in range(6)]
    if COLS[i] is not None:
        lines = sorted(set(FURROW_Y + (ROW_Y if i == 0 else [])))
        for y in lines:
            ho, hi = half_width(y, FIELD_A), half_width(y, inner_a)
            if ho <= 1e-6:
                continue
            xs = [-ho]
            if hi > 1e-6:
                s = COLS[i]
                k0 = math.ceil((-hi + 0.4) / s)
                xs += [-hi] + [k * s for k in range(k0, 1 - k0) if abs(k * s) < hi - 0.4] + [hi]
            xs.append(ho)
            ids = [add((x, y)) for x in xs]
            edges += list(zip(ids, ids[1:]))
    out_v, _e, out_f, _ov, _oe, _of = delaunay_2d_cdt(pts, edges, [outer], 1, 1e-6)
    verts = []
    for v in out_v:
        x, y = v.x, v.y
        d = field_dist(x, y)
        if COLS[i] is None:
            z = -DIP / 2 if d < 1e-4 else LOD2_Z
        else:
            z = relief(x, y) - DIP if d < 1e-4 else relief(x, y) + profile(y)
        verts.append((x, y, z))
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[j]) for j in f[:3])
        area2 = (b - a).cross(c - a).z
        # Line ends sit on the hexagon's edges, where the CDT can leave collinear slivers (zero
        # tangents in the export). They lie on the buried outer ring or the bank line; drop them.
        if abs(area2) < 1e-5:
            continue
        faces.append(list(f) if area2 > 0 else list(reversed(f)))
    used = sorted({j for f in faces for j in f})
    remap = {j: k for k, j in enumerate(used)}
    return [verts[j] for j in used], [[remap[j] for j in f] for f in faces]


def earth_deficit(verts, faces, lod_i):
    """Largest amount the earth falls below relief + MARGIN (LOD2: below LOD2_Z - 0.01) over a
    4x4 barycentric sampling of every face inside the bank, with its location."""
    worst, at = -1.0, None
    for f in faces:
        a, b, c = (np.array(verts[j]) for j in f)
        for u in range(5):
            for v in range(5 - u):
                w = (u / 4, v / 4, 1 - u / 4 - v / 4)
                p = a * w[0] + b * w[1] + c * w[2]
                if field_dist(p[0], p[1]) < BANK + 0.02:
                    continue
                need = (LOD2_Z - 0.01) if COLS[lod_i] is None else relief(p[0], p[1]) + MARGIN
                if need - p[2] > worst:
                    worst, at = need - p[2], (round(p[0], 2), round(p[1], 2))
    return worst, at


def build_earth(i):
    verts, faces = earth_lod(i)
    worst, at = earth_deficit(verts, faces, i)
    lifted = 0.0
    if worst > 0 and COLS[i] is not None:
        # Chord sag over a long column: raise every interior vertex by the deficit (one pass: a
        # uniform raise lifts every sample by the same amount).
        lifted = worst + 0.005
        verts = [(x, y, z + (lifted if field_dist(x, y) > 1e-4 else 0.0)) for x, y, z in verts]
        worst, at = earth_deficit(verts, faces, i)
    assert worst <= 0.0, (i, worst, at)
    me = bpy.data.meshes.new(f"{PIECE}_lod{i}_earth")
    me.from_pydata(verts, [], faces)
    me.validate(clean_customdata=False)
    for p in me.polygons:
        p.use_smooth = True
    uvl = me.uv_layers.new(name="UVMap")
    for loop in me.loops:
        x, y, _ = verts[loop.vertex_index]
        uvl.data[loop.index].uv = ((x + 16.0) / 32.0, (y + 16.0) / 32.0)  # masks only (uniform)
    wl = me.uv_layers.new(name="wind")
    for loop in me.loops:
        wl.data[loop.index].uv = (0.0, 0.0)
    me.uv_layers.active = uvl
    me.update()
    o = scene.link(bpy.data.objects.new(f"{PIECE}_lod{i}_earth", me))
    o.data.materials.append(soil_mat)
    return o, lifted


def phase(x, y):
    """Wind phase V in [0.05, 0.95]: one smooth value per position, a slow wave over the field."""
    return 0.5 + 0.45 * math.sin(0.45 * x + 1.3 * y)


def ridge_z(x, y, lod_i):
    return LOD2_Z if COLS[lod_i] is None else relief(x, y) + LIFT_RIDGE


def tile_uv(tile, s, t):
    """Atlas UV of `tile` ('row0', 'row1', 'top') at tile-local (s, t) in [0, 1]^2, inset half a texel."""
    v0, v1 = TILES[tile]
    return (EPS + s * (1 - 2 * EPS), v0 + EPS + t * (v1 - v0 - 2 * EPS))


def wheat_lod(i):
    """The row cards of LOD i: per row a V pair leaning outward by LEAN (its feet FOOT off the row
    line), plus on LOD0 a vertical card on the row line."""
    verts, tris, uvs, winds, nrms = [], [], [], [], []
    u_tip = math.sqrt(STALK_H / 2.0) * CARD_H / STALK_H
    inner_a = FIELD_A - BANK
    # (foot offset across the row, lean sign, tile shift): the V faces lean toward -y and +y.
    cards = [(-FOOT, -1, 0), (FOOT, 1, 1)] + ([(0.0, 0, 2)] if CENTRE[i] else [])

    def quad(corners):
        """corners: 4 x (pos, uv, wind, normal) counter-clockwise seen from the front."""
        base = len(verts)
        for p, uv_, w, n in corners:
            verts.append(tuple(p))
            uvs.append(uv_)
            winds.append(w)
            nrms.append(tuple(n))
        tris.extend(([base, base + 1, base + 2], [base, base + 2, base + 3]))

    for r, y in enumerate(ROW_Y):
        hw = half_width(y, inner_a) - CARD_INSET
        if hw < 0.3:
            continue
        tile = f"row{r % ROWS}"
        length = CARD_L[i] if CARD_L[i] is not None else 2 * hw
        if CARD_L[i] is None:
            cuts = [-hw, hw]
        else:
            k0, k1 = math.floor(-hw / length), math.ceil(hw / length)
            cuts = [-hw] + [k * length for k in range(k0 + 1, k1) if -hw < k * length < hw] + [hw]
        for x0, x1 in zip(cuts, cuts[1:]):
            if CARD_L[i] is None:
                s0, s1 = 0.0, 1.0
            else:
                # Position within the global card column, so a partial card maps its slice.
                c0 = math.floor(x0 / length + 1e-9) * length
                s0, s1 = (x0 - c0) / length, (x1 - c0) / length
            for foot, lean, shift in cards:
                ts = f"row{(r + shift) % ROWS}"
                yb = y + foot
                dy, dz = lean * CARD_H * math.sin(LEAN), CARD_H * math.cos(LEAN)
                # The shading normal: half the card's own face normal (toward the lean side, -y for
                # the centre card), half up, so the row shades as a rounded crown (never in the card
                # plane: MikkTSpace tangents stay valid).
                face = Vector((0.0, (lean or -1) * math.cos(LEAN if lean else 0.0), abs(lean) * math.sin(LEAN)))
                n = (face.normalized() * 0.5 + Vector((0.0, 0.0, 1.0)) * 0.5).normalized()
                zb0, zb1 = ridge_z(x0, yb, i) - SINK, ridge_z(x1, yb, i) - SINK
                # Bottom left, bottom right, top right, top left, counter-clockwise from the lean
                # side (front faces outward; the slot is double-sided under alpha clip anyway).
                c = [((x0, yb, zb0), tile_uv(ts, s0, 0.0), (0.0, phase(x0, y)), n),
                     ((x1, yb, zb1), tile_uv(ts, s1, 0.0), (0.0, phase(x1, y)), n),
                     ((x1, yb + dy, zb1 + dz), tile_uv(ts, s1, 1.0), (u_tip, phase(x1, y)), n),
                     ((x0, yb + dy, zb0 + dz), tile_uv(ts, s0, 1.0), (u_tip, phase(x0, y)), n)]
                quad(c if (lean or -1) < 0 else [c[1], c[0], c[3], c[2]])
    me = bpy.data.meshes.new(f"{PIECE}_lod{i}_wheat")
    me.from_pydata(verts, [], tris)
    me.validate(clean_customdata=False)
    uvl = me.uv_layers.new(name="UVMap")
    wl = me.uv_layers.new(name="wind")
    me.uv_layers.active = uvl
    loop_n = []
    for loop in me.loops:
        uvl.data[loop.index].uv = uvs[loop.vertex_index]
        u, v = winds[loop.vertex_index]
        wl.data[loop.index].uv = (min(max(u, 0.0), 1.0), min(max(v, 0.0), 1.0))
        loop_n.append(nrms[loop.vertex_index])
    for p in me.polygons:
        p.use_smooth = True
    me.normals_split_custom_set(loop_n)
    me.update()
    o = scene.link(bpy.data.objects.new(f"{PIECE}_lod{i}_wheat", me))
    o.data.materials.append(wheat_mat)
    return o


# Blender preview colours only (the runtime binds the manifest slots).
soil_mat = materials.pbr_material(f"{PIECE}_soil", materials.srgb("#3b2a1c"), 0.95)
wheat_mat = materials.pbr_material(f"{PIECE}_wheat", materials.srgb("#c9a14e"), 0.72)
TILES = {f"row{k}": (k * (ATLAS // ROWS) / ATLAS, (k + 1) * (ATLAS // ROWS) / ATLAS) for k in range(ROWS)}

lods, lifts = [], []
for i in range(3):
    earth, lifted = build_earth(i)
    wheat = wheat_lod(i)
    body = geom.join([earth, wheat], f"{PIECE}_lod{i}")
    lod.triangulate(body)
    lods.append(body)
    lifts.append(round(lifted, 4))
tris = [geom.tri_count(o) for o in lods]
part_tris = {}
for i, o in enumerate(lods):
    counts = {}
    for p in o.data.polygons:
        name = o.data.materials[p.material_index].name
        counts[name] = counts.get(name, 0) + len(p.vertices) - 2
    part_tris[f"lod{i}"] = counts


def density(o, slot):
    """Texel density (px/m) of the faces of `slot` on UV0 against the atlas width."""
    uvd = o.data.uv_layers["UVMap"].data
    out = []
    for p in o.data.polygons:
        if o.data.materials[p.material_index].name != slot:
            continue
        li = list(p.loop_indices)
        a, b, c = (Vector(uvd[j].uv) * ATLAS for j in li[:3])
        ua = abs((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)) / 2
        if p.area > 1e-9 and ua > 0:
            out.append(math.sqrt(ua / p.area))
    out.sort()
    return {"min": round(out[0], 1), "median": round(out[len(out) // 2], 1), "max": round(out[-1], 1)}


texel = density(lods[0], wheat_mat.name)
zs = [v.co.z for v in lods[0].data.vertices]
print(f"{PIECE}: tris per LOD {tris}, parts {part_tris}, wheat texel {texel}, earth lift {lifts}, "
      f"z {min(zs):.3f}..{max(zs):.3f}")
budgets.check(tris, "prop", PIECE)
assert texel["median"] >= 256, texel
assert [u.name for u in lods[0].data.uv_layers] == ["UVMap", "wind"], [u.name for u in lods[0].data.uv_layers]

# Every wheat triangle of LOD0 that is a side card is exactly vertical (no ground footprint), and
# the wind data is in [0, 1] with U = 0 on every earth vertex and on every card bottom.
for o in lods:
    wl = o.data.uv_layers["wind"].data
    w = np.empty(len(wl) * 2)
    wl.foreach_get("uv", w)
    assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
    for p in o.data.polygons:
        if o.data.materials[p.material_index].name == soil_mat.name:
            assert all(wl[j].uv[0] == 0.0 for j in p.loop_indices), o.name

# Textures: numpy atlas (albedo with alpha in A; normal and ORM at half size), uniform masks.
coverage = {}
if not skip_bake:
    alb, nrm, rough, ao, tiles = _wheat.atlas(ATLAS, ROWS, CARD_L[0], CARD_H, SINK, PIECE)
    assert tiles == TILES, (tiles, TILES)
    n2 = nrm.reshape(HALF, 2, HALF, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((HALF, HALF, 1))], -1).astype(np.float32).reshape(-1, 4)
    ao2 = ao.reshape(HALF, 2, HALF, 2).mean(axis=(1, 3))
    rough2 = rough.reshape(HALF, 2, HALF, 2).mean(axis=(1, 3))
    orm = np.stack([ao2, rough2, np.zeros_like(ao2), np.ones_like(ao2)], -1).astype(np.float32).reshape(-1, 4)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{PIECE}.albedo.png"), non_color=False)
    bake.write_png(HALF, normal, str(out / f"{PIECE}.normal.png"))
    bake.write_png(HALF, orm, str(out / f"{PIECE}.orm.png"))
    # masks (linear): R snow capacity 0.6 (ears and earth hold some), G porosity 0.6, B no night,
    # A = 1 (the unique ORM.R carries the wheat's AO; the earth's comes from its furrows' shading).
    bake.make_masks(MASK_SIZE, 0.6, 0.6, 0.0, None, str(out / f"{PIECE}.masks.png"))
    for name, (v0, v1) in TILES.items():
        a = alb[round(v0 * ATLAS):round(v1 * ATLAS), :, 3]
        coverage[name] = _props.coverage_by_mip(a)
    print(f"{PIECE}: alpha coverage by mip {coverage}")
    # Mips down to 8 px tall (a tile's last rows are a few texels, where one row decides).
    for name, cov in coverage.items():
        rows_px = round((TILES[name][1] - TILES[name][0]) * ATLAS)
        keep = [c for k, c in enumerate(cov) if rows_px >> k >= 8]
        assert min(keep) >= 0.5, (name, cov)

col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": HALF, "colorspace": "linear"},
                                 "orm": {"size": HALF, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM={"wheat_field_wheat": texel},
                       bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                       partTris=part_tris, earthLiftM=lifts, extentZ=[min(zs), max(zs)],
                       field={"apothemM": FIELD_A, "pitchM": PITCH, "rows": len(ROW_Y), "bankM": BANK},
                       skipped=skip_bake)
