"""river_road_bridge_center: a stone arch road bridge carrying the road lane (edges 1 and 4) across
the straight river channel (edges 0 and 3) at 60 degrees. It sits on river_straight_base, which
carries the whole bed.

- Road: exactly road_straight_center's sweep (`_road.build` / `_road.paint`, x in [-6, 6], the
  same segments per LOD), turned by one edge step so it runs from lane 4 to lane 1. Its sections at
  r = +-6 are therefore the road_straight_edge section, and the frozen road stubs meet it.
- Stone causeway: the road strip (|t| <= W = 4.6, the road profile's outer sink edge) crossed with
  the channel band (|y| <= BANK_TOP), so it also carries the parts of the road stubs that hang over
  the channel (they reach the band until r = 10.7). Skew face walls on t = +-W from 3 m down up to
  DECK_Z (1 cm under the road's sink edge), an elliptical arch across the river (springing at
  y = +-ARCH_A just above the water, crown 1 m higher) and its barrel along the river, pier faces
  under the springing, and parapets with end pillars on the road edges over the water.
- Bank stones and reeds as on the other river centres, kept clear of the causeway.
- Masks (planar UV0, 512 px): the road's AO bake as before, plus `dress_masks` for the slots that
  carry no AO: the bank stones and reeds take the river centres' field (`_river_dress.masks_field`)
  over their own texels and a halo around them, and the causeway's one parked texel (`STONE_UV`)
  takes stone values. A is 1 on all of it, the AO those slots had before they bound the map, so the
  MVP look is unchanged. The halo keeps the unbaked A = 0 background out of the dressing's mips.
Road frame: r along the road (lane 4 -> lane 1), t across it; Blender (x, y) = r u + t v with
u = (0.5, -0.866) and v = (0.866, 0.5) (hexgeo.rotate_xy by one edge step)."""
import math
import os
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

import _river
import _river_dress
import _road

W = _road.SINK_T  # 4.6: causeway half-width = the road profile's outer edge
DECK_Z = _road.BED_SINK - 0.01  # face wall top, just under the road's sink edge
FOOT_Z = -3.0  # below the deepest bed point (-2.95)
ARCH_A = 3.6  # springing half-span across the river (waterline at 4.0)
SPRING_Z = _river.WATER - 0.05
ARCH_RISE = 1.0
PARAPET_T = (4.3, 4.6)
PARAPET_TOP = 0.75
PARAPET_Y = 5.0  # parapets cover |y| <= PARAPET_Y (the water and the lower banks)
PILLAR = 0.25  # pillar half-size
PILLAR_TOP = 0.95
STONE_SLOT = "bridge_stone"
ROAD_SEGMENTS = (12, 5, 1)  # = road_straight_center
ARCH_SEGMENTS = (16, 8, 3)
MASK_SIZE = 512
STONE_UV = (0.005, 0.005)  # where every causeway loop is parked (a world-space tileable slot)
STONE_POROSITY = 0.8  # masks G on the causeway: stone over the water
STONE_BLOCK = 16  # the parked texel's corner block, so the mips down to 32 x 32 still read stone
# The AO bake leaves A = 0 off the road, which box-filtered mips blend into the dressing (A 0.4 at
# mip 4 without a halo). HALO_PX of dressing values around its texels keeps A >= 0.9 to mip 4;
# ROAD_CLEAR_PX of untouched texels around the road's baked texels keep road mips 0-4 unchanged.
HALO_PX = 32
ROAD_CLEAR_PX = 11
COS, SIN = math.cos(math.radians(60)), math.sin(math.radians(60))


def rt(r, t):
    """Road frame -> Blender (x, y)."""
    return (COS * r + SIN * t, -SIN * r + COS * t)


def road_r(x, y):
    return COS * x - SIN * y


def road_t(x, y):
    return SIN * x + COS * y


def face_x(t, y):
    """x on the skew face plane t = const at river-lateral y."""
    return (t - COS * y) / SIN


def _mesh(name, verts, faces):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    return scene.link(bpy.data.objects.new(name, mesh))


def arch_points(n):
    return [(ARCH_A * math.cos(math.pi * (1 - i / n)), SPRING_Z + ARCH_RISE * math.sin(math.pi * (1 - i / n)))
            for i in range(n + 1)]


def causeway(name, n_arch):
    """Face walls, barrel and pier faces of the stone causeway (no top: the road covers it, no ends:
    they are buried in the banks at |y| = BANK_TOP)."""
    y_top = _river.BANK_TOP
    arch = arch_points(n_arch)
    outline = ([(-y_top, FOOT_Z), (-ARCH_A, FOOT_Z)] + arch
               + [(ARCH_A, FOOT_Z), (y_top, FOOT_Z), (y_top, DECK_Z), (-y_top, DECK_Z)])
    out_v, _e, out_f, _ov, _oe, _of = delaunay_2d_cdt([Vector(p) for p in outline], [],
                                                      [list(range(len(outline)))], 1, 1e-6)
    verts, faces = [], []
    for side in (-1, 1):
        t = side * W
        base = len(verts)
        verts += [(face_x(t, v.x), v.x, v.y) for v in out_v]
        for f in out_f:
            faces.append([base + i for i in f])
    # Barrel (intrados) between the two faces, and the pier faces under the springing.
    strips = [arch, [(-ARCH_A, FOOT_Z), (-ARCH_A, SPRING_Z)], [(ARCH_A, SPRING_Z), (ARCH_A, FOOT_Z)]]
    for strip in strips:
        base = len(verts)
        for y, z in strip:
            verts += [(face_x(-W, y), y, z), (face_x(W, y), y, z)]
        for i in range(len(strip) - 1):
            a = base + 2 * i
            faces.append([a, a + 1, a + 3, a + 2])
    n_wall = 2 * len(out_f)
    obj = _mesh(name, verts, faces)
    # Orient: face walls outward along +-v; barrel and piers toward the opening's middle line.
    v = Vector((SIN, COS, 0.0))
    for p in obj.data.polygons:
        c = p.center
        if p.index < n_wall:
            want = 1.0 if road_t(c.x, c.y) > 0 else -1.0
            if p.normal.dot(v) * want < 0:
                p.flip()
        else:
            inward = Vector((0.0, -c.y, SPRING_Z - c.z if abs(c.y) < ARCH_A - 1e-3 else 0.0))
            if p.normal.dot(inward) < 0:
                p.flip()
    obj.data.update()
    return obj


def box(bm, r0, r1, t0, t1, z0, z1):
    """An axis-aligned box in the road frame (no bottom face). (r, t) -> (x, y) is a rotation, so
    the counter-clockwise corner order gives an up-facing top and outward sides."""
    c = [rt(r, t) for r, t in ((r0, t0), (r1, t0), (r1, t1), (r0, t1))]
    lo = [bm.verts.new((x, y, z0)) for x, y in c]
    hi = [bm.verts.new((x, y, z1)) for x, y in c]
    bm.faces.new(hi)
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))


def parapets(name):
    bm = bmesh.new()
    for side in (-1, 1):
        t0, t1 = sorted((side * PARAPET_T[0], side * PARAPET_T[1]))
        tc = (t0 + t1) / 2
        # r where the parapet centre line crosses y = -+PARAPET_Y.
        ra, rb = sorted(((COS * tc + PARAPET_Y) / SIN, (COS * tc - PARAPET_Y) / SIN))
        box(bm, ra, rb, t0, t1, DECK_Z, PARAPET_TOP)
        for r in (ra, rb):
            box(bm, r - PILLAR, r + PILLAR, tc - PILLAR, tc + PILLAR, DECK_Z, PILLAR_TOP)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(name, mesh))


def road(name, lod, slots_index):
    bed = _road.build(f"{name}_bed", -_road.CENTER_HALF, _road.CENTER_HALF, ROAD_SEGMENTS[lod], lod,
                      lambda _i, t0, t1: slots_index[_road.span_slot(t0, t1)])
    pt = _road.paint(f"{name}_paint", -_road.CENTER_HALF, _road.CENTER_HALF, slots_index[_road.PAINT])
    for o in (bed, pt):
        o.rotation_euler.z = math.radians(-60.0)
        scene.apply_transforms(o)
    return bed, pt


def dressing(name, lod, rnd_seed):
    """Bank stones and reeds along the straight channel, clear of the causeway (|t| > W + 1.2)."""
    rnd = random.Random(rnd_seed)
    stones, reeds = [], []
    for side in (-1, 1):
        for i in range(14):
            x = _river.JOINT_X - 2 * _river.JOINT_X * (i + 0.5 + 0.6 * (rnd.random() - 0.5)) / 14
            y = side * rnd.uniform(3.6, 4.5)
            size = (rnd.uniform(0.25, 0.55), rnd.uniform(0.2, 0.45), rnd.uniform(0.15, 0.3))
            stones.append(((x, y, _river.profile_z(y) + 0.03), size, rnd.randrange(1 << 30)))
        for i in range(5):
            x = _river.JOINT_X - 2 * _river.JOINT_X * (i + 0.3 + 0.4 * rnd.random()) / 5
            y = side * rnd.uniform(4.7, 5.7)
            reeds.append(((x, y, _river.profile_z(y)), rnd.randrange(1 << 30)))
    clear = W + 1.2
    stones = [s for s in stones if abs(road_t(s[0][0], s[0][1])) > clear]
    reeds = [r for r in reeds if abs(road_t(r[0][0], r[0][1])) > clear]
    stones.sort(key=lambda st: -st[1][0] * st[1][1])
    bm = bmesh.new()
    for at, size, seed in stones[: (len(stones), len(stones) // 2, 3)[lod]]:
        _river_dress._stone(bm, at, size, seed)
    n_stone = len(bm.faces)
    tops = []
    for at, seed in (reeds, reeds[::2], [])[lod]:
        tops += _river_dress._reed_clump(bm, at, seed, 9 if lod == 0 else 4, lod == 0)
    n_reed = len(bm.faces)
    for top in tops:
        _river_dress._cattail(bm, top)
    bm.faces.ensure_lookup_table()
    for fi, f in enumerate(bm.faces):
        f.material_index = 0 if fi < n_stone else (1 if fi < n_reed else 2)
        f.smooth = False
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(name, mesh)), len(stones), len(reeds)


def uv_footprint(objs, size, grow=0):
    """(size*size,) bool, Blender pixel order: the texels whose centres lie in a UV0 triangle of
    objs, plus every texel under a UV0 vertex (thin reed blades), grown by `grow` texels."""
    m = np.zeros((size, size), bool)
    for o in objs:
        uvl = o.data.uv_layers["UVMap"].data
        for poly in o.data.polygons:
            uvs = np.array([tuple(uvl[li].uv) for li in poly.loop_indices]) * size
            for u, v in uvs:
                m[min(max(int(v), 0), size - 1), min(max(int(u), 0), size - 1)] = True
            for k in range(1, len(uvs) - 1):
                a, b, c = uvs[0], uvs[k], uvs[k + 1]
                lo = np.clip(np.floor(np.minimum(np.minimum(a, b), c)).astype(int), 0, size - 1)
                hi = np.clip(np.ceil(np.maximum(np.maximum(a, b), c)).astype(int), 0, size - 1)
                ii, jj = np.meshgrid(np.arange(lo[0], hi[0] + 1), np.arange(lo[1], hi[1] + 1))
                px, py = ii + 0.5, jj + 0.5
                d1 = (px - b[0]) * (a[1] - b[1]) - (a[0] - b[0]) * (py - b[1])
                d2 = (px - c[0]) * (b[1] - c[1]) - (b[0] - c[0]) * (py - c[1])
                d3 = (px - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (py - a[1])
                inside = ~(((d1 < 0) | (d2 < 0) | (d3 < 0)) & ((d1 > 0) | (d2 > 0) | (d3 > 0)))
                m[jj[inside], ii[inside]] = True
    return grow_mask(m, grow).ravel()


def grow_mask(m, n):
    """A 2D bool mask grown by n texels (4-neighbourhood)."""
    for _ in range(n):
        g = m.copy()
        g[1:, :] |= m[:-1, :]
        g[:-1, :] |= m[1:, :]
        g[:, 1:] |= m[:, :-1]
        g[:, :-1] |= m[:, 1:]
        m = g
    return m


def dress_masks(masks_png, dress, road, size=MASK_SIZE):
    """Writes the dressing's and the causeway's masks into the road's baked masks without touching
    a road texel or the road's mips 0-4: the dressing's footprint (grown 3 texels) must miss the
    road's baked texels (A > 0: islands and gutter), its halo (HALO_PX) stays ROAD_CLEAR_PX from
    them, and the parked stone block misses both. Returns texel counts for the bake report."""
    n, px = bake.load_png(masks_png)
    assert n == size, (n, size)
    baked = (px[:, 3] > 0).reshape(size, size)
    dress_fp = uv_footprint(dress, size, grow=3)
    road_fp = uv_footprint(road, size)
    assert not (dress_fp & (road_fp | baked.ravel())).any(), "dressing masks would overwrite road texels"
    halo = grow_mask(dress_fp.reshape(size, size), HALO_PX) & ~grow_mask(baked, ROAD_CLEAR_PX)
    fill = dress_fp | halo.ravel()
    block = np.zeros((size, size), bool)
    j0, i0 = int(STONE_UV[1] * size), int(STONE_UV[0] * size)
    assert j0 < STONE_BLOCK and i0 < STONE_BLOCK, (j0, i0)
    block[:STONE_BLOCK, :STONE_BLOCK] = True
    block = block.ravel()
    assert not (block & (road_fp | baked.ravel() | fill)).any(), "parked stone block overlaps other texels"
    c = (np.arange(size, dtype=np.float64) + 0.5) / size * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    wet = _river_dress.porosity(np.vectorize(_river.LAYOUTS["straight"]["dist"])(xs, ys)).ravel()
    k = int(fill.sum())
    px[fill] = np.stack([np.ones(k), wet[fill], np.zeros(k), np.ones(k)], axis=1)
    px[block] = (1.0, STONE_POROSITY, 0.0, 1.0)
    bake.write_png(size, px, masks_png)
    return {"dressTexels": int(dress_fp.sum()), "haloTexels": int(fill.sum() - dress_fp.sum()),
            "stoneTexels": int(block.sum()), "roadTexels": int(road_fp.sum())}


def joint_clearance(objs, tol=0.002):
    """Only the road may have vertices on the r = +-6 joint planes (the gate compares every vertex
    there with the road edge section)."""
    bad = []
    for o in objs:
        for v in o.data.vertices:
            w = o.matrix_world @ v.co
            if abs(abs(road_r(w.x, w.y)) - _road.CENTER_HALF) < tol:
                bad.append((o.name, tuple(round(c, 3) for c in w)))
    assert not bad, f"vertices on a road joint plane: {bad[:5]}"


def build(piece, generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    road_mats = {
        _road.ASPHALT: materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
        _road.VERGE_SLOT: materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
        _road.KERB: materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
        _road.PAINT: materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6),
    }
    stone_mat = materials.pbr_material(STONE_SLOT, materials.srgb("#8f877a"), 0.85)
    dress_mats = [materials.pbr_material(n, materials.srgb(c), r) for n, c, r in (
        (_river_dress.STONE, "#8a8378", 0.8), (_river_dress.REED, "#5d7a35", 0.85),
        (_river_dress.CATTAIL, "#5a3a22", 0.9))]
    slots = list(_road.SLOTS) + [STONE_SLOT] + list(_river_dress.DRESS_SLOTS)
    all_mats = [road_mats[n] for n in _road.SLOTS] + [stone_mat] + dress_mats
    index = {n: i for i, n in enumerate(slots)}
    parts, beds, dresses, roads = [], [], [], []
    stats = {}
    for li in range(3):
        bed, pt = road(f"{piece}_lod{li}_road", li, index)
        cw = causeway(f"{piece}_lod{li}_causeway", ARCH_SEGMENTS[li])
        stone_parts = [cw] + ([parapets(f"{piece}_lod{li}_parapets")] if li < 2 else [])
        dress, n_st, n_re = dressing(f"{piece}_lod{li}_dress", li, piece)
        stats = {"stones": n_st, "reedClumps": n_re}
        joint_clearance(stone_parts + [dress])
        group = [bed, pt] + stone_parts + [dress]
        # Slots first: Blender clamps a polygon's material_index to the mesh's slot count, so an
        # index set before the slots exist silently becomes 0.
        dress_index = [p.material_index for p in dress.data.polygons]
        road_index = {o.name: [p.material_index for p in o.data.polygons] for o in (bed, pt)}
        for o in group:
            o.data.materials.clear()
            for m in all_mats:
                o.data.materials.append(m)
            uv.planar_cap_uv(o)
        for o in (bed, pt):
            for p, i in zip(o.data.polygons, road_index[o.name]):
                p.material_index = i
        for o in stone_parts:
            for p in o.data.polygons:
                p.material_index = index[STONE_SLOT]
                p.use_smooth = False
        for p, i in zip(dress.data.polygons, dress_index):
            p.material_index = i + index[_river_dress.STONE]
        # The stone slot samples no UV map (world-space tileable): park every stone loop on one
        # texel like the meadow skirt instead of the lines a planar map collapses walls to.
        for o in stone_parts:
            for d in o.data.uv_layers["UVMap"].data:
                d.uv = (0.005, 0.005)
        beds.append(bed)
        dresses.append(dress)
        roads += [bed, pt]
        parts.append(group)

    signature = bake.receiver_signature([beds[0]])
    mask_stats = None
    texel = uv.measure_texel_density(beds[0], MASK_SIZE)
    if not skip_bake:
        bake.setup_cycles(samples=64)
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        bake.bake_to(beds[0], "AO", ao, margin=12)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
        xs, ys = np.meshgrid(c, c)
        t = SIN * xs + COS * ys
        wet = np.where(np.abs(t) <= _road.ROAD_WIDTH / 2 + _road.KERB_W, 0.3, 0.7).ravel().astype(np.float32)
        bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)
        mask_stats = dress_masks(str(out / f"{piece}.masks.png"), dresses, roads)
        print(f"{piece}: masks {mask_stats}")

    lods = []
    for li, group in enumerate(parts):
        body = geom.join(group, f"{piece}_lod{li}")
        body.data.set_sharp_from_angle(angle=math.radians(30.0))
        body.parent = root
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    for k in (1, 4):
        cen = hexgeo.lane_frame(k, 0.0, _road.ROAD_WIDTH)["center"]
        export.add_socket(f"socket-lane-{k}", (*cen, 0.0), root, kind="road", edge=k, offset=0.0,
                          width=_road.ROAD_WIDTH)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, dressing=stats, maskStats=mask_stats,
                           masks={"dressPorosity": list(_river_dress.POROSITY), "stonePorosity": STONE_POROSITY,
                                  "stoneUv": list(STONE_UV), "stoneBlockPx": STONE_BLOCK, "haloPx": HALO_PX,
                                  "roadClearPx": ROAD_CLEAR_PX},
                           causeway={"halfWidthM": W, "deckZ": DECK_Z, "archHalfSpanM": ARCH_A,
                                     "springZ": SPRING_Z, "archRiseM": ARCH_RISE},
                           skipped=skip_bake)
    return root, lods
