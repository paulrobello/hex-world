"""wildflower_clump: a ~1 m clump of meadow grass with white daisies, yellow buttercups, purple
clover and blue cornflowers (refs: assets-source/refs/meadow_props). Scripted route (PRD §11.1),
prop kind (LOD0 <= 1,500 tris). Usage: make kit-build PIECE=wildflower_clump.

Alpha-tested cards, following the hero oak's leaf-card route (`_leaves.py`, PRD §9.1 foliage):
the tiles are drawn in numpy (`_props.clump_atlas`: grass and per-species tufts seen from the side,
flower rosettes seen from above), so no Cycles bake is needed. Each tuft is three crossed vertical
cards whose sod band starts SINK below the pivot, so the card's bottom edge is always under the turf;
every other tile edge fades to alpha 0 inside the tile. Three near-horizontal rosette cards carry
the flowers for the strategic (top-down) camera. Card normals blend the card's own normal with a
rounded clump normal (never in the card plane, so MikkTSpace tangents stay valid).

Wind (TEXCOORD_1 `wind`, project.md "Tree wind data"): U = sway, 0 at the roots rising to ~1 at the
blade tips; V = a seeded phase per tuft. The prop gate does not check it; it is authored to the
tree contract so the P2 wind pass needs no re-authoring. One piece: no variant mechanism exists, so
variety comes from the runtime's per-instance hue, scale and rotation jitter (PRD §6)."""
import math
import os
import pathlib
import random
import sys

import bpy
import numpy as np
from mathutils import Vector

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, collider, export, geom, materials, scene, uv  # noqa: E402
import _props  # noqa: E402

PIECE, TILE_PX, MASK_SIZE = "wildflower_clump", 256, 64
ATLAS = TILE_PX * _props.GRID
# The normal map ships at half the atlas: at full size it alone took 0.59 MB of the 1.5 MB piece
# budget (blade folds are low-frequency; 1024² albedo keeps the alpha edges sharp).
NORMAL_SIZE = ATLAS // 2
TUFTS, RADIUS, SINK = 32, 0.42, 0.07
MAT = f"{PIECE}_foliage"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(PIECE)
rng = random.Random(PIECE)

TUFT_TILES = {sp: [cr for cr, (kind, s, _) in _props.LAYOUT.items() if kind == "tuft" and s == sp]
              for sp in (None, *_props.SPECIES)}
TOP_TILES = [cr for cr, (kind, _, _) in _props.LAYOUT.items() if kind == "top"]
EPS = 0.5 / ATLAS


def tile_uv(cr, s, t):
    """Atlas UV of tile (col, row) at tile-local (s, t) in [0, 1]^2, inset by half a texel."""
    c, r = cr
    g = _props.GRID
    return (c / g + EPS + s * (1 / g - 2 * EPS), r / g + EPS + t * (1 / g - 2 * EPS))


# The clump layout: tuft centres in a disc, denser toward the middle, taller in the middle.
tufts = []
for i in range(TUFTS):
    a = 2 * math.pi * rng.random()
    d = RADIUS * math.sqrt(rng.random()) * (0.35 + 0.65 * rng.random())
    centre = d / RADIUS
    sp = None if rng.random() < 0.15 + 0.4 * centre else _props.SPECIES[i % 4]  # grassier rim
    tufts.append({"pos": (d * math.cos(a), d * math.sin(a)), "rot": rng.uniform(0, math.pi / 3),
                  "h": rng.uniform(0.36, 0.5) * (1.2 - 0.6 * centre), "w": rng.uniform(0.32, 0.44),
                  "lean": rng.uniform(0.04, 0.1), "sp": sp,
                  "phase": _props.unit_hash(PIECE, "tuft", i), "sway": rng.uniform(0.85, 1.0)})
tops = []
for i, cr in enumerate(TOP_TILES):
    a = 2 * math.pi * (i + rng.uniform(-0.2, 0.2)) / len(TOP_TILES)
    d = rng.uniform(0.1, 0.22)
    tops.append({"pos": (d * math.cos(a), d * math.sin(a)), "rot": rng.uniform(0, 2 * math.pi),
                 "z": rng.uniform(0.2, 0.25), "s": rng.uniform(0.5, 0.6), "tile": cr,
                 "phase": _props.unit_hash(PIECE, "top", i)})


def clump_normal(p, face_n, keep=0.45):
    """Rounded clump normal blended with the card's own normal (keeps it out of the card plane)."""
    radial = Vector((p.x, p.y, 0.0)) / (RADIUS * 1.2) + Vector((0.0, 0.0, 0.9))
    n = face_n.normalized() * keep + radial.normalized() * (1.0 - keep)
    return n.normalized()


def build_lod(name, every, cards_per_tuft, rows, wscale, hscale):
    """One LOD mesh: every `every`-th tuft with `cards_per_tuft` crossed cards of `rows` rows,
    scaled by (wscale, hscale) to hold coverage, plus every rosette card."""
    verts, tris, uvs, winds, nrms = [], [], [], [], []

    def quad_strip(cols):
        """cols: [[(pos, uv, wind, normal) bottom..top] left, right] -> triangles."""
        base = len(verts)
        n = len(cols[0])
        for col in cols:
            for p, uv_, w, nm in col:
                verts.append(tuple(p))
                uvs.append(uv_)
                winds.append(w)
                nrms.append(tuple(nm))
        for j in range(n - 1):
            a, b, c, d = base + j, base + n + j, base + n + j + 1, base + j + 1
            tris.extend(([a, b, c], [a, c, d]))

    for i, t in enumerate(tufts):
        if i % every:
            continue
        cx, cy = t["pos"]
        h, w = t["h"] * hscale, t["w"] * wscale
        pal = TUFT_TILES[t["sp"]]
        for k in range(cards_per_tuft):
            th = t["rot"] + k * math.pi / cards_per_tuft
            along = Vector((math.cos(th), math.sin(th), 0.0))
            fn = Vector((-math.sin(th), math.cos(th), 0.0))
            if fn.dot(Vector((cx, cy, 0.0))) < 0:
                fn = -fn
            # Species tufts: two cards show the species' two variants, the third is grass.
            grass = TUFT_TILES[None][(i + k) % len(TUFT_TILES[None])]
            tile = grass if (t["sp"] is None or k == 2) else pal[(i + k) % len(pal)]
            cols = []
            for s in (0.0, 1.0):
                col = []
                for j in range(rows + 1):
                    f = j / rows
                    p = Vector((cx, cy, -SINK)) + along * (s - 0.5) * w + Vector((0, 0, h * f))
                    p += fn * t["lean"] * f * f  # blades splay outward toward the tips
                    sway = t["sway"] * f ** 1.5 if p.z > 0 else 0.0
                    col.append((p, tile_uv(tile, s, f), (sway, t["phase"]), clump_normal(p, fn)))
                cols.append(col)
            # Winding: geometric normal = fn (left column first when along x fn = +z).
            if along.cross(fn).z < 0:
                cols.reverse()
            quad_strip(cols)
    for t in tops:
        cx, cy = t["pos"]
        s = t["s"] * (wscale ** 0.5)
        th = t["rot"]
        # Tilted outward with the clump's dome, so a low camera sees the rosette, not its edge.
        fn = (Vector((cx, cy, 0.0)).normalized() * 0.35 + Vector((0.0, 0.0, 1.0))).normalized()
        ay = fn.cross(Vector((math.cos(th), math.sin(th), 0.0))).normalized()
        ax = ay.cross(fn).normalized()
        cols = []
        for su in (0.0, 1.0):
            col = []
            for sv in (0.0, 1.0):
                p = Vector((cx, cy, t["z"])) + ax * (su - 0.5) * s + ay * (sv - 0.5) * s
                col.append((p, tile_uv(t["tile"], su, sv), (0.7 if p.z > 0 else 0.0, t["phase"]),
                            clump_normal(p, fn, keep=0.6)))
            cols.append(col)
        quad_strip(cols)
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], tris)
    me.validate(clean_customdata=False)
    uvl = me.uv_layers.new(name="UVMap")
    wl = me.uv_layers.new(name="wind")  # after UVMap: UVMap stays TEXCOORD_0 and active
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
    o = scene.link(bpy.data.objects.new(name, me))
    o.data.materials.append(mat)
    return o


mat = materials.pbr_material(MAT, materials.srgb("#5d7f2e"), 0.75)
lods = [build_lod(f"{PIECE}_lod0", 1, 3, 3, 1.0, 1.0),
        build_lod(f"{PIECE}_lod1", 2, 3, 2, 1.3, 1.1),
        build_lod(f"{PIECE}_lod2", 6, 2, 1, 1.7, 1.15)]
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
texel = uv.measure_texel_density(lods[0], ATLAS)
assert texel["median"] >= 256, texel

# Textures: the numpy atlas (albedo with the alpha mask in A, normal, ORM with tuft-root AO).
alb, nrm, rough, ao = _props.clump_atlas(TILE_PX, PIECE)
albedo = alb.reshape(-1, 4)
n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32).reshape(-1, 4)
orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1).reshape(-1, 4)
bake.write_png(ATLAS, albedo, str(out / f"{PIECE}.albedo.png"), non_color=False)
bake.write_png(NORMAL_SIZE, normal, str(out / f"{PIECE}.normal.png"))
bake.write_png(ATLAS, orm, str(out / f"{PIECE}.orm.png"))
# masks (linear): R snow capacity 0.5 (blades shed snow), G porosity 0.6, B no night emissive,
# A = 1 (the unique ORM.R carries the root AO).
bake.make_masks(MASK_SIZE, 0.5, 0.6, 0.0, None, str(out / f"{PIECE}.masks.png"))
coverage = {}
for (c, r), (kind, sp, k) in _props.LAYOUT.items():
    a = alb[r * TILE_PX:(r + 1) * TILE_PX, c * TILE_PX:(c + 1) * TILE_PX, 3]
    coverage[f"{kind}_{sp or 'grass'}_{k}"] = _props.coverage_by_mip(a)
coverage["atlas"] = _props.coverage_by_mip(alb[..., 3])

# Wind self-check: every LOD carries `wind` in [0, 1], U = 0 at and below the ground.
for o in lods:
    wl = o.data.uv_layers["wind"].data
    w = np.empty(len(wl) * 2)
    wl.foreach_get("uv", w)
    assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
    for loop in o.data.loops:
        if o.data.vertices[loop.vertex_index].co.z <= 0.0:
            assert wl[loop.index].uv[0] == 0.0, o.name
assert [u.name for u in lods[0].data.uv_layers] == ["UVMap", "wind"]

col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
zs = [v.co.z for v in lods[0].data.vertices]
print(f"{PIECE}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, coverage {coverage}")

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel,
                       bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                       extentZ=[min(zs), max(zs)], sinkM=SINK, tufts=TUFTS,
                       skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
