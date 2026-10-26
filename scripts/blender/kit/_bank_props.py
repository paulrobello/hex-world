"""Riverbank scatter props (owner, 2026-10-26: "scatter type items like rocks and reeds should not be
built into the tile"): the bank stones and reed clumps that `_river_dress` used to bake into every
river centre, as standalone pieces the runtime scatter places, a seeded 0 to 4 per river tile
(`hex-play` scatter). Refs: assets-source/refs/river (the river family's approved references).

- Stone clusters (`stones`): two to four faceted bank stones in `_river_dress`'s size range
  (semi-axes 0.25-0.55 by 0.2-0.45 by 0.15-0.3 m), each sunk `SINK` of its height, strung along
  +x within 0.2 m of the axis: the scatter lays +x along the bank's contour (`hex-play` scatter
  `contour`) and stands the pivot at the bank's height there, so every stone sits on about the
  same height of the sloping bank. One stone shape
  per LOD from the same direction-hashed jitter, so the LODs sit on one another: LOD0 a 2x subdivided
  icosphere (80 tris), LOD1 the 20-tri icosphere, LOD2 an 8-tri octahedron of the biggest stone (a
  cluster is about 1-1.5 m across, 3-5 px at 400 m, so it still reads as a speck there).
- Reed clumps (`reeds`): `_river_dress`'s close-up clumps with cattails, unchanged at LOD0 (blades,
  stalks, heads and pale spikes in the stone slot). LOD1 is `_river_dress`'s LOD1 (four flat
  back-to-back blade triangles, 3-sided stalks and heads) plus each head's 3-tri spike, so every
  LOD0 slot is drawn in LOD1. LOD2 keeps two back-to-back blade triangles per clump; the cattail and
  stone slots leave it with a measured reason (`LOD2_DROPS`).

Colour-only slots with the river centres' names and colours (river_stone, river_reed,
river_cattail), so a bank prop looks exactly like the dressing it replaces, plus a constant §9.4
masks map on the planar UV0 (R 1 snow, G wet bank, B 0, A 1): an analytic field, no Cycles bake.
Wind (TEXCOORD_1 `wind`) as `_river_dress`: U along blades and stalks, 0 on stones."""
import math
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np

from lib import bake, collider, export, geom, materials, scene, uv

import _river_dress as rd

STONE, REED, CATTAIL = rd.STONE, rd.REED, rd.CATTAIL
COLORS = {STONE: ("#8a8378", 0.8), REED: ("#5d7a35", 0.85), CATTAIL: ("#5a3a22", 0.9)}
MASK_SIZE = 16
WET = 0.9  # masks G: a bank prop stands between the waterline (1.0) and the bank top (0.8)
STONE_SUBDIV = (2, 1, 0)  # icosphere subdivisions per LOD; 0 means the octahedron
SINK = 0.4  # share of a stone's height below the pivot
LOD2_DROPS = {
    CATTAIL: rd.CATTAIL_LOD2_DROP,
    STONE: ("under a pixel at 400 m: a cattail spike is 0.011 m across and 0.12-0.19 m long "
            "(0.04 x 0.6 px at 0.31 m/px), and LOD2 has no cattail heads to carry it"),
}


def _octahedron(bm):
    vs = [bm.verts.new(c) for c in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))]
    for a, b in ((0, 2), (2, 1), (1, 3), (3, 0)):
        bm.faces.new((vs[a], vs[b], vs[4]))
        bm.faces.new((vs[b], vs[a], vs[5]))
    return vs


def _jitter(seed, d):
    """Radius factor for unit direction d: a smooth seeded wobble of +-15%, the same at every LOD."""
    r = random.Random(seed)
    k = [(r.uniform(1.5, 3.0), r.uniform(0, 2 * math.pi)) for _ in range(3)]
    return 1.0 + 0.15 * sum(math.sin(f * d[i] + ph) for i, (f, ph) in enumerate(k)) / 3.0


def stone(bm, at, size, seed, lod):
    """One faceted stone centred at `at` with semi-axes `size`, LOD `lod` (`STONE_SUBDIV`)."""
    tmp = bmesh.new()
    if STONE_SUBDIV[lod]:
        bmesh.ops.create_icosphere(tmp, subdivisions=STONE_SUBDIV[lod], radius=1.0)
    else:
        _octahedron(tmp)
    yaw = random.Random(seed + 1).uniform(0, 2 * math.pi)
    c, s = math.cos(yaw), math.sin(yaw)
    for v in tmp.verts:
        d = v.co.normalized()
        j = _jitter(seed, d)
        x, y, z = d.x * size[0] * j, d.y * size[1] * j, d.z * size[2] * j
        v.co = (at[0] + c * x - s * y, at[1] + s * x + c * y, at[2] + z)
    mesh = bpy.data.meshes.new("_stone_tmp")
    tmp.to_mesh(mesh)
    tmp.free()
    bm.from_mesh(mesh)
    bpy.data.meshes.remove(mesh)


def stone_plan(seed, count):
    """`count` stones (centre, semi-axes, seed) around the pivot, biggest first, none overlapping."""
    rnd = random.Random(seed)
    out = []
    for i in range(count):
        size = (rnd.uniform(0.25, 0.55), rnd.uniform(0.2, 0.45), rnd.uniform(0.15, 0.3))
        for _ in range(200):
            x = 0.0 if i == 0 else rnd.choice((-1, 1)) * rnd.uniform(0.45, 0.95)
            y = 0.0 if i == 0 else rnd.uniform(-0.2, 0.2)
            if all(math.hypot(x - o[0][0], y - o[0][1]) > 0.85 * (max(size[:2]) + max(o[1][:2])) for o in out):
                break
        out.append(((x, y, size[2] * (1.0 - 2.0 * SINK)), size, rnd.randrange(1 << 30)))
    out.sort(key=lambda st: -st[1][0] * st[1][1])
    return out


def _cattail_lod1_spike(bm, top):
    """`_river_dress._cattail`'s spike on a LOD1 head (3 tris, stone slot through SPIKE_LAYER)."""
    bottom, axis, hl, rad, sl, ph, wind, _lod = top
    layer = bm.faces.layers.int.get(rd.SPIKE_LAYER) or bm.faces.layers.int.new(rd.SPIKE_LAYER)
    n = len(bm.verts)
    spike = rd._tube(bm, [rd._ring(rd._add(bottom, axis, hl - 0.02), axis, 0.0055, 3, ph)],
                     apex_top=rd._add(bottom, axis, hl + sl))
    for f in spike:
        f[layer] = 1
    bm.verts.ensure_lookup_table()
    rd._wind_along(bm, bm.verts[n:], axis, wind)


def _reed_lod2(bm, at, seed):
    """Two back-to-back triangles along the clump's hero blade (LOD0's first plan blade)."""
    blades, _ = rd._plan(at, seed, True)
    _kind, az, line, hw, _tw = blades[0]
    p = (-math.sin(az), math.cos(az), 0.0)
    b = line[0][1]
    co = (rd._add(b, p, 2.5 * hw), rd._add(b, p, -2.5 * hw), rd._sample(line, 0.85)[0])
    u = rd.wind_scale(math.dist(line[0][1], line[1][1]) * (len(line) - 1))
    v = rd.wind_phase(seed, 0)
    for cs in (co, tuple(reversed(co))):
        vs = [bm.verts.new(c) for c in cs]
        bm.faces.new(vs)
        rd._set_wind(bm, [x for x, c in zip(vs, cs) if c is not co[2]], 0.0, v)
        rd._set_wind(bm, [x for x, c in zip(vs, cs) if c is co[2]], 0.85 * u, v)


def build(piece, generator, stones=0, reeds=(), seed=None):
    """A bank prop: `stones` stones from `stone_plan(seed)` and reed clumps at `reeds` [(x, y, seed)].
    Writes the GLB, export and bake reports and the masks PNG into the staging dir."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {k: materials.pbr_material(k, materials.srgb(c), r) for k, (c, r) in COLORS.items()}
    slots = ([STONE] if stones or reeds else []) + ([REED, CATTAIL] if reeds else [])
    plan = stone_plan(seed if seed is not None else piece, stones) if stones else []
    lods = []
    for li in range(3):
        bm = bmesh.new()
        rd.wind_layers(bm)
        for at, size, s in plan[: (len(plan), len(plan), 1)[li]]:
            stone(bm, at, size, s, li)
        n_stone = len(bm.faces)
        tops = []
        for x, y, s in reeds:
            at = (x, y, 0.0)
            if li < 2:
                tops += rd._reed_clump(bm, at, s, li, True)
            else:
                _reed_lod2(bm, at, s)
        n_reed = len(bm.faces)
        for t in tops:
            rd._cattail(bm, t)
            if li == 1:
                _cattail_lod1_spike(bm, t)
        bm.faces.ensure_lookup_table()
        for fi, f in enumerate(bm.faces):
            f.material_index = slots.index(STONE if fi < n_stone else (REED if fi < n_reed else CATTAIL))
            if fi < n_stone:
                f.smooth = False
        if reeds:
            rd.retag_spikes(bm, slots.index(STONE))
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(f"{piece}_lod{li}")
        bm.to_mesh(mesh)
        bm.free()
        body = scene.link(bpy.data.objects.new(f"{piece}_lod{li}", mesh))
        for name in slots:
            body.data.materials.append(mats[name])
        uv.planar_cap_uv(body)
        print(f"{piece}_lod{li}: wind U max {rd.wind_uv(body):.3f}")
        body.parent = root
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    col = collider.collider_from(lods[1], "box", f"{piece}_collider")
    col.parent = root
    n = MASK_SIZE * MASK_SIZE
    masks = np.stack((np.ones(n, np.float32), np.full(n, WET, np.float32), np.zeros(n, np.float32),
                      np.ones(n, np.float32)), axis=1)
    bake.write_png(MASK_SIZE, masks, str(out / f"{piece}.masks.png"))
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    drops = {k: v for k, v in LOD2_DROPS.items() if reeds}
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           props={"stones": len(plan), "reedClumps": len(reeds), "tris": tris},
                           skipped=True,
                           reason="no Cycles bake: colour-only slots and a constant masks map on the planar UV0",
                           masks={"snow": 1.0, "porosity": WET, "night": 0.0, "ao": 1.0},
                           lod2Drops=drops)
