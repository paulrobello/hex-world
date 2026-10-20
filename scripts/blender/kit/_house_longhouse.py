"""House family, L-plan variant: the builder behind house_longhouse.py (PRD §6 Variety, house_cluster).

A single-storey hall-and-cross-wing cottage in the house_cottage style: a hall range with its eaves to
the front and a cross wing at its +y end whose gable faces the front, both on one front wall plane, so
the plan is an L opening to the back. The hall's roof is lower than the wing's and dies into the wing's
-y slope: every hall profile vertex ends on that slope's plane, OVERLAP inside the wing's slab, so no
eave or course end shows in the valley.

Frame and slot as _house.py: Blender (x, y, z) exports as runtime (x, z, -y), the pivot is the footprint
centre at ground level, and the front door faces +x. The walls stay inside house_cottage's wall box
(x within ±2.2, y within ±3.2) and the eaves, verges and stack inside its collider box (x within ±2.5,
y within [-3.45, 3.85]), so the piece fits a cottage slot of data/tiles/house_cluster.ron with the same
front wall line, doorstep and parking clearance (hex-sim tests/parking.rs `cottage 0/1`); the build
asserts it.

Family conventions kept (_house.py, bakery_shop.py): isotropic surfaces sample the world-space
`plaster` set tinted per slot; tile courses, framing and joinery are geometry; timber, joinery and glass
are colour-only; the one unique map is a 1024² masks atlas shared by all three LODs, each LOD baked onto
its own islands, then `fill_unbaked` and a zero-unbaked-triangle assert. LODs are rebuilt from the spec
at lower detail, never decimated. The collider is the LOD2 box: an L's hull fills only about 90% of its
bounds, and scatter::GROUND_BOXES needs 95%."""
import math
import os
import pathlib
import statistics
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from _house import (DROP, PLASTER_LIN, PLINTH_OUT, PLINTH_TOP, Z, Mesh, Wall, centroid_samples,  # noqa: F401
                    fill_unbaked, frame_matrix, roof_profile)
from lib import bake, budgets, collider, export, geom, materials, scene, uv

MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
# The house family's atlas margin and bake margin (_house.UV_MARGIN, BAKE_MARGIN): a tighter gap
# darkened thin members at mips 2-3 (house_cottage friction, 2026-10-19).
UV_MARGIN = 0.012
BAKE_MARGIN = 4
OVERLAP = 0.08  # how far each hall roof vertex ends inside the wing's slope plane (m, horizontal)
# house_cottage's footprint in Blender (x, y): its collider box (scatter::GROUND_BOXES, the LOD2 hull's
# bounds), which this piece's LOD2 and box collider must stay inside, and its LOD0 extents (the door
# step to x 2.6, the stack's oversail to y 3.9), which LOD0 and LOD1 must stay inside.
COTTAGE_BOUNDS = ((-2.5, 2.5), (-3.45, 3.85))
COTTAGE_LOD0_BOUNDS = ((-2.53, 2.6), (-3.45, 3.9))
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))


def box(mesh, center, ax, ay, az, size, slot, drop=()):
    """Box along the right-handed frame (ax, ay, az), sized `size`; faces whose outward normal points
    along any `drop` direction are left out (buried or butting faces)."""
    assert ax.cross(ay).dot(az) > 0.99, (ax, ay, az)
    res = bmesh.ops.create_cube(mesh.bm, size=1.0, matrix=frame_matrix(Vector(center), ax, ay, az, size))
    for f in list({f for v in res["verts"] for f in v.link_faces}):
        f.normal_update()
        if any(f.normal.dot(d) > 0.9 for d in drop):
            mesh.bm.faces.remove(f)
        else:
            f.material_index = mesh.idx[slot]


def beam(mesh, wall, u0, z0, u1, z1, width, proud, slot, out=0.0, sides=True):
    """A framing member on a wall plane: its front face and (sides) its two long sides. The ends are
    open: members butt into posts, rails, the eaves or each other."""
    a, b = wall.p(u0, z0, out), wall.p(u1, z1, out)
    t = (b - a).normalized()
    h = wall.n.cross(t) * (width / 2)
    f = wall.n * proud
    mesh.face([a - h + f, b - h + f, b + h + f, a + h + f], slot, wall.n)
    if sides:
        mesh.face([a + h, b + h, b + h + f, a + h + f], slot, h)
        mesh.face([a - h, a - h + f, b - h + f, b - h], slot, -h)


def sweep(mesh, slot, pts, at, s0, s1, caps):
    """A closed (x, z) roof profile swept from s0 to s1; at(x, z, s) maps it to Blender space. s0 and
    s1 are a position or one per profile vertex (an end on a slope plane). `caps`: (start, end) end
    caps, built only at a constant position. The side faces are wound outward (checked on the course
    just below the ridge)."""
    k = len(pts)
    s0 = list(s0) if isinstance(s0, (list, tuple)) else [s0] * k
    s1 = list(s1) if isinstance(s1, (list, tuple)) else [s1] * k
    r0 = [mesh.bm.verts.new(at(x, z, s)) for (x, z), s in zip(pts, s0)]
    r1 = [mesh.bm.verts.new(at(x, z, s)) for (x, z), s in zip(pts, s1)]
    faces = []
    for i in range(k):
        j = (i + 1) % k
        f = mesh.bm.faces.new((r0[i], r1[i], r1[j], r0[j]))
        f.material_index = mesh.idx[slot]
        faces.append(f)
    ridge = max(range(k), key=lambda i: pts[i][1])
    faces[ridge - 1].normal_update()
    if faces[ridge - 1].normal.z < 0:
        for f in faces:
            f.normal_flip()
    faces[ridge - 1].normal_update()
    assert faces[ridge - 1].normal.z > 0.3, f"roof sweep winds inward ({faces[ridge - 1].normal[:]})"
    d = Vector(at(0.0, 0.0, 1.0)) - Vector(at(0.0, 0.0, 0.0))
    for ring, on, sgn in ((r0, caps[0], -1.0), (r1, caps[1], 1.0)):
        if on:
            f = mesh.bm.faces.new(ring)
            f.material_index = mesh.idx[slot]
            f.normal_update()
            if f.normal.dot(d) * sgn < 0:
                f.normal_flip()


def plinth(mesh, plan, slot):
    """The stone plinth round a counter-clockwise, axis-aligned plan: each edge's outer face and its
    ledge over the wall line (no top over the floor, nothing behind the walls)."""
    n = len(plan)
    nrm = []
    for i in range(n):
        (ax, ay), (bx, by) = plan[i], plan[(i + 1) % n]
        ln = math.hypot(bx - ax, by - ay)
        nrm.append(((by - ay) / ln, -(bx - ax) / ln))
    outer = []
    for i in range(n):
        (n0x, n0y), (n1x, n1y) = nrm[i - 1], nrm[i]
        outer.append((plan[i][0] + (n0x + n1x) * PLINTH_OUT, plan[i][1] + (n0y + n1y) * PLINTH_OUT))
    for i in range(n):
        j = (i + 1) % n
        (ax, ay), (bx, by) = outer[i], outer[j]
        nv = Vector((*nrm[i], 0.0))
        mesh.face([(ax, ay, DROP), (bx, by, DROP), (bx, by, PLINTH_TOP), (ax, ay, PLINTH_TOP)], slot, nv)
        mesh.face([(ax, ay, PLINTH_TOP), (bx, by, PLINTH_TOP), (*plan[j], PLINTH_TOP), (*plan[i], PLINTH_TOP)], slot, Z)


def walls_of(spec):
    xf, xh, xw = spec["front"], spec["hall_back"], spec["wing_back"]
    hw, wy0 = spec["half_len"], spec["wing_y0"]
    return {"F": Wall((1, 0, 0), (xf, 0, 0)), "HB": Wall((-1, 0, 0), (xh, 0, 0)), "WB": Wall((-1, 0, 0), (xw, 0, 0)),
            "WR": Wall((0, 1, 0), (0, hw, 0)), "WL": Wall((0, -1, 0), (0, wy0, 0)), "HL": Wall((0, -1, 0), (0, -hw, 0))}


def chimney(mesh, spec, s, lod, hall_ridge_z):
    """A red-brick stack on the hall ridge with an oversail course and clay pots; its foot sits inside
    the roof slab, so no buried face shows."""
    cx, cy = spec["chimney"]
    sx, sy = spec["chimney_size"]
    top = hall_ridge_z + spec["chimney_above"]
    foot = hall_ridge_z - 0.45
    box(mesh, (cx, cy, (foot + top) / 2), X, Y, Z, (sx, sy, top - foot), s["brick"], drop=(-Z,))
    if lod == 2:
        return top
    box(mesh, (cx, cy, top + 0.06), X, Y, Z, (sx + 0.14, sy + 0.14, 0.12), s["brick"], drop=(-Z,))
    for py in ((-0.22, 0.22) if lod == 0 else (0.0,)):
        res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=6 if lod == 0 else 5, radius1=0.12,
                                    radius2=0.1, depth=0.4, matrix=Matrix.Translation((cx, cy + py, top + 0.32)))
        for f in list({f for v in res["verts"] for f in v.link_faces}):
            f.normal_update()
            if f.normal.z < -0.9:
                mesh.bm.faces.remove(f)
            else:
                f.material_index = mesh.idx[s["pots"]]
    return top + 0.52


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    xf, xh, xw = spec["front"], spec["hall_back"], spec["wing_back"]
    hw, wy0, eave = spec["half_len"], spec["wing_y0"], spec["eave"]
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    # Hall: x in [xh, xf], y in [-hw, wy0]; wing: x in [xw, xf], y in [wy0, hw].
    hx, hd = (xf + xh) / 2, (xf - xh) / 2
    wy, wd = (wy0 + hw) / 2, (hw - wy0) / 2
    hp, wp = math.radians(spec["hall_pitch"]), math.radians(spec["wing_pitch"])
    h_apex = eave + hd * math.tan(hp) - 0.05
    w_apex = eave + wd * math.tan(wp) - 0.05

    # Walls: one front plane over the hall and the wing, the wing's gable triangles front and back, the
    # hall's free gable, and the backs of the L.
    mesh.face([(xf, -hw, z0), (xf, hw, z0), (xf, hw, eave), (xf, -hw, eave)], s["walls"], X)
    mesh.face([(xf, wy0, eave), (xf, hw, eave), (xf, wy, w_apex)], s["walls"], X)
    mesh.face([(xw, wy0, z0), (xw, hw, z0), (xw, hw, eave), (xw, wy0, eave)], s["walls"], -X)
    mesh.face([(xw, wy0, eave), (xw, hw, eave), (xw, wy, w_apex)], s["walls"], -X)
    mesh.face([(xh, -hw, z0), (xh, wy0, z0), (xh, wy0, eave), (xh, -hw, eave)], s["walls"], -X)
    mesh.face([(xw, hw, z0), (xf, hw, z0), (xf, hw, eave), (xw, hw, eave)], s["walls"], Y)
    mesh.face([(xw, wy0, z0), (xh, wy0, z0), (xh, wy0, eave), (xw, wy0, eave)], s["walls"], -Y)
    mesh.face([(xf, -hw, z0), (xh, -hw, z0), (xh, -hw, eave), (hx, -hw, h_apex), (xf, -hw, eave)], s["walls"], -Y)
    if lod < 2:
        plinth(mesh, [(xf, -hw), (xf, hw), (xw, hw), (xw, wy0), (xh, wy0), (xh, -hw)], s["stone"])

    # Roofs. The wing: swept along +x between its two gable verges. Profile x maps to -y, which keeps
    # the (profile, sweep, z) frame of the hall's sweep along +y (bakery_shop.py's cross wing).
    thick, kick = spec["roof_thick"], spec["kick"]
    w_over, w_gover = spec["wing_overhang"], spec["wing_gable_overhang"]
    wpts, w_ridge = roof_profile(wd, wp, w_over, spec["wing_courses"] if lod == 0 else 0, kick, thick)
    w_zb = eave - w_over * math.tan(wp)
    sweep(mesh, s["roof"], wpts, lambda x, z, t: (t, wy - x, w_zb + z), xw - w_gover, xf + w_gover, (True, True))
    # The hall: swept along +y from its gable verge to the wing's -y slope plane, each vertex ending
    # OVERLAP inside it: y = (wing eave line y) + (z - wing eave z) / tan(wing pitch) + OVERLAP.
    h_over, h_gover = spec["hall_overhang"], spec["hall_gable_overhang"]
    hpts, h_ridge = roof_profile(hd, hp, h_over, spec["hall_courses"] if lod == 0 else 0, kick, thick)
    h_zb = eave - h_over * math.tan(hp)
    w_eave_y = wy - (wd + w_over)
    ends = [w_eave_y + (h_zb + z - w_zb) / math.tan(wp) + OVERLAP for _, z in hpts]
    assert min(ends) > w_eave_y, "the hall roof must end inside the wing roof"
    sweep(mesh, s["roof"], hpts, lambda x, z, t: (hx + x, t, h_zb + z), -hw - h_gover, ends, (True, False))
    hall_ridge_z = h_zb + h_ridge
    assert hall_ridge_z < w_zb + w_ridge, "the hall ridge must sit below the cross wing's"

    chimney(mesh, spec, s, lod, hall_ridge_z)

    W = walls_of(spec)
    du, dw, dh = spec["door"]
    W["F"].door(mesh, du, dw, dh, s, lod)
    for name, u, zs, ww, hh, major in spec["windows"]:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zs, ww, hh, s, detail, bars=True)
    if lod < 2:
        for name, u0, za, u1, zb, keep, width in spec["timbers"]:
            if lod == 0 or keep:
                beam(mesh, W[name], u0, za, u1, zb, width, 0.05, s["timber"])
        # Bargeboards along the wing's front verges, over the course ends. The -y one starts at the hall's
        # eave line, so it never hangs over the hall's front wall.
        run = wd + w_over
        for sg in (-1, 1):
            z_lo = w_zb - 0.05 if sg > 0 else h_zb
            u_lo = wy + sg * run * (1.0 - (z_lo - (w_zb - 0.05)) / w_ridge)
            beam(mesh, W["F"], u_lo, z_lo, wy, w_zb + w_ridge - 0.05, 0.22, 0.05, s["timber"], out=w_gover + 0.002)
    return mesh.obj(f"{spec['piece']}_lod{lod}", mats)


def density_by_slot(obj, size, slots, piece):
    """Median px/m of the masks atlas per material slot."""
    m = obj.data
    uvl = m.uv_layers["UVMap"].data
    vals = {}
    for p in m.polygons:
        if p.area < 1e-6:
            continue
        uvs = [uvl[li].uv for li in p.loop_indices]
        a = sum(abs((uvs[i] - uvs[0]).cross(uvs[i + 1] - uvs[0])) / 2 for i in range(1, len(uvs) - 1))
        vals.setdefault(slots[p.material_index][len(piece) + 1:], []).append(math.sqrt(a * size * size / p.area))
    return {k: round(statistics.median(v), 2) for k, v in sorted(vals.items())}


def footprint_errors(lods):
    """LOD2 (the collider's source) inside house_cottage's collider box, LOD0 and LOD1 inside its LOD0
    extents: the cottage slot's clearances (module docstring)."""
    err = []
    for i, o in enumerate(lods):
        (x0, x1), (y0, y1) = COTTAGE_BOUNDS if i == 2 else COTTAGE_LOD0_BOUNDS
        xs, ys, _ = zip(*[v.co[:] for v in o.data.vertices])
        if min(xs) < x0 - 1e-4 or max(xs) > x1 + 1e-4 or min(ys) < y0 - 1e-4 or max(ys) > y1 + 1e-4:
            err.append(f"{o.name}: bounds x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f}")
    return err


def build_piece(spec, generator):
    """As _house.build_piece, with the LOD2 box collider, budgets.check and the footprint check."""
    piece = spec["piece"]
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    slots = list(spec["materials"])
    mats = []
    for name in slots:
        col, rough = spec["materials"][name]
        mats.append(materials.pbr_material(name, (*col, 1.0), rough))
    lods = [build(spec, i, slots, mats) for i in range(3)]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris} (LOD1 {tris[1] / tris[0]:.3f}, LOD2 {tris[2] / tris[0]:.3f})")
    budgets.check(tris, "prop", piece)
    bad = footprint_errors(lods)
    assert not bad, f"outside house_cottage's footprint: {bad}"

    uv.atlas_unwrap(lods, margin=UV_MARGIN)
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in lods}
    per_slot = {f"lod{i}": density_by_slot(o, MASK_SIZE, slots, piece) for i, o in enumerate(lods)}
    print(f"{piece}: masks {MASK_SIZE}² density per LOD {density}")
    print(f"{piece}: masks px/m per slot {per_slot}")
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
        # Hooks through one EMIT bake: each material's base colour temporarily carries
        # (snow, porosity, night) and is restored afterwards.
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*spec["hooks"][m.name], 1.0)
        hooks = bake.new_image(f"{piece}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{piece}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        px, filled = fill_unbaked(px, lods, MASK_SIZE, [spec["hooks"][n] for n in slots])
        bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        samples = centroid_samples(lods, px, MASK_SIZE, slots, piece)
        stats = {"aoMean": float(a.mean()) if a.size else None, "aoP05": float(np.percentile(a, 5)) if a.size else None,
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": samples}
        print(f"{piece}: masks {stats}")
        bad = {k: v["unbakedTris"] for k, v in samples.items() if v["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

    for o in lods:
        o.data.shade_flat()
        o.parent = root
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{piece}_lod0"], masksDensityPxPerM=density,
                           masksDensityBySlot=per_slot, uvMargin=UV_MARGIN, maskStats=stats, skipped=skip_bake)
    return root, lods
