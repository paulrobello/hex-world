"""Shared builder for the P3 train: train_engine (a six-wheeled steam tank engine) and
train_carriage (a short four-wheeled passenger carriage); the consist is the engine plus two
carriages (owner-approved refs `assets-source/refs/train`, A17: "Steam tank engine as drawn; runtime
colour on the body, like the cars; two carriages").

Frame and pivot, as the cars (`_car.py`): Blender (x, y, z) exports as runtime (x, z, -y). A vehicle
faces +x (the engine's smokebox end), its pivot is the centre of its length and width, and z = 0 is
the wheel tread's contact line, i.e. the rail top: the runtime stands it at the rail pieces' rail top
(`_rail.RAIL_TOP` = 0.46 m above the cap), as `hex_render::cars` stands a car on ROAD_SURFACE_Y.

Rail fit (`_rail.py`): gauge 1.6 m between the rail heads' inner faces (|y| = 0.80), heads 0.08 m wide
(centres at |y| = 0.84). The treads span |y| in [TREAD_IN, TREAD_OUT] over the whole head, and each
flange sits inside the rails (|y| in [FLANGE_IN, TREAD_IN], outer face below 0.80) and dips FLANGE_D
below the rail top. Wheels and rods are static mesh (no rig, no spin data; the cars carry none).

Runtime recolour, exactly as the cars (PRD §6, `_car.py`): COLOR_0 FLOAT VEC4 per corner, rgb the
surface's own colour (white on paint) and A the paint mask (1 on paint only), written by
`_car.float_color0` after export. Three slots by roughness: `<id>_body` (paint and the dark
charcoal parts), `<id>_glass` (windows, night-lit), `<id>_tyre` (wheels, rods, springs: steel). A
unique 1024² `masks` (R snow, G porosity, B night, A AO) is baked across all three LODs' islands, and
texels the bake misses under thin faces are filled per slot (`_house.fill_unbaked`).

Curve clearance: the consist must take the rail bend (`_rail.BEND_R` = 8 m). Every LOD0 vertex is
checked with the vehicle posed on the arc (outer axles on the track centreline): its distance from
the arc must stay within CLEAR_MAX of R, so a car body's corners stay inside ±2.5 m of the track."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import _car
import _house
from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv

MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
UV_MARGIN = 0.010
BAKE_MARGIN = 4

GAUGE = 1.6
RAIL_HEAD_W = 0.08
RAIL_TOP = 0.46  # _rail.RAIL_TOP: where the runtime stands z = 0
BEND_R = 8.0  # _rail.BEND_R
CLEAR_MAX = 2.5
FLANGE_IN, TREAD_IN, TREAD_OUT = 0.755, 0.785, 0.915
FLANGE_D = 0.035

# Surface: (sRGB colour, slot, paint mask, hooks (snow capacity, porosity, night emissive)).
SURFACES = {
    "paint": ("#ffffff", "body", 1.0, (1.0, 0.12, 0.0)),
    "dark": ("#2c2d2f", "body", 0.0, (1.0, 0.2, 0.0)),
    "roof": ("#3b3d40", "body", 0.0, (1.0, 0.25, 0.0)),
    "steel": ("#8a8d90", "body", 0.0, (0.8, 0.1, 0.0)),
    "brass": ("#a8854f", "body", 0.0, (0.8, 0.1, 0.0)),
    "coal": ("#141414", "body", 0.0, (1.0, 0.45, 0.0)),
    "lamp": ("#f4eedf", "body", 0.0, (0.3, 0.05, 1.0)),
    "glass": ("#4a3f31", "glass", 0.0, (0.6, 0.05, 1.0)),
    "wheel": ("#1c1c1d", "tyre", 0.0, (0.6, 0.25, 0.0)),
    "rim": ("#55575a", "tyre", 0.0, (0.6, 0.2, 0.0)),
}
SURF = list(SURFACES)
SLOTS = {"body": 0.45, "glass": 0.05, "tyre": 0.6}  # slot -> manifest roughness


class Part:
    """bmesh accumulator: every face carries a surface index in material_index."""

    def __init__(self):
        self.bm = bmesh.new()

    def v(self, co):
        return self.bm.verts.new(Vector(co))

    def face(self, verts, surf, ref=None, out=None):
        f = self.bm.faces.new(verts)
        f.material_index = SURF.index(surf)
        f.normal_update()
        d = Vector(out) if out is not None else f.calc_center_median() - Vector(ref)
        if f.normal.dot(d) < 0:
            f.normal_flip()
        return f

    def quad(self, pts, surf, out):
        return self.face([self.v(p) for p in pts], surf, out=out)

    def box(self, lo, hi, surf, skip=()):
        """Axis-aligned box from corner lo to corner hi; `skip` names faces to leave out."""
        c = [(a + b) / 2 for a, b in zip(lo, hi)]
        vs = {(sx, sy, sz): self.v((lo[0] if sx < 0 else hi[0], lo[1] if sy < 0 else hi[1],
                                    lo[2] if sz < 0 else hi[2]))
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
        return c

    def cyl(self, axis, a0, a1, centre, r, n, surf, caps=(True, True), cap_surf=None, r1=None, phase=0.5):
        """Prism of n sides along `axis` (0 x, 1 y, 2 z) from a0 to a1 through `centre` (the other two
        coordinates), radius r (r1 at a1 for a taper). caps: (at a0, at a1)."""
        o = [k for k in (0, 1, 2) if k != axis]
        r1 = r if r1 is None else r1

        def ring(at, rr):
            pts = []
            for k in range(n):
                t = 2 * math.pi * (k + phase) / n
                p = [0.0, 0.0, 0.0]
                p[axis] = at
                p[o[0]] = centre[0] + rr * math.cos(t)
                p[o[1]] = centre[1] + rr * math.sin(t)
                pts.append(self.v(p))
            return pts

        A, B = ring(a0, r), ring(a1, r1)
        mid = [0.0, 0.0, 0.0]
        mid[axis] = (a0 + a1) / 2
        mid[o[0]], mid[o[1]] = centre
        for k in range(n):
            kk = (k + 1) % n
            self.face((A[k], A[kk], B[kk], B[k]), surf, ref=self._axis_ref(axis, A[k], A[kk], B[k], B[kk], centre, o))
        sgn = 1 if a1 > a0 else -1
        for want, loop, s in ((caps[0], A, -sgn), (caps[1], B, sgn)):
            if want:
                d = [0.0, 0.0, 0.0]
                d[axis] = s
                self.face(loop, cap_surf or surf, out=d)

    @staticmethod
    def _axis_ref(axis, *args):
        vs, centre, o = args[:4], args[4], args[5]
        c = sum((v.co for v in vs), Vector()) / 4
        p = list(c)
        p[o[0]], p[o[1]] = centre
        return p

    def wheel(self, x, side, r, n, detail):
        """A flanged rail wheel, axis along y; side +1 (left, +y) or -1. The tread circle is scaled so
        a flat sits on z = 0; the flange (inside the rails) dips FLANGE_D below it."""
        k_ = 1.0 / math.cos(math.pi / n)
        th = [-math.pi / 2 + math.pi / n + 2 * math.pi * k / n for k in range(n)]  # a flat at the bottom
        ring = [(k_ * math.cos(t), k_ * math.sin(t)) for t in th]
        centre = Vector((x, 0.0, r))

        def loop(yy, rr):
            return [self.v((x + rr * c, side * yy, r + rr * s)) for c, s in ring]

        rf = r + FLANGE_D
        if detail == 0:
            levels = [(FLANGE_IN, rf, None), (TREAD_IN, rf, "rim"), (TREAD_IN, r, "rim"), (TREAD_OUT, r, "rim")]
        else:  # no flange beyond the LOD switch: a plain tread cylinder
            levels = [(TREAD_IN, r, None), (TREAD_OUT, r, "rim")]
        loops = [loop(yy, rr) for yy, rr, _ in levels]
        for (_, _, surf), a, b in zip(levels[1:], loops, loops[1:]):
            for k in range(n):
                kk = (k + 1) % n
                self.face((a[k], a[kk], b[kk], b[k]), surf, ref=centre)
        self.face(loops[0], "wheel", out=(0, -side, 0))
        self.face(loops[-1], "wheel", out=(0, side, 0))

    def arch(self, x0, x1, half_w, z_eave, z_crown, segs, surf, end_surf=None, caps=(True, True)):
        """A curved roof: a circular-arc section from eave to eave (z_eave at |y| = half_w, z_crown at
        y = 0) extruded along x, with flat end gables down to z_eave."""
        s = half_w
        h = z_crown - z_eave
        rad = (s * s + h * h) / (2 * h)
        zc = z_crown - rad
        a = math.asin(s / rad)
        prof = [(rad * math.sin(-a + 2 * a * i / segs), zc + rad * math.cos(-a + 2 * a * i / segs))
                for i in range(segs + 1)]
        A = [self.v((x0, y, z)) for y, z in prof]
        B = [self.v((x1, y, z)) for y, z in prof]
        for i in range(segs):
            ym, zm = (prof[i][0] + prof[i + 1][0]) / 2, (prof[i][1] + prof[i + 1][1]) / 2
            self.face((A[i], A[i + 1], B[i + 1], B[i]), surf, out=(0.0, ym, zm - zc))
        for want, loop, d in ((caps[0], A, -1), (caps[1], B, 1)):
            if want:
                self.face(loop, end_surf or surf, out=(d, 0, 0))

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


def paint_attribute(o):
    """COLOR_0 source: per corner, rgb = surface colour (linear), A = paint mask (as `_car`)."""
    me = o.data
    ca = me.color_attributes.new("paint", "FLOAT_COLOR", "CORNER")
    cols = {i: (*materials.srgb(SURFACES[s][0])[:3], SURFACES[s][2]) for i, s in enumerate(SURF)}
    for poly in me.polygons:
        col = cols[poly.material_index]
        for li in poly.loop_indices:
            ca.data[li].color = col
    me.color_attributes.active_color = ca


def arc_pose(a):
    """Distance from the curve centre to the vehicle's centreline midpoint when its outer axles (at
    x = ±a) sit on the BEND_R track centreline."""
    return math.sqrt(BEND_R * BEND_R - a * a)


def clearance(obj, a):
    """Max |distance from the arc centre - BEND_R| over obj's vertices, posed on the bend both ways."""
    d = arc_pose(a)
    worst = 0.0
    for v in obj.data.vertices:
        x, y = v.co.x, v.co.y
        for s in (1, -1):
            worst = max(worst, abs(math.hypot(x, d + s * y) - BEND_R))
    return worst


def wheel_fit_errors(objs):
    """Wheel faces (rim/wheel surfaces): flanges inside the rail heads, treads over them."""
    e = []
    for o in objs:
        for p in o.data.polygons:
            s = SURF[p.material_index]
            if s not in ("rim", "wheel"):
                continue
            for vi in p.vertices:
                co = o.data.vertices[vi].co
                if co.z < -1e-4 and abs(co.y) > GAUGE / 2 - 0.005:
                    e.append(f"{o.name}: wheel vertex below rail top outside the flangeway at |y| {abs(co.y):.3f}")
                    return e
    return e


def build_piece(spec, generator):
    """spec: piece, kind, axles (x of every axle), wheel_r, build(part, detail) adding the body."""
    piece = spec["piece"]
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    bake_mats = [materials.pbr_material(f"{piece}_bake_{s}", materials.srgb(SURFACES[s][0]), 0.5) for s in SURF]
    lods = []
    for detail in range(3):
        p = Part()
        spec["build"](p, detail)
        if detail < 2:
            n = (10, 6)[detail]
            for xw in spec["axles"]:
                for s in (1, -1):
                    p.wheel(xw, s, spec["wheel_r"], n, detail)
        o = p.obj(f"{piece}_lod{detail}", bake_mats)
        lod.triangulate(o)
        lods.append(o)
    tris = [geom.tri_count(o) for o in lods]
    lod0_max, r1, r2 = budgets.lod_budget(spec["kind"], piece)
    xs = [v.co.x for v in lods[0].data.vertices]
    ys = [v.co.y for v in lods[0].data.vertices]
    zs = [v.co.z for v in lods[0].data.vertices]
    a = max(abs(x) for x in spec["axles"])
    clear = [clearance(o, a) for o in lods]
    fit = wheel_fit_errors(lods)
    geometry = {
        "lengthM": max(xs) - min(xs), "widthM": max(ys) - min(ys), "heightAboveRailM": max(zs),
        "axlesX": list(spec["axles"]), "wheelbaseM": 2 * a, "wheelRadiusM": spec["wheel_r"],
        "overhangM": [max(xs) - a, -min(xs) - a],
        "bendR": BEND_R, "arcCentreToMidpointM": arc_pose(a), "sagittaM": BEND_R - arc_pose(a),
        "maxOffTrackCentreOnBendM": clear,
        "extent": {"x": [min(xs), max(xs)], "y": [min(ys), max(ys)], "z": [min(zs), max(zs)]},
    }
    if len(spec["axles"]) > 2:
        mid = min(abs(x) for x in spec["axles"])
        geometry["innerAxleOffRailOnBendM"] = math.hypot(mid, arc_pose(a)) - BEND_R
    print(f"{piece}: tris per LOD {tris} (LOD0 max {lod0_max}), geometry {geometry}, wheel fit {fit}")
    assert tris[0] <= lod0_max and tris[1] <= r1 * tris[0] and tris[2] <= r2 * tris[0], tris
    assert max(clear) <= CLEAR_MAX, ("body leaves the ±2.5 m envelope on the 8 m bend", clear)
    assert not fit, fit
    for o in lods:
        idx = {p.material_index for p in o.data.polygons}
        assert SURF.index("paint") in idx, (o.name, "no painted face")

    uv.atlas_unwrap(lods, margin=UV_MARGIN)
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in lods}
    print(f"{piece}: masks {MASK_SIZE}² density per LOD {density}")
    signature = bake.receiver_signature(lods)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in bake_mats}
        for m, s in zip(bake_mats, SURF):
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*SURFACES[s][3], 1.0)
        hooks = bake.new_image(f"{piece}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{piece}.hooks.tmp.png"))
        for m in bake_mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        px, filled = _house.fill_unbaked(px, lods, MASK_SIZE, [SURFACES[s][3] for s in SURF])
        bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every surface has porosity >= 0.05; empty texels stay 0
        a_ = px[covered, 3]
        samples = _house.centroid_samples(lods, px, MASK_SIZE, [f"{piece}_{s}" for s in SURF], piece)
        stats = {"aoMean": float(a_.mean()) if a_.size else None,
                 "aoP05": float(np.percentile(a_, 5)) if a_.size else None,
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": samples}
        print(f"{piece}: masks {stats}")
        bad = {k: v["unbakedTris"] for k, v in samples.items() if v["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

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
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.data.materials.clear()
    for c in list(col.data.color_attributes):
        col.data.color_attributes.remove(c)
    # A box from LOD2 keeps no usable UVs (a hull over LOD2 gave zero tangents on the engine): an oblique
    # planar UV, flat shaded (pitfalls.md, Water beds: "A hull collider ... zero tangents").
    col.data.shade_flat()
    if "UVMap" not in col.data.uv_layers:
        col.data.uv_layers.new(name="UVMap")
    for poly in col.data.polygons:
        for li in poly.loop_indices:
            co = col.data.vertices[col.data.loops[li].vertex_index].co
            col.data.uv_layers["UVMap"].data[li].uv = (0.05 * (co.x + 0.37 * co.z), 0.05 * (co.y + 0.61 * co.z + 0.23 * co.x))
    col.parent = root
    glb = out / f"{piece}.glb"
    export.export_glb(root, str(glb))
    _car.float_color0(glb)
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{piece}_lod0"], masksDensityPxPerM=density,
                           uvMargin=UV_MARGIN, maskStats=stats, geometry=geometry, lodTris=tris,
                           surfaces={s: {"srgb": SURFACES[s][0], "slot": SURFACES[s][1], "paint": SURFACES[s][2]}
                                     for s in SURF},
                           skipped=skip_bake)
    return root, lods
