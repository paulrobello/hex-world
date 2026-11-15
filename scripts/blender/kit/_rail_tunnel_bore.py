"""Rail tunnel bore family: rail_tunnel_bore.py, the stone-lined tube a complete rail tunnel draws under
each cover cell (open tunnels, owner 2026-11-14, docs/superpowers/specs/2026-11-14-open-tunnels-design.md;
section ruling O1, owner 2026-11-14: a turf ridge over the bore, docs/checkpoints/rail_tunnel_bore/options.md).

Frame: the piece origin is the cover cell's centre at the tunnel (portal) level; the axis runs along x
from edge 3 (x = -APOTHEM) to edge 0 (x = +APOTHEM). Section (Blender y = s across, z = h up): the rail
portal's opening, jambs at |s| = A from -BURY to the springing J, a semicircle of radius A to the crown
J + A. The bore is the rail portal's open interior carried on: the lining is
`_tunnel_portal_rail.open_lining` on `open_ring(lod)` (the portal lining's arch divisions plus the crown and
the panel island's girth cuts), the ridge `add_ridge` (crest 5.75 m, easing to the 4.0 m cover by |s| = 7
and diving 6 cm under it by 7.4), so a bore meets the portal's `_open` slots and any other bore vertex for
vertex at both ends in every LOD, and the girth's v is measured from the crown, so a bore turned half round
meets on the same v. The floor is the rail track (`_rail.track`), its ballast running into the walls, with
`_rail.section()` exactly at both ends.

Maps: the rail portal's own atlas, baked from its own templates and materials (`_tunnel_portal.build_templates`
with `stone_materials("rail_tunnel_portal")`, the same islands), so the lining's panel island holds the
portal's pixels and the stone continues across edge 0. Unused portal islands carry the bore's parts: the
glass at true scale on the barrel island (amber, lit by the emissive map, which is on at all hours, and
masks B for the P5 night), the track on the lining block, the lamp iron on park_iron (no snow under the
hill), the ridge's turf on park_turf (the portal ridge's own masks). Lamps: LAMPS per bore on alternating
walls, L / LAMPS apart (so the spacing holds across joints)."""
from __future__ import annotations

import json
import math
import os
import pathlib
import sys

import bpy
import numpy as np

import _rail
import _tunnel_portal_rail as tpr
from lib import bake, budgets, collider, export, hexgeo, materials, scene

tp = tpr.tp
PORTAL = "rail_tunnel_portal"
APOTHEM = hexgeo.APOTHEM
L = 2 * APOTHEM
X0, X1 = -APOTHEM, APOTHEM
A, J, BURY = tp.A, tp.J, tp.BURY
CROWN = J + A
assert abs(CROWN - 5.0) < 1e-9 and abs(A - 2.6) < 1e-9 and abs(J - 2.4) < 1e-9, "the rail portal's opening"
ATLAS = tp.ATLAS
EMISSIVE_SIZE = 256
# Per LOD: lining cells along the bore (each one panel width of u, 1.73 m at LOD0), the sleepers, and
# whether the lamps keep their iron.
CELLS = (16, 8, 1)
LOD = [{"sleepers": "full", "iron": True}, {"sleepers": "top", "iron": True}, {"sleepers": None, "iron": False}]
# Lamps: a glass box on an iron arm, LAMP_Z0..LAMP_Z1 over the tunnel level, LAMP_OUT off the wall.
LAMPS = 4
LAMP_HW, LAMP_Z0, LAMP_Z1, LAMP_OUT, ARM_Z = 0.085, 2.0, 2.28, 0.32, 2.5
SLOTS = ("stone", "glass", "iron", "turf", _rail.BALLAST, _rail.SLEEPER, _rail.STEEL, _rail.HEAD)
RAIL_OFFSET = SLOTS.index(_rail.SLOTS[0])
# The bore's own masks on the portal islands it repurposes (R snow, G wetness, B night, A AO).
GLASS_ALBEDO, GLASS_EMIT = (0.86, 0.6, 0.24, 1.0), (1.0, 0.7, 0.27, 1.0)  # sRGB-encoded amber
# R stays a trace under the hill (the hooks gate wants every slot's snow channel authored; the shader's
# N.y ramp keeps it off the walls, so only the up-facing track and lamp caps take that trace).
MASK_GLASS, MASK_IRON, MASK_TRACK = (0.0, 0.1, 1.0, 1.0), (0.02, 0.15, 0.0, 1.0), (0.02, 0.6, 0.0, 1.0)


def lamp_positions():
    """(x, side) per lamp: L / LAMPS apart on alternating walls (an even count keeps it across joints)."""
    assert LAMPS % 2 == 0
    return [(X0 + L * (k + 0.5) / LAMPS, 1 if k % 2 == 0 else -1) for k in range(LAMPS)]


def lamps(b, isl, d):
    gi, park = isl["barrel"], isl["park_iron"].uv(0.5, 0.5)
    hw, gh = LAMP_HW, LAMP_Z1 - LAMP_Z0
    for x, side in lamp_positions():
        s = side * (A - LAMP_OUT)
        box = [(x - hw, s - hw), (x + hw, s - hw), (x + hw, s + hw), (x - hw, s + hw)]
        for q in range(4):
            (x0, y0), (x1, y1) = box[q], box[(q + 1) % 4]
            out = ((x0 + x1) / 2 - x, (y0 + y1) / 2 - s, 0.0)
            u0 = 2 * hw * q
            b.face([(x0, y0, LAMP_Z0), (x1, y1, LAMP_Z0), (x1, y1, LAMP_Z1), (x0, y0, LAMP_Z1)],
                   [gi.uv(u0, 0.0), gi.uv(u0 + 2 * hw, 0.0), gi.uv(u0 + 2 * hw, gh), gi.uv(u0, gh)], "glass", out)
        if not d["iron"]:
            continue
        apex = (x, s, LAMP_Z1 + 0.1)
        e = hw + 0.02
        cap = [(x - e, s - e), (x + e, s - e), (x + e, s + e), (x - e, s + e)]
        for q in range(4):
            (x0, y0), (x1, y1) = cap[q], cap[(q + 1) % 4]
            b.face([(x0, y0, LAMP_Z1), (x1, y1, LAMP_Z1), apex], [park] * 3, "iron",
                   ((x0 + x1) / 2 - x, (y0 + y1) / 2 - s, 0.5))
        b.face([(px, py, LAMP_Z0) for px, py in box], [park] * 4, "iron", (0.0, 0.0, -1.0))
        bw, w0 = 0.015, side * A  # the arm: a square bar from the wall to over the lamp
        for z, up in ((ARM_Z + bw, 1.0), (ARM_Z - bw, -1.0)):
            b.face([(x - bw, w0, z), (x + bw, w0, z), (x + bw, s, z), (x - bw, s, z)], [park] * 4, "iron", (0.0, 0.0, up))
        for xo in (-bw, bw):
            b.face([(x + xo, w0, ARM_Z - bw), (x + xo, s, ARM_Z - bw), (x + xo, s, ARM_Z + bw), (x + xo, w0, ARM_Z + bw)],
                   [park] * 4, "iron", (math.copysign(1.0, xo), 0.0, 0.0))
        b.face([(x - 0.008, s, apex[2]), (x + 0.008, s, apex[2]), (x + 0.008, s, ARM_Z - bw), (x - 0.008, s, ARM_Z - bw)],
               [park] * 4, "iron", (0.0, -side, 0.0))


def track(b, isl, d):
    """`_rail.track` on the straight from edge 3 to edge 0, its UVs planar into the lining block (the
    rail slots are colour-only or world-space tiled, so UV0 addresses the masks only)."""
    m = _rail.track(_rail.straight_frames(1), _rail.straight_sleepers() if d["sleepers"] else None,
                    top_only=d["sleepers"] == "top")
    pk = isl["lining"]
    for idx, slot in m.faces:
        pts = [m.verts[i] for i in idx]
        f = b.bm.faces.new([b.bm.verts.new(p) for p in pts])
        f.material_index = RAIL_OFFSET + slot
        for loop, p in zip(f.loops, pts):
            loop[b.uv].uv = pk.uv(0.1 + 0.8 * (p[0] - X0) / L, min(max(0.5 + 0.12 * p[1] / A + 0.05 * p[2], 0.1), 0.9))
    return len(m.faces)


def end_plane(o, x, k):
    me = o.data
    return sorted({(round(me.vertices[vi].co.y * 1e4), round(me.vertices[vi].co.z * 1e4))
                   for p in me.polygons if p.material_index in k for vi in p.vertices
                   if abs(me.vertices[vi].co.x - x) < 1e-4})


def section_errors(o, lod):
    """At both end planes: the track slots carry `_rail.section()` exactly, the lining the portal's
    `open_section(lod)`, the ridge `ridge_section()`, and nothing else touches the plane."""
    rail = set(range(RAIL_OFFSET, len(SLOTS)))
    errs = []
    for x_end in (X0, X1):
        if end_plane(o, x_end, rail) != _rail.section():
            errs.append(f"{o.name} x {x_end:.4f}: track section differs from _rail.section()")
        if end_plane(o, x_end, {SLOTS.index("stone")}) != tpr.open_section(lod):
            errs.append(f"{o.name} x {x_end:.4f}: lining differs from the portal's open_section({lod})")
        if end_plane(o, x_end, {SLOTS.index("turf")}) != tpr.ridge_section():
            errs.append(f"{o.name} x {x_end:.4f}: ridge differs from the portal's ridge_section()")
        other = end_plane(o, x_end, {SLOTS.index("glass"), SLOTS.index("iron")})
        if other:
            errs.append(f"{o.name} x {x_end:.4f}: {len(other)} lamp vertices on the end plane")
    return errs


def lamp_clearance(o, trains):
    """The lamps' smallest lateral margin over the widest vehicle (the vehicles stop at |y| = halfWidthM,
    under the lamps' height), and the lamps' lowest point."""
    half = max(float(t[2][:, 0].max()) for t in trains)
    me = o.data
    lamp = [me.vertices[vi].co for p in me.polygons if SLOTS[p.material_index] in ("glass", "iron") for vi in p.vertices]
    return min(abs(c.y) for c in lamp) - half, min(c.z for c in lamp)


def lining_sweep(o, env):
    """Every lining vertex over the vehicles' envelope at its |y| (the bore is all tunnel)."""
    ys, zs = env
    me, stone = o.data, SLOTS.index("stone")
    worst = (math.inf, None)
    for p in me.polygons:
        if p.material_index != stone:
            continue
        for vi in p.vertices:
            co = me.vertices[vi].co
            if abs(co.y) > ys[-1]:
                continue
            h = zs[min(np.searchsorted(ys, abs(co.y) - 0.02), len(zs) - 1)]
            if co.z - h < worst[0]:
                worst = (float(co.z - h), (round(co.x, 3), round(co.y, 3), round(co.z, 3)))
    return worst


def ridge_cover(o):
    """The ridge's smallest height over the lining (the turf's cover over the crown)."""
    prof = tpr.ridge_profile()

    def ridge_at(s):
        for (s0, h0), (s1, h1) in zip(prof, prof[1:]):
            if s0 <= s <= s1:
                return h0 + (h1 - h0) * (s - s0) / (s1 - s0)
        return -math.inf
    me, stone = o.data, SLOTS.index("stone")
    return min(ridge_at(me.vertices[vi].co.y) - me.vertices[vi].co.z
               for p in me.polygons if p.material_index == stone for vi in p.vertices
               if me.vertices[vi].co.z > J)


def build_lod(spec, lod, isl, mats):
    d = LOD[lod]
    b = tp.Builder(SLOTS)
    xs = [X0 + L * k / CELLS[lod] for k in range(CELLS[lod] + 1)]
    xs[0], xs[-1] = X0, X1
    tpr.open_lining(b, isl, lod, xs, "stone")
    tpr.add_ridge(b, isl, X0, X1, "turf")
    lamps(b, isl, d)
    nt = track(b, isl, d)
    o = b.obj(f"{spec['piece']}_lod{lod}", mats)
    me = o.data
    stone, turf = SLOTS.index("stone"), SLOTS.index("turf")
    loop_n = []
    for p in me.polygons:
        p.use_smooth = p.material_index in (stone, turf)
        for li in p.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            if p.material_index == stone:
                loop_n.append((0.0, *tpr.inward(co.y, co.z)))
            elif p.material_index == turf:
                loop_n.append(tuple(tpr.ridge_normal(co.y)))
            else:
                loop_n.append(tuple(p.normal))
    me.normals_split_custom_set(loop_n)
    tally = {}
    for p in me.polygons:
        tally[SLOTS[p.material_index]] = tally.get(SLOTS[p.material_index], 0) + 1
    print(f"{spec['piece']}: lod{lod} tris {len(me.polygons)} by slot {tally}, track faces {nt}")
    return o, tally


def bake_maps(isl, piece, out, skip_bake):
    """The rail portal's maps, from its own templates (as `_tunnel_portal.build_piece` bakes them), then
    the bore's parts on the islands it repurposes. Returns (albedo, normal, orm, masks, emissive, stats,
    receiver signature)."""
    highs, receiver = tp.build_templates(isl, tp.stone_materials(PORTAL))
    signature = bake.receiver_signature([receiver])
    stats, baked = {}, None
    if not skip_bake:
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)
        scn.world.light_settings.distance = tp.AO_DIST
        bake.setup_cycles(samples=tp.SAMPLES)
        kw = dict(cage_extrusion=tp.CAGE, max_ray=tp.MAX_RAY, margin=tp.MARGIN)
        imgs = {k: bake.new_image(f"{piece}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "ao", "rough")}
        bake.bake_emit_input(highs, receiver, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(receiver, "NORMAL", imgs["normal"], highs=highs, **kw)
        bake.bake_to(receiver, "ROUGHNESS", imgs["rough"], highs=highs, **kw)
        bake.bake_to(receiver, "AO", imgs["ao"], highs=highs, **kw)
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1].reshape(ATLAS, ATLAS, 4)
            os.remove(p)
        cover = np.zeros((ATLAS, ATLAS), bool)
        for name in isl:
            if name.split("_")[0] in ("panel", "band", "v0", "v1", "key", "jamb", "cop"):
                x0, y0, x1, y1 = isl[name].px_box()
                cover[y0:y1, x0:x1] = True
        px["cover"] = cover
        baked = px
        stats["normalMissFraction"] = float((px["normal"][cover, 2] < 0.05).mean())
        a = px["ao"][cover, 0]
        stats["ao"] = {"mean": float(a.mean()), "p05": float(np.percentile(a, 5))}
        stats["albedoLuma"] = float(px["albedo"][cover, :3].mean())
        stats["seam"] = {f"panel.{k}": tp.seam_errors(px[k], isl, ATLAS, "panel") for k in ("albedo", "normal", "ao")}
        print(f"{piece}: bake stats {json.dumps(stats)}")
        bad = {k: v for k, v in stats["seam"].items() if max(v.values()) > (0.06 if k.endswith("ao") else 0.02)}
        assert not bad, f"the panel island does not wrap: {bad}"
    alb, nrm, orm, msk = tp.compose_maps(isl, baked, {"piece": PORTAL})
    for o in list(highs) + [receiver]:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    emi = np.zeros((ATLAS, ATLAS, 4), np.float32)
    emi[..., 3] = 1.0
    x0, y0, x1, y1 = isl["barrel"].px_box()
    alb[y0:y1, x0:x1] = GLASS_ALBEDO
    nrm[y0:y1, x0:x1] = (0.5, 0.5, 1.0, 1.0)
    orm[y0:y1, x0:x1] = (1.0, 0.2, 0.0, 1.0)
    msk[y0:y1, x0:x1] = MASK_GLASS
    emi[y0:y1, x0:x1] = GLASS_EMIT
    tp._fill_box(msk, isl["park_iron"].px_box(), MASK_IRON)
    tp._fill_box(msk, isl["lining"].px_box(), MASK_TRACK)
    return alb, nrm, orm, msk, emi, stats, signature


def build_piece(spec, generator):
    piece = spec["piece"]
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    trains = tpr.train_vertices()  # resets the scene, so first
    clear = tpr.train_clearance(trains)
    env = tpr.envelope(trains)
    tpr._set_end_b()
    scene.reset_scene(piece)
    isl = tp.pack(tp._islands())
    alb, nrm, orm, msk, emi, stats, signature = bake_maps(isl, piece, out, skip_bake)
    mats = [materials.pbr_material(n if n in _rail.SLOTS else f"{piece}_{n}", (*spec["colors"][n], 1.0), spec["rough"][n])
            for n in SLOTS]
    root = scene.empty(piece)
    built = [build_lod(spec, i, isl, mats) for i in range(3)]
    lods = [o for o, _ in built]
    tris = [len(o.data.polygons) for o in lods]
    density = {n: tp.slot_density(lods[0], SLOTS.index(n)) for n in ("stone", "glass")}
    errs = [e for k, o in enumerate(lods) for e in section_errors(o, k)]
    lamp_side, lamp_low = min(lamp_clearance(o, trains) for o in lods)
    sweeps = [lining_sweep(o, env) for o in lods]
    covers = [ridge_cover(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}, density {density}, lamp lateral margin {lamp_side:.3f} m "
          f"(lowest {lamp_low:.2f} m), lining sweep {sweeps}, ridge cover {covers}, section errors {errs}")
    assert not errs, errs
    assert lamp_side >= tpr.CLEAR_MIN, ("a lamp stands in the train's path", lamp_side)
    assert min(s[0] for s in sweeps) >= tpr.CLEAR_MIN, ("a lining vertex sits in the train's path", sweeps)
    assert min(covers) >= tpr.LINING_COVER, ("the ridge does not cover the lining", covers)
    budgets.check(tris, "prop", piece)
    for n, v in density.items():
        assert v["median"] >= 256.0, (n, v)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(tp.NORMAL_SIZE, tp._down(nrm, ATLAS, tp.NORMAL_SIZE, renormal=True), str(out / f"{piece}.normal.png"))
    bake.write_png(tp.ORM_SIZE, tp._down(orm, ATLAS, tp.ORM_SIZE), str(out / f"{piece}.orm.png"))
    bake.write_png(tp.MASK_SIZE, tp._down(msk, ATLAS, tp.MASK_SIZE), str(out / f"{piece}.masks.png"))
    bake.write_png(EMISSIVE_SIZE, tp._down(emi, ATLAS, EMISSIVE_SIZE), str(out / f"{piece}.emissive.png"), non_color=False)
    # The collider: the lining's box (LOD2's stone), the tube the trains and walkers run through.
    tmp = lods[2].copy()
    tmp.data = lods[2].data.copy()
    scene.link(tmp)
    import bmesh
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != SLOTS.index("stone")], context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    col = collider.collider_from(tmp, "box", f"{piece}_collider")
    bpy.data.objects.remove(tmp, do_unlink=True)
    for o in lods + [col]:
        o.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    rep_path = out / "export-report.json"
    rep = json.loads(rep_path.read_text())
    rep["pngFallbackSize"] = {"albedo": tp.PNG_ALBEDO}
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"),
        samples=tp.SAMPLES, marginPx=tp.MARGIN, cageExtrusionM=tp.CAGE, maxRayDistanceM=tp.MAX_RAY, aoDistanceM=tp.AO_DIST,
        channels={"albedo": {"size": ATLAS, "colorspace": "srgb"}, "normal": {"size": tp.NORMAL_SIZE, "colorspace": "linear"},
                  "orm": {"size": tp.ORM_SIZE, "colorspace": "linear"}, "masks": {"size": tp.MASK_SIZE, "colorspace": "linear"},
                  "emissive": {"size": EMISSIVE_SIZE, "colorspace": "srgb"}},
        texelDensityPxPerM=density, bakeStats=stats, lodTris=tris,
        lod2Drops={_rail.SLEEPER: "under a pixel at 400 m: a sleeper is 0.26 m wide (0.8 px at 0.31 m/px), inside the bore",
                   f"{piece}_iron": "under a pixel at 400 m: the lamp iron is 0.03 m bars and a 0.21 m cap "
                                    "(0.1 to 0.7 px at 0.31 m/px), inside the bore"},
        islands={k: [v.x0, v.y0, v.pw, v.ph] for k, v in sorted(isl.items())},
        bore={"lengthM": round(L, 4), "halfSpanM": A, "springingM": J, "crownM": CROWN, "cells": list(CELLS),
              "ringPoints": [len(tpr.open_ring(k)) for k in range(3)], "aroundPeriods": list(tpr.OPEN_AROUND),
              "ridge": {"crestM": tpr.RIDGE_CREST, "halfM": tpr.RIDGE_HALF, "outM": tpr.RIDGE_OUT, "diveM": tpr.RIDGE_DIVE,
                        "section": [[round(s, 4), round(h, 4)] for s, h in tpr.ridge_profile()],
                        "coverOverLiningM": [round(c, 4) for c in covers]},
              "trisBySlot": {f"lod{k}": t for k, (_, t) in enumerate(built)},
              "railTopM": _rail.RAIL_TOP, "ballastTopM": _rail.BALLAST_TOP, "mapsFrom": PORTAL,
              "lamps": [[round(x, 4), s] for x, s in lamp_positions()], "lampLateralMarginM": round(lamp_side, 4),
              "lampLowestM": round(lamp_low, 3), "liningSweepMarginM": [round(s[0], 4) for s in sweeps],
              "liningSweepAt": [s[1] for s in sweeps], "trains": clear},
        skipped=skip_bake)
    return root, lods
