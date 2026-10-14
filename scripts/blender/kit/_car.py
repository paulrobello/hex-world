"""Shared car family builder for car_sedan, car_van and car_pickup (PRD §6 runtime recolour, §7.2).

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The car faces +x (its driving direction,
the frame `hex_render::cars::car_transform` turns onto the heading), its wheels stand on z = 0 and
its pivot is the centre of its length and width. Real-metre scale (the sedan is about 4.2 m long).

Budget class: a car is kind `prop` (LOD0 <= 1,500 tris, LOD1 <= 50 %, LOD2 <= 15 %, KTX2 class
`piece`). Like the houses, every surface is a flat colour and the shape is geometry; the only map is
a unique 512² `masks` (R snow 1, G porosity, B night emissive on the head and tail lights, A baked
AO). All three LODs share the masks atlas, each baked onto its own islands. LODs are rebuilt from the
same spec at lower detail (never decimated).

Runtime recolour (owner, 2026-10-13): one model per type, painted per instance. COLOR_0 carries the
mask: rgb is the surface colour (white on paint), A is the paint mask (1 on paint, 0 on glass, tyres,
lights and trim). Bevy's StandardMaterial multiplies COLOR_0 into the base colour, so unpainted parts
keep their own colour, and `car_material.wesl` multiplies the instance paint colour (MeshTag) in where
A = 1. No texture is added for the mask. Blender's glTF exporter writes colour attributes only as
normalized UNSIGNED_SHORT/BYTE (and adds a fake white COLOR_0), which D7 and kit-compress reject, so
`float_color0` rewrites the exported GLB: the authored attribute becomes COLOR_0 as FLOAT VEC4, and
every other COLOR_n is dropped.

Three slots by roughness: `<id>_body` (paint, trim, lights, plates, hubs), `<id>_glass`, `<id>_tyre`.
Each is one batch per LOD for every car of the type, whatever its colour."""
import json
import math
import os
import pathlib
import struct
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, collider, export, geom, lod, materials, scene, uv

MASK_SIZE = 512
SAMPLES = 64
AO_DIST = 0.45
MARGIN = 4

# Surface: (sRGB colour, slot, paint mask, hooks (snow capacity, porosity, night emissive)).
SURFACES = {
    "paint": ("#ffffff", "body", 1.0, (1.0, 0.12, 0.0)),
    "trim": ("#1e1f21", "body", 0.0, (1.0, 0.2, 0.0)),
    "chrome": ("#a3a6a8", "body", 0.0, (0.8, 0.1, 0.0)),
    "plate": ("#dedcd2", "body", 0.0, (1.0, 0.1, 0.0)),
    "head": ("#f4eedf", "body", 0.0, (0.3, 0.05, 1.0)),
    "tail": ("#a3110f", "body", 0.0, (0.3, 0.05, 1.0)),
    "liner": ("#2e2d2b", "body", 0.0, (1.0, 0.45, 0.0)),
    "glass": ("#1b252c", "glass", 0.0, (0.6, 0.05, 0.0)),
    "tyre": ("#161616", "tyre", 0.0, (0.5, 0.3, 0.0)),
}
SURF = list(SURFACES)
SLOTS = {"body": 0.36, "glass": 0.05, "tyre": 0.88}  # slot -> manifest roughness


class Part:
    """bmesh accumulator: every face carries a surface index in material_index."""

    def __init__(self):
        self.bm = bmesh.new()

    def v(self, co):
        return self.bm.verts.new(Vector(co))

    def face(self, verts, surf, ref=None, out=None):
        """A face whose normal points away from `ref` (a point) or along `out` (a direction)."""
        f = self.bm.faces.new(verts)
        f.material_index = SURF.index(surf)
        f.normal_update()
        c = f.calc_center_median()
        d = Vector(out) if out is not None else c - Vector(ref)
        if f.normal.dot(d) < 0:
            f.normal_flip()
        return f

    def quad(self, pts, surf, ref=None, out=None):
        return self.face([self.v(p) for p in pts], surf, ref, out)

    def box(self, c, size, surf, skip=()):
        """Axis-aligned box at centre c; `skip` names faces to leave out (+x, -x, +y, -y, +z, -z)."""
        hx, hy, hz = (s / 2 for s in size)
        cx, cy, cz = c
        vs = {(sx, sy, sz): self.v((cx + sx * hx, cy + sy * hy, cz + sz * hz))
              for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)}
        for axis, name in ((0, "x"), (1, "y"), (2, "z")):
            for s in (1, -1):
                if f"{'+' if s > 0 else '-'}{name}" in skip:
                    continue
                corners = [k for k in vs if k[axis] == s]
                o = [k for k in (0, 1, 2) if k != axis]
                corners.sort(key=lambda k: math.atan2(k[o[1]], k[o[0]]))
                d = [0.0, 0.0, 0.0]
                d[axis] = s
                self.face([vs[k] for k in corners], surf, out=d)

    def loft(self, rings, surf_fn, cap_front, cap_rear):
        """Rings (stations of increasing x, each a closed loop of equal length) joined by quads."""
        V = [[self.v(p) for p in r] for r in rings]
        n = len(rings[0])
        for i in range(len(V) - 1):
            zc = sum(p[2] for p in rings[i] + rings[i + 1]) / (2 * n)
            for j in range(n):
                k = (j + 1) % n
                vs = (V[i][j], V[i][k], V[i + 1][k], V[i + 1][j])
                c = sum((v.co for v in vs), Vector()) / 4
                self.face(vs, surf_fn(i, j, c), ref=(c.x, 0.0, zc))
        if cap_front:
            self.face(V[-1], cap_front, out=(1, 0, 0))
        if cap_rear:
            self.face(V[0], cap_rear, out=(-1, 0, 0))
        return V

    def stack(self, loops, surf_fn, cap_top):
        """Horizontal loops (increasing z, equal length) joined by quads, capped on top."""
        V = [[self.v(p) for p in lp] for lp in loops]
        n = len(loops[0])
        cx = sum(p[0] for p in loops[0]) / n
        for i in range(len(V) - 1):
            for j in range(n):
                k = (j + 1) % n
                vs = (V[i][j], V[i][k], V[i + 1][k], V[i + 1][j])
                c = sum((v.co for v in vs), Vector()) / 4
                self.face(vs, surf_fn(i, j, c), ref=(cx, 0.0, c.z))
        if cap_top:
            self.face(V[-1], cap_top, out=(0, 0, 1))
        return V

    def wheel(self, x, y, side, r, width, n, detail):
        """Tyre and hub, axis along y; `side` is +1 (left, +y) or -1 (right)."""
        yo, yi = y + side * width / 2, y - side * width / 2
        # Scaled so the bottom flat (the contact patch) sits on z = 0.
        k_ = 1.0 / math.cos(math.pi / n)
        ring = [(k_ * math.cos(2 * math.pi * (k + 0.5) / n), k_ * math.sin(2 * math.pi * (k + 0.5) / n))
                for k in range(n)]
        centre = Vector((x, y, r))

        def loop(yy, rr):
            return [self.v((x + rr * c, yy, r + rr * s)) for c, s in ring]

        if detail == 0:
            levels = [(yi, r, None), (yo - side * 0.03, r, "tyre"), (yo, r - 0.05, "tyre"), (yo, 0.62 * r, "tyre"),
                      (yo - side * 0.035, 0.5 * r, "chrome")]
        elif detail == 1:
            levels = [(yi, r, None), (yo - side * 0.03, r, "tyre"), (yo, 0.62 * r, "tyre")]
        else:
            levels = [(yi, r, None), (yo, r, "tyre")]
        loops = [loop(yy, rr) for yy, rr, _ in levels]
        for (_, _, surf), a, b in zip(levels[1:], loops, loops[1:]):
            for k in range(n):
                kk = (k + 1) % n
                self.face((a[k], a[kk], b[kk], b[k]), surf, ref=centre)
        self.face(loops[-1], "chrome" if detail < 2 else "tyre", out=(0, side, 0))

    def obj(self, name, mats):
        me = bpy.data.meshes.new(name)
        self.bm.to_mesh(me)
        self.bm.free()
        for m in mats:
            me.materials.append(m)
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(38))
        return scene.link(bpy.data.objects.new(name, me))


def interp(pts, x):
    """Piecewise-linear y(x) through sorted (x, y) points, clamped at the ends."""
    if x <= pts[0][0]:
        return pts[0][1]
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x <= x1:
            t = (x - x0) / (x1 - x0)
            return y0 + t * (y1 - y0)
    return pts[-1][1]


def body_stations(spec, detail):
    """Station x positions: the profile's key points plus each wheel arch, thinned per LOD."""
    xs = {round(x, 4) for x, _ in spec["top"]}
    xs |= {round(x, 4) for x, _ in spec["half_width"]}
    if detail == 2:
        xs = {spec["top"][0][0], spec["top"][-1][0]} | set(spec.get("lod2_x", ()))
        return sorted(xs)
    if detail == 1:
        xs = {x for x in xs if x in {round(v, 4) for v in spec.get("lod1_x", xs)}}
    ra = spec["wheel_r"] + 0.07
    steps = (180, 150, 120, 90, 60, 30, 0) if detail == 0 else (180, 120, 60, 0)
    for xw in spec["axles"]:
        xs = {x for x in xs if abs(x - xw) > ra + 0.02}
        xs |= {round(xw + ra * math.cos(math.radians(a)), 4) for a in steps}
    return sorted(xs)


def sill_at(spec, x, detail):
    ra = spec["wheel_r"] + 0.07
    z = interp(spec["sill"], x)
    if detail < 2:
        for xw in spec["axles"]:
            dx = abs(x - xw)
            if dx <= ra + 1e-6:
                z = max(z, spec["wheel_r"] + math.sqrt(max(ra * ra - dx * dx, 0.0)))
    return z


def lower_ring(spec, x, detail):
    """Lower body cross-section at x, counter-clockwise seen from +x (screen right = +y)."""
    zt, zb, w = interp(spec["top"], x), sill_at(spec, x, detail), interp(spec["half_width"], x)
    wb = w - 0.07
    if detail > 0:
        zm = min(zb + 0.10, (zb + zt) / 2)
        return [(x, wb, zb), (x, w, zm), (x, w - 0.06, zt), (x, -(w - 0.06), zt), (x, -w, zm), (x, -wb, zb)]
    z1 = min(zb + 0.10, zb + 0.4 * (zt - zb))
    z2 = max(zt - 0.09, z1 + 0.02)
    ws = w - 0.07
    zs = zt - 0.3 * (zt - z2)
    return [(x, wb, zb), (x, w, z1), (x, w - 0.02, z2), (x, w - 0.045, zs), (x, ws, zt),
            (x, -ws, zt), (x, -(w - 0.045), zs), (x, -(w - 0.02), z2), (x, -w, z1), (x, -wb, zb)]


def greenhouse_loop(gh, t, cuts):
    """Chamfered footprint at fraction t of the greenhouse height, counter-clockwise from above.
    `cuts` are extra x stops on both sides (pillars, the van's cab/cargo split)."""
    xr = gh["rear"][0] + t * (gh["rear"][1] - gh["rear"][0])
    xf = gh["front"][0] + t * (gh["front"][1] - gh["front"][0])
    w = gh["half_width"][0] + t * (gh["half_width"][1] - gh["half_width"][0])
    c = gh["chamfer"]
    right = [(x, -w) for x in sorted(cuts) if xr + c < x < xf - c]
    left = [(x, w) for x in sorted(cuts, reverse=True) if xr + c < x < xf - c]
    pts = ([(xf - c, -w), (xf, -w + c), (xf, w - c), (xf - c, w)] + left
           + [(xr + c, w), (xr, w - c), (xr, -w + c), (xr + c, -w)] + right)
    # Segment j runs from point j to point j + 1.
    kinds = (["pillar", "front", "pillar"] + ["side"] * (len(left) + 1)
             + ["pillar", "rear", "pillar"] + ["side"] * (len(right) + 1))
    return pts, kinds


def build(spec, lod, mats):
    p = Part()
    detail = lod
    xs = body_stations(spec, detail)
    rings = [lower_ring(spec, x, detail) for x in xs]
    bed = spec.get("bed")

    def lower_surf(i, j, c):
        if bed and bed["x"][0] <= c.x <= bed["x"][1] and c.z > interp(spec["top"], c.x) - 0.02 and abs(c.y) < 0.6:
            return "liner"
        return "paint"

    p.loft(rings, lower_surf, "paint", "paint")

    # Greenhouse: loops at belt, sill band top, glass top and roof.
    gh = spec["greenhouse"]
    z0, z3 = gh["z"]
    cuts = gh.get("cuts", ())
    fr = [0.0, 0.05, 0.92, 1.0] if detail < 2 else [0.0, 1.0]
    lc = cuts if detail < 2 else ()
    kinds = greenhouse_loop(gh, 0.0, lc)[1]
    loops = [[(x, y, z0 + f * (z3 - z0)) for x, y in greenhouse_loop(gh, f, lc)[0]] for f in fr]
    glass_band = 1 if detail < 2 else 0

    def gh_surf(i, j, c):
        if i != glass_band:
            return "paint"
        k = kinds[j]
        if k == "pillar":
            return gh.get("pillar", "paint")
        if k == "front":
            return "glass"
        if k == "rear":
            return "glass" if gh.get("rear_glass", True) else "paint"
        return "glass" if any(a <= c.x <= b for a, b in gh["side_glass"]) else "paint"

    p.stack(loops, gh_surf, "paint")

    # Pillar strips over the side glass (B pillar and others), 1 cm proud.
    if detail < 2:
        for xb, half in gh.get("pillars", ()):
            for s in (1, -1):
                pts = []
                for f in (0.05, 0.92):
                    zz = z0 + f * (z3 - z0)
                    w = gh["half_width"][0] + f * (gh["half_width"][1] - gh["half_width"][0]) + 0.01
                    pts.append(((xb - half, s * w, zz), (xb + half, s * w, zz)))
                (a, b), (c_, d) = pts
                p.quad((a, b, d, c_), gh.get("pillar", "trim"), out=(0, s, 0))

    # Wheels.
    n = (12, 8, 6)[detail]
    for xw in spec["axles"]:
        for s in (1, -1):
            p.wheel(xw, s * spec["track"], s, spec["wheel_r"], spec["tyre_w"], n, detail)

    # Details: boxes (centre, size, surface, skipped faces, minimum detail level that keeps it).
    for c, size, surf, skip, keep in spec["details"](detail):
        if detail <= keep:
            p.box(c, size, surf, skip)
    return p.obj(f"{spec['piece']}_lod{lod}", mats)


def paint_attribute(o):
    """COLOR_0 source: per corner, rgb = surface colour (linear), A = paint mask."""
    me = o.data
    ca = me.color_attributes.new("paint", "FLOAT_COLOR", "CORNER")
    cols = {i: (*materials.srgb(SURFACES[s][0])[:3], SURFACES[s][2]) for i, s in enumerate(SURF)}
    for poly in me.polygons:
        col = cols[poly.material_index]
        for li in poly.loop_indices:
            ca.data[li].color = col
    me.color_attributes.active_color = ca


def float_color0(path):
    """Rewrites the GLB so COLOR_0 is the authored colour attribute as FLOAT VEC4 (D7) and no other
    COLOR_n remains. Unused accessors and buffer views are dropped and the BIN is repacked."""
    data = pathlib.Path(path).read_bytes()
    jlen = struct.unpack_from("<I", data, 12)[0]
    j = json.loads(data[20:20 + jlen])
    boff = 20 + jlen
    blen = struct.unpack_from("<I", data, boff)[0]
    blob = data[boff + 8:boff + 8 + blen]
    acc, views = j["accessors"], j["bufferViews"]
    comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
    dtypes = {5121: np.uint8, 5123: np.uint16, 5126: np.float32}
    new_data = {}  # accessor index -> float32 bytes
    for mesh in j["meshes"]:
        for prim in mesh["primitives"]:
            attrs = prim["attributes"]
            names = sorted(k for k in attrs if k.startswith("COLOR_"))
            if not names:
                continue  # the collider
            real =[k for k in names if acc[attrs[k]]["componentType"] in (5123, 5126)]
            assert len(real) == 1, (path, mesh.get("name"), names)
            a = acc[attrs[real[0]]]
            bv = views[a["bufferView"]]
            nc, dt = comps[a["type"]], np.dtype(dtypes[a["componentType"]])
            stride = bv.get("byteStride", nc * dt.itemsize)
            start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
            raw = np.frombuffer(blob, np.uint8, count=stride * (a["count"] - 1) + nc * dt.itemsize, offset=start)
            arr = np.lib.stride_tricks.as_strided(raw, (a["count"], nc * dt.itemsize), (stride, 1)).copy()
            vals = arr.view(dt).reshape(a["count"], nc).astype(np.float64)
            if a["componentType"] == 5123:
                vals /= 65535.0
            if nc == 3:
                vals = np.concatenate([vals, np.ones((a["count"], 1))], axis=1)
            for k in names:
                del attrs[k]
            acc.append({"componentType": 5126, "count": a["count"], "type": "VEC4"})
            new_data[len(acc) - 1] = np.round(vals, 6).astype("<f4").tobytes()
            attrs["COLOR_0"] = len(acc) - 1
    # Keep only referenced accessors, then only referenced views; repack the BIN in order.
    used = []
    for mesh in j["meshes"]:
        for prim in mesh["primitives"]:
            used += list(prim["attributes"].values()) + ([prim["indices"]] if "indices" in prim else [])
            for t in prim.get("targets", []):
                used += list(t.values())
    for sk in j.get("skins", []):
        if "inverseBindMatrices" in sk:
            used.append(sk["inverseBindMatrices"])
    keep = sorted(set(used))
    amap = {old: new for new, old in enumerate(keep)}
    out_bin = bytearray()
    out_views = []
    vmap = {}
    for old in keep:
        a = acc[old]
        if old in new_data:
            chunk, view = new_data[old], {"buffer": 0, "target": 34962}
        else:
            vi = a["bufferView"]
            if vi in vmap:
                a["bufferView"] = vmap[vi]
                continue
            view = dict(views[vi])
            o = view.get("byteOffset", 0)
            chunk = blob[o:o + view["byteLength"]]
            vmap[vi] = len(out_views)
        while len(out_bin) % 4:
            out_bin.append(0)
        view["byteOffset"] = len(out_bin)
        view["byteLength"] = len(chunk)
        out_bin += chunk
        a["bufferView"] = len(out_views)
        out_views.append(view)
    while len(out_bin) % 4:
        out_bin.append(0)
    j["accessors"] = [acc[i] for i in keep]
    j["bufferViews"] = out_views
    j["buffers"] = [{"byteLength": len(out_bin)}]
    for mesh in j["meshes"]:
        for prim in mesh["primitives"]:
            prim["attributes"] = {k: amap[v] for k, v in prim["attributes"].items()}
            if "indices" in prim:
                prim["indices"] = amap[prim["indices"]]
            for t in prim.get("targets", []):
                for k in t:
                    t[k] = amap[t[k]]
    for sk in j.get("skins", []):
        if "inverseBindMatrices" in sk:
            sk["inverseBindMatrices"] = amap[sk["inverseBindMatrices"]]
    js = json.dumps(j, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    total = 12 + 8 + len(js) + 8 + len(out_bin)
    glb = (struct.pack("<III", 0x46546C67, 2, total) + struct.pack("<II", len(js), 0x4E4F534A) + js
           + struct.pack("<II", len(out_bin), 0x004E4942) + bytes(out_bin))
    pathlib.Path(path).write_bytes(glb)


def build_piece(spec, generator):
    piece = spec["piece"]
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    bake_mats = []
    for s in SURF:
        col, _, _, _ = SURFACES[s]
        bake_mats.append(materials.pbr_material(f"{piece}_bake_{s}", materials.srgb(col), 0.5))
    lods = [build(spec, i, bake_mats) for i in range(3)]
    for o in lods:
        lod.triangulate(o)  # bake and export tangents agree; caps are n-gons
        lod_tris = geom.tri_count(o)
        print(f"{o.name}: {lod_tris} tris")

    uv.atlas_unwrap(lods, margin=0.012)
    signature = bake.receiver_signature(lods)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=MARGIN, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        # Hooks through one EMIT bake: each surface material's base colour temporarily carries
        # (snow, porosity, night) and is restored afterwards.
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in bake_mats}
        for m, s in zip(bake_mats, SURF):
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*SURFACES[s][3], 1.0)
        hooks = bake.new_image(f"{piece}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{piece}.hooks.tmp.png"))
        for m in bake_mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every surface has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        stats = {"aoMean": float(a.mean()) if a.size else None, "aoP05": float(np.percentile(a, 5)) if a.size else None,
                 "nightTexels": int((px[:, 2] > 0.5).sum())}
        print(f"{piece}: masks {stats}")

    # Export slots: collapse the per-surface bake materials into the three roughness slots, and write
    # the paint mask (COLOR_0 source).
    slot_names = list(SLOTS)
    slot_mats = [materials.pbr_material(f"{piece}_{s}", (1.0, 1.0, 1.0, 1.0), SLOTS[s]) for s in slot_names]
    for o in lods:
        paint_attribute(o)
        idx = [p.material_index for p in o.data.polygons]
        o.data.materials.clear()
        for m in slot_mats:
            o.data.materials.append(m)
        for p, i in zip(o.data.polygons, idx):
            p.material_index = slot_names.index(SURFACES[SURF[i]][1])
        o.parent = root
    for m in bake_mats:
        bpy.data.materials.remove(m)
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    zs = [v.co.z for v in lods[0].data.vertices]
    xs = [v.co.x for v in lods[0].data.vertices]
    ys = [v.co.y for v in lods[0].data.vertices]
    print(f"{piece}: tris per LOD {tris}, size x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f} "
          f"z {min(zs):.3f}..{max(zs):.3f}")
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.data.materials.clear()
    for a in list(col.data.color_attributes):
        col.data.color_attributes.remove(a)
    col.parent = root
    glb = out / f"{piece}.glb"
    export.export_glb(root, str(glb))
    float_color0(glb)
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=uv.measure_texel_density(lods[0], MASK_SIZE),
                           maskStats=stats, extent={"x": [min(xs), max(xs)], "y": [min(ys), max(ys)],
                                                    "z": [min(zs), max(zs)]},
                           surfaces={s: {"srgb": SURFACES[s][0], "slot": SURFACES[s][1], "paint": SURFACES[s][2]}
                                     for s in SURF},
                           skipped=skip_bake)
    return root, lods
