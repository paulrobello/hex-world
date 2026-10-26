"""Stone scatter props (owner 2026-10-26: "scatter type items like rocks and reeds should not be built
into the tile ... scatter is something that can be controlled via LOD also"): the pebbles that
beach_center, beach_inner_center and beach_outer_center baked at the grass line, and the rocks and
pebbles ocean_center baked on the ocean floor, rebuilt as props the play scatter places per tile.

- `beach_pebbles`, `_b`, `_c`: one cluster of 5, 3 or 7 pebbles each (refs/beach ref-1), the same
  pebble as `_beach_dress._pebble` (a jittered icosahedron, 2r wide, 0.85 x 2r deep, 0.45-0.65 r
  high, its centre 0.2 of its half height above the ground) and the same `beach_pebble` slot colour.
- `seabed_rock`: one low rock, `seabed_pebbles`: one cluster of six pebbles (refs/ocean ref-1), the
  old `_coast.stone` (a jittered, flattened icosphere sunk 30% of its height), slots `coast_rock` and
  `coast_pebble`. The play scatter stands them on the contract bed (y = -4.0), so their top stays
  within the ocean floor's 0.10 m upward allowance (docs/kit/water-contract.md §4) at the scatter's
  largest scale (1.15): TOP_MAX_SEABED.

LODs follow each stone's screen size (1 px is about 0.12 m at 150 m and 0.31 m at 400 m, 45 degree
vertical field of view, 1080 rows): LOD0 icospheres; LOD1 (150-400 m, a pebble 1-3 px) an octahedron
per stone (a rock an icosahedron); LOD2 (beyond 400 m, a pebble 0.5-1.2 px) only the largest stones,
each a low pyramid. The one slot is drawn in every LOD, so nothing leaves LOD2. Colour-only slots plus
a uniform §9.4 masks map, so there is nothing to bake. The collider is LOD0's convex hull, whose reach
the scatter's FOOTPRINTS entry pins. Self-contained: it imports no other kit helper, so edits to the
beach or coast helpers never stale these props."""
import math
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np

from lib import bake, budgets, collider, export, geom, materials, scene, uv

MASK_SIZE = 32  # uniform, so the size only has to survive the mip chain
# The ocean floor's upward allowance (0.10 m) over the scatter's largest scale (1 + 0.15), less 2 mm.
TOP_MAX_SEABED = 0.10 / 1.15 - 0.002
# (slot, linear colour, roughness, masks R snow, G porosity, B night, A ao). The colours are the old
# centres' slots, mirrored in assets/kit/manifest.json.
BEACH = ("beach_pebble", (0.159, 0.136, 0.109), 0.6, (1.0, 0.65, 0.0, 1.0))
ROCK = ("coast_rock", (0.17, 0.16, 0.14), 0.6, (0.0, 0.0, 0.0, 1.0))
PEBBLE = ("coast_pebble", (0.105, 0.095, 0.08), 0.45, (0.0, 0.0, 0.0, 1.0))

# piece: (slot, stones, cluster radius, stone sizes, LOD2 keeps the largest n, seed)
# Beach pebble size is r (half width, 0.08-0.18 as the old centres); a seabed stone's is its full
# width w (rocks 0.55-1.0, pebbles 0.15-0.35, as `_coast.scatter`).
SPECS = {
    "beach_pebbles": dict(kind="beach", n=5, spread=0.40, size=(0.08, 0.18), keep=2),
    "beach_pebbles_b": dict(kind="beach", n=3, spread=0.36, size=(0.09, 0.17), keep=2),
    "beach_pebbles_c": dict(kind="beach", n=7, spread=0.48, size=(0.07, 0.16), keep=3),
    "seabed_rock": dict(kind="rock", n=1, spread=0.0, size=(0.8, 0.8), keep=1),
    "seabed_pebbles": dict(kind="seabed", n=6, spread=0.45, size=(0.15, 0.35), keep=3),
}


def stones(piece):
    """Deterministic [(x, y, rx, ry, rz, zc, yaw, seed)] semi-axes and centre height, largest first."""
    spec = SPECS[piece]
    rng = random.Random(piece)
    out = []
    for i in range(spec["n"]):
        for _try in range(200):
            if spec["kind"] == "beach":
                r = rng.uniform(*spec["size"])
                rx, ry, rz = r, 0.85 * r, r * rng.uniform(0.45, 0.65)
                zc = 0.2 * rz
            else:
                w = rng.uniform(*spec["size"])
                h = (rng.uniform(0.12, 0.2) if spec["kind"] == "rock" else w * rng.uniform(0.4, 0.6))
                jit = 0.12 if spec["kind"] == "rock" else 0.06
                # The top (0.2 h + 0.5 h (1 + jit)) stays within TOP_MAX_SEABED.
                h = min(h, TOP_MAX_SEABED / (0.2 + 0.5 * (1 + jit)))
                rx, ry, rz = 0.5 * w, 0.4 * w, 0.5 * h
                zc = 0.2 * h
            a, d = rng.uniform(0, 2 * math.pi), spec["spread"] * math.sqrt(rng.random()) if i else 0.0
            x, y = d * math.cos(a), d * math.sin(a)
            if i == 0 or all(math.hypot(x - s[0], y - s[1]) > rx + s[2] + 0.03 for s in out):
                out.append((x, y, rx, ry, rz, zc, rng.uniform(0, 2 * math.pi), rng.randrange(1 << 30)))
                break
        else:
            raise AssertionError(f"{piece}: no room for stone {i}")
    return sorted(out, key=lambda s: -s[2])


def _place(v, s, scale=1.0):
    """Moves a unit-frame vertex onto stone s: semi-axes, yaw, centre."""
    x, y, rx, ry, rz, zc, yaw, _ = s
    px, py, pz = v.co.x * rx * scale, v.co.y * ry * scale, v.co.z * rz * scale
    c, sn = math.cos(yaw), math.sin(yaw)
    v.co.x, v.co.y, v.co.z = x + px * c - py * sn, y + px * sn + py * c, zc + pz


def icosphere(bm, s, subdiv, jit):
    """A jittered icosphere (subdiv 1: 20 tris, 2: 80) on stone s; jitter scales each vertex."""
    rnd = random.Random(s[7])
    verts = bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0)["verts"]
    for v in verts:
        _place(v, s, 1.0 + rnd.uniform(-jit, jit))


def octahedron(bm, s):
    """8 tris on stone s's semi-axes (LOD1 pebble)."""
    pts = [(1, 0, 0), (0, 1, 0), (-1, 0, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
    vs = [bm.verts.new(p) for p in pts]
    for v in vs:
        _place(v, s)
    for k in range(4):
        a, b = vs[k], vs[(k + 1) % 4]
        bm.faces.new((a, b, vs[4]))
        bm.faces.new((b, a, vs[5]))


def pyramid(bm, s, sides=4):
    """A low pyramid (LOD2): base ring 1 cm under the origin plane on the stone's section there, apex
    at its top, no bottom (buried)."""
    x, y, rx, ry, rz, zc, yaw, _ = s
    k = math.sqrt(max(0.0, 1.0 - ((-0.01 - zc) / rz) ** 2))
    c, sn = math.cos(yaw), math.sin(yaw)
    ring = []
    for i in range(sides):
        a = 2 * math.pi * i / sides
        px, py = k * rx * math.cos(a), k * ry * math.sin(a)
        ring.append(bm.verts.new((x + px * c - py * sn, y + px * sn + py * c, -0.01)))
    apex = bm.verts.new((x, y, zc + rz))
    for i in range(sides):
        bm.faces.new((ring[i], ring[(i + 1) % sides], apex))


def lod_mesh(piece, lod, items, kind):
    """One LOD of the piece: every stone at LOD0 and LOD1, the `keep` largest at LOD2."""
    bm = bmesh.new()
    rock = kind == "rock"
    jit = 0.12 if rock else (0.1 if kind == "beach" else 0.06)
    for s in (items if lod < 2 else items[: SPECS[piece]["keep"]]):
        if lod == 0:
            icosphere(bm, s, 2 if rock else 1, jit)
        elif lod == 1:
            icosphere(bm, s, 1, jit) if rock else octahedron(bm, s)
        else:
            pyramid(bm, s, 6 if rock else 4)
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    bmesh.ops.triangulate(bm, faces=list(bm.faces))
    for f in bm.faces:
        f.material_index = 0
        f.smooth = not rock
    mesh = bpy.data.meshes.new(f"{piece}_lod{lod}")
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(f"{piece}_lod{lod}", mesh))


def build(piece, generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    spec = SPECS[piece]
    kind = spec["kind"]
    slot, color, rough, masks = {"beach": BEACH, "rock": ROCK, "seabed": PEBBLE}[kind]
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mat = materials.pbr_material(slot, (*color, 1.0), rough)
    items = stones(piece)
    lods = []
    for li in range(3):
        o = lod_mesh(piece, li, items, kind)
        o.data.materials.append(mat)
        uv.atlas_unwrap([o])  # UV0 for tangents and the uniform masks map
        o.parent = root
        lods.append(o)
    tris = [geom.tri_count(o) for o in lods]
    zs = [v.co.z for o in lods for v in o.data.vertices]
    reach = max(math.hypot(v.co.x, v.co.y) for v in lods[0].data.vertices)
    print(f"{piece}: {len(items)} stones, tris per LOD {tris}, top {max(zs):.4f} m, reach {reach:.3f} m, "
          f"widths {[round(2 * s[2], 3) for s in items]}")
    budgets.check(tris, "prop", piece)
    if kind != "beach":
        assert max(zs) <= TOP_MAX_SEABED, (piece, max(zs))
    col = collider.collider_from(lods[0], "hull", f"{piece}_collider")
    col.parent = root
    bake.write_png(MASK_SIZE, np.tile(np.asarray(masks, np.float32), (MASK_SIZE * MASK_SIZE, 1)),
                   str(out / f"{piece}.masks.png"))
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    px150, px400 = 0.12, 0.31  # metres per pixel at 150 m and 400 m
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
        device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, skipped=True,
        reason="colour-only slot; uniform masks, no Cycles bake",
        masks={"snow": masks[0], "porosity": masks[1], "night": masks[2], "ao": masks[3]},
        stones={"count": len(items), "widthsM": [round(2 * s[2], 4) for s in items],
                "topM": round(max(zs), 4), "reachM": round(reach, 4), "lod2Keeps": spec["keep"],
                "pxAt150m": [round(2 * s[2] / px150, 2) for s in items],
                "pxAt400m": [round(2 * s[2] / px400, 2) for s in items]})
    return root, lods
