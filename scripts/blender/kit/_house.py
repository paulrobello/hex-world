"""Shared house family builder for house_cottage and house_twostorey (PRD §5.4 tile 3, §6 props).

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the footprint centre at ground
level (z = 0), and the front door faces +x (yaw 0 in data/tiles/house_cluster.ron, whose yaw turns
the way edges run). Walls run from DROP (below the meadow cap's deepest relief, -0.25 m) so a house
never floats over a cap dip.

Budget class: houses are props (PRD §6 lists houses as props; §10 prop LOD0 <= 1,500 tris), so
kind `prop`, LOD1 <= 50 %, LOD2 <= 15 %, KTX2 class `piece`. `hero` (12k) is for landmarks and the
gate rejects it for a prop.

Look (owner-approved refs, assets-source/refs/house_cluster): stylized-real, real-metre scale on
slightly exaggerated proportions. Every isotropic surface (walls, stone, roof clay or slate) samples
the world-space `plaster` set tinted by a manifest colour; directional detail (tile courses, timber
framing, window frames) is geometry, because a world-space tile does not turn with the instance.
Timber, joinery and glass are colour-only slots. The only unique map is `masks` (1024², UV0 atlas):
R snow 1, G porosity per surface, B night emissive on the glass only (§9.4: the shader adds
base colour x B x night x 4, so the glass colour is a warm mid tone), A baked AO.

LODs are rebuilt from the same spec at lower detail (never decimated): LOD1 drops tile courses,
sills, bars and minor timbers; LOD2 is the wall shell, a plain roof, the chimney and flat glass.
All three LODs share one UV0 atlas, each with its own islands, and each LOD's AO and hooks are
baked onto its own islands (the first bake clears the image, later ones add to it). A nearest-face
UV transfer from LOD0 was tried and failed: LOD faces whose corners sit on edges shared by two LOD0
islands took UVs from different islands, stretched across the atlas, and lost their night glow
(friction log)."""
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from lib import bake, collider, export, geom, materials, scene, uv

DROP = -0.35
PLINTH_TOP, PLINTH_OUT = 0.32, 0.04
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
# Linear albedo of the plaster set (mean of plaster.albedo.png), so Blender renders of tinted
# tileable slots show the runtime colour: runtime = manifest colour x set albedo.
PLASTER_LIN = 0.66

Z = Vector((0.0, 0.0, 1.0))


class Mesh:
    """bmesh accumulator with per-face slot indices."""

    def __init__(self, slots):
        self.bm = bmesh.new()
        self.idx = {n: i for i, n in enumerate(slots)}

    def face(self, pts, slot, expect=None):
        f = self.bm.faces.new([self.bm.verts.new(p) for p in pts])
        f.material_index = self.idx[slot]
        if expect is not None:
            f.normal_update()
            if f.normal.dot(expect) < 0:
                f.normal_flip()
        return f

    def box(self, m, slot, drop_back=None):
        """Unit cube transformed by matrix m; drop_back: drop the face whose normal points along
        this world direction (the face buried in a wall)."""
        res = bmesh.ops.create_cube(self.bm, size=1.0, matrix=m)
        faces = {f for v in res["verts"] for f in v.link_faces}
        for f in faces:
            f.material_index = self.idx[slot]
        if drop_back is not None:
            for f in list(faces):
                f.normal_update()
                if f.normal.dot(drop_back) > 0.9:
                    self.bm.faces.remove(f)

    def obj(self, name, mats):
        bmesh.ops.remove_doubles(self.bm, verts=self.bm.verts, dist=1e-5)
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        o = scene.link(bpy.data.objects.new(name, mesh))
        for m in mats:
            o.data.materials.append(m)
        return o


def frame_matrix(origin, t, up, n, size):
    """Matrix placing a unit cube with its x along t, y along up, z along n, scaled by size."""
    r = Matrix((t, up, n)).transposed()
    s = Matrix.Diagonal((*size, 1.0))
    m = r.to_4x4() @ s
    m.translation = origin
    return m


class Wall:
    """A wall face: outward normal n, horizontal tangent t (t x Z = -n... chosen so (t, Z, n) is
    right-handed), and the plane offset d (the face lies at origin + u t + z Z)."""

    def __init__(self, n, origin):
        self.n = Vector(n)
        self.t = Z.cross(self.n)
        self.o = Vector(origin)

    def p(self, u, z, out=0.0):
        return self.o + self.t * u + Z * z + self.n * out

    def beam(self, mesh, u0, z0, u1, z1, width, proud, slot):
        """A timber member between two wall points (any angle), proud of the wall."""
        a, b = self.p(u0, z0), self.p(u1, z1)
        d = b - a
        ln = d.length
        t = d / ln
        up = self.n.cross(t)
        mid = (a + b) / 2 + self.n * (proud / 2)
        mesh.box(frame_matrix(mid, t, up, self.n, (ln, width, proud)), slot, drop_back=-self.n)

    def window(self, mesh, u, z0, w, h, s, detail, bars=True):
        """Casement window centred at u, sill at z0: glass, a frame ring, sill and (bars) glazing bars."""
        f, p, g = 0.09, 0.06, 0.015
        u0, u1, z1 = u - w / 2, u + w / 2, z0 + h
        mesh.face([self.p(u0, z0, g), self.p(u1, z0, g), self.p(u1, z1, g), self.p(u0, z1, g)], s["glass"], self.n)
        if detail < 2:
            outer = [(u0 - f, z0 - f), (u1 + f, z0 - f), (u1 + f, z1 + f), (u0 - f, z1 + f)]
            inner = [(u0, z0), (u1, z0), (u1, z1), (u0, z1)]
            for i in range(4):
                j = (i + 1) % 4
                (ou0, oz0), (ou1, oz1) = outer[i], outer[j]
                (iu0, iz0), (iu1, iz1) = inner[i], inner[j]
                mesh.face([self.p(ou0, oz0, p), self.p(ou1, oz1, p), self.p(iu1, iz1, p), self.p(iu0, iz0, p)], s["frame"], self.n)
                c = Vector(((ou0 + ou1) / 2 - u, 0, (oz0 + oz1) / 2 - (z0 + z1) / 2))
                side_out = (self.t * c.x + Z * c.z).normalized()
                mesh.face([self.p(ou0, oz0, 0), self.p(ou1, oz1, 0), self.p(ou1, oz1, p), self.p(ou0, oz0, p)], s["frame"], side_out)
                mesh.face([self.p(iu0, iz0, p), self.p(iu1, iz1, p), self.p(iu1, iz1, g), self.p(iu0, iz0, g)], s["frame"], -side_out)
        if detail == 0:
            sill = self.p(u, z0 - f - 0.04, 0.07)
            mesh.box(frame_matrix(sill, self.t, Z, self.n, (w + 2 * f + 0.12, 0.08, 0.14)), s["sill"], drop_back=-self.n)
            for bu, bz, bw, bh in ((u, (z0 + z1) / 2, 0.05, h), (u, z0 + h * 0.62, w, 0.05)) if bars else ():
                c = self.p(bu, bz, 0.035)
                mesh.box(frame_matrix(c, self.t, Z, self.n, (bw, bh, 0.04)), s["frame"], drop_back=-self.n)

    def door(self, mesh, u, w, h, s, detail):
        f, p, g = 0.1, 0.07, 0.03
        u0, u1 = u - w / 2, u + w / 2
        mesh.face([self.p(u0, PLINTH_TOP, g), self.p(u1, PLINTH_TOP, g), self.p(u1, h, g), self.p(u0, h, g)], s["door"], self.n)
        if detail < 2:
            for a, b, c, d in ((u0 - f, PLINTH_TOP, u0, h), (u1, PLINTH_TOP, u1 + f, h), (u0 - f, h, u1 + f, h + f)):
                mid = self.p((a + c) / 2, (b + d) / 2, p / 2)
                mesh.box(frame_matrix(mid, self.t, Z, self.n, (c - a, d - b, p)), s["frame"], drop_back=-self.n)
        if detail < 2:
            step = self.p(u, (DROP + 0.2) / 2, 0.2)
            mesh.box(frame_matrix(step, self.t, Z, self.n, (w + 0.4, 0.2 - DROP, 0.4)), s["sill"], drop_back=-self.n)
        if detail == 0:
            # A small glazed light in the door (ref-2).
            mesh.face([self.p(u - 0.13, h - 0.55, g + 0.005), self.p(u + 0.13, h - 0.55, g + 0.005),
                       self.p(u + 0.13, h - 0.3, g + 0.005), self.p(u - 0.13, h - 0.3, g + 0.005)], s["glass"], self.n)


def roof_profile(half_d, pitch, overhang, courses, kick, thick):
    """Closed (x, z) cross-section of a pitched roof with its ridge along y: the two slopes as tile
    courses (each course's butt stands `kick` proud of the course below it) over a slab `thick`
    deep. z is measured from the eave line. Returns (points, ridge_z)."""
    run = half_d + overhang
    ridge = run * math.tan(pitch)
    slope = run / math.cos(pitch)
    nrm = Vector((math.sin(pitch), math.cos(pitch)))  # front slope outward normal in (x, z)
    up = Vector((-math.cos(pitch), math.sin(pitch)))  # up-slope direction from the front eave
    eave = Vector((run, 0.0))
    front = []
    n = max(courses, 1)
    c = slope / n
    for k in range(n):
        base = eave + up * (k * c)
        front.append(base + nrm * (kick if courses else 0.0))
        if courses and k < n - 1:
            front.append(eave + up * ((k + 1) * c))
    # The last course runs up to the ridge point, shared by both slopes (listing the ridge on each
    # slope would duplicate it and collapse the end caps when the vertices weld).
    top = Vector((0.0, ridge + (kick if courses else 0.0)))
    front_pts = [eave - nrm * thick] + front
    back_pts = [Vector((-q.x, q.y)) for q in reversed(front_pts)]
    under_ridge = Vector((0.0, ridge - thick / math.cos(pitch)))
    pts = front_pts + [top] + back_pts + [under_ridge]
    return [(q.x, q.y) for q in pts], ridge


def build(spec, lod, slots, mats):
    """One LOD of the house described by spec (see house_cottage.py). Returns the object."""
    s = spec["slots"]
    d, w, eave = spec["depth"], spec["width"], spec["eave"]
    pitch = math.radians(spec["pitch"])
    hd, hw = d / 2, w / 2
    mesh = Mesh(slots)
    detail = lod
    ridge = (hd + spec["overhang"]) * math.tan(pitch)
    gable_top = eave + hd * math.tan(pitch) - 0.05

    # Walls: front/back rectangles to the eave, gable pentagons to under the ridge.
    fl, fr, bl, br = (hd, -hw), (hd, hw), (-hd, -hw), (-hd, hw)
    mesh.face([(*fl, DROP), (*fr, DROP), (*fr, eave), (*fl, eave)], s["walls"], Vector((1, 0, 0)))
    mesh.face([(*bl, DROP), (*br, DROP), (*br, eave), (*bl, eave)], s["walls"], Vector((-1, 0, 0)))
    for y, ny in ((hw, 1), (-hw, -1)):
        mesh.face([(hd, y, DROP), (-hd, y, DROP), (-hd, y, eave), (0.0, y, gable_top), (hd, y, eave)], s["walls"], Vector((0, ny, 0)))
    if detail < 2:
        # Stone plinth, proud of the walls (no top face over the wall line, no bottom).
        m = frame_matrix(Vector((0, 0, (DROP + PLINTH_TOP) / 2)), Vector((1, 0, 0)), Vector((0, 1, 0)), Z,
                         (d + 2 * PLINTH_OUT, w + 2 * PLINTH_OUT, PLINTH_TOP - DROP))
        mesh.box(m, s["stone"], drop_back=-Z)

    # Roof: the courses profile extruded along y, with gable overhang.
    pts, ridge = roof_profile(hd, pitch, spec["overhang"], spec["courses"] if detail == 0 else 0,
                              spec["kick"], spec["roof_thick"])
    y0, y1 = -hw - spec["gable_overhang"], hw + spec["gable_overhang"]
    zb = eave - spec["overhang"] * math.tan(pitch)
    ring0 = [mesh.bm.verts.new((x, y0, zb + z)) for x, z in pts]
    ring1 = [mesh.bm.verts.new((x, y1, zb + z)) for x, z in pts]
    k = len(pts)
    for i in range(k):
        j = (i + 1) % k
        # The profile runs counter-clockwise in (x, z) and the sweep runs along +y, a left-handed
        # frame, so this winding puts every swept face's normal outward.
        f = mesh.bm.faces.new((ring0[i], ring1[i], ring1[j], ring0[j]))
        f.material_index = mesh.idx[s["roof"]]
    for ring, ny in ((ring0, -1), (ring1, 1)):
        f = mesh.bm.faces.new(ring)
        f.material_index = mesh.idx[s["roof"]]
        f.normal_update()
        if f.normal.y * ny < 0:
            f.normal_flip()

    # Chimney stack on a gable end, through the verge, with pots.
    cx, cy_side = spec["chimney"]
    cy = cy_side * (hw + 0.3)
    top = zb + ridge + spec["chimney_above"]
    m = frame_matrix(Vector((cx, cy, (DROP + top) / 2)), Vector((1, 0, 0)), Vector((0, 1, 0)), Z,
                     (0.95, 0.7, top - DROP))
    mesh.box(m, s["stone"], drop_back=-Z)
    if detail < 2:
        m = frame_matrix(Vector((cx, cy, top + 0.05)), Vector((1, 0, 0)), Vector((0, 1, 0)), Z, (1.05, 0.8, 0.1))
        mesh.box(m, s["stone"])
        for px in (-0.2, 0.2) if detail == 0 else (0.0,):
            res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=8 if detail == 0 else 5,
                                        radius1=0.12, radius2=0.1, depth=0.36,
                                        matrix=Matrix.Translation((cx + px, cy, top + 0.28)))
            for f in {f for v in res["verts"] for f in v.link_faces}:
                f.material_index = mesh.idx[s["pots"]]

    walls = {"front": Wall((1, 0, 0), (hd, 0, 0)), "back": Wall((-1, 0, 0), (-hd, 0, 0)),
             "left": Wall((0, -1, 0), (0, -hw, 0)), "right": Wall((0, 1, 0), (0, hw, 0))}
    for name, u, z0, ww, hh in spec["windows"]:
        walls[name].window(mesh, u, z0, ww, hh, s, detail, spec.get("bars", True))
    du, dw, dh = spec["door"]
    walls["front"].door(mesh, du, dw, dh, s, detail)
    if spec.get("timbers") and detail < 2:
        for name, u0, z0, u1, z1, minor in spec["timbers"]:
            if minor and detail > 0:
                continue
            walls[name].beam(mesh, u0, z0, u1, z1, 0.15, 0.05, s["timber"])
    if spec.get("quoins") and detail == 0:
        # Dressed corner stones, alternately long on the front/back face and on the gable face,
        # each standing 4 cm proud of both faces.
        n_q, qh, step = spec["quoins"]
        for sx in (1, -1):
            for sy in (1, -1):
                for q in range(n_q):
                    a_, b_ = (0.5, 0.3) if q % 2 == 0 else (0.3, 0.5)
                    c = Vector((sx * (hd - a_ / 2 + 0.04), sy * (hw - b_ / 2 + 0.04), PLINTH_TOP + qh * 0.5 + step * q))
                    mesh.box(frame_matrix(c, Vector((1, 0, 0)), Vector((0, 1, 0)), Z, (a_, b_, qh)), s["stone"])
    if spec.get("porch") and detail < 2:
        pu, pw, pdp, ph = spec["porch"]
        fw = walls["front"]
        for side in (-1, 1):
            post = fw.p(pu + side * (pw / 2 - 0.08), (PLINTH_TOP + ph) / 2, pdp - 0.08)
            mesh.box(frame_matrix(post, fw.t, Z, fw.n, (0.14, ph - PLINTH_TOP, 0.14)), s["frame"])
        ppts, pr = roof_profile(pw / 2, math.radians(45), 0.12, 0, 0.0, 0.08)
        # Porch roof: a small gable ridge running out from the wall (local x across, z up).
        ring_a, ring_b = [], []
        for x, z in ppts:
            ring_a.append(mesh.bm.verts.new(fw.p(pu + x, ph + z, 0.0)))
            ring_b.append(mesh.bm.verts.new(fw.p(pu + x, ph + z, pdp + 0.1)))
        kk = len(ppts)
        for i in range(kk):
            j = (i + 1) % kk
            f = mesh.bm.faces.new((ring_a[i], ring_a[j], ring_b[j], ring_b[i]))
            f.material_index = mesh.idx[s["roof"]]
        f = mesh.bm.faces.new(ring_b)
        f.material_index = mesh.idx[s["roof"]]
        f.normal_update()
        if f.normal.dot(fw.n) < 0:
            f.normal_flip()

    return mesh.obj(f"{spec['piece']}_lod{lod}", mats)


def build_piece(spec, generator):
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

    uv.atlas_unwrap(lods, margin=0.012)
    signature = bake.receiver_signature(lods)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        # A 4 px margin stays inside the 12 px island spacing, so a later LOD's bake cannot spill onto
        # an earlier LOD's islands.
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=4, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        # Hooks through one EMIT bake: each material's base colour temporarily carries
        # (snow, porosity, night) and is restored afterwards.
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*spec["hooks"][m.name], 1.0)
        hooks = bake.new_image(f"{piece}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=4, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{piece}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        stats = {"aoMean": float(a.mean()) if a.size else None, "aoP05": float(np.percentile(a, 5)) if a.size else None,
                 "nightTexels": int((px[:, 2] > 0.5).sum())}
        print(f"{piece}: masks {stats}")

    for i, o in enumerate(lods):
        o.data.shade_flat()
        o.parent = root
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    print(f"{piece}: tris per LOD {tris}")
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=4,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=uv.measure_texel_density(lods[0], MASK_SIZE),
                           maskStats=stats, skipped=skip_bake)
    return root, lods
