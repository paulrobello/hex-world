"""Shared road cross-section for road_straight_center and road_straight_edge (and, later, bends).

Edge/center contract (owner requirement, controller C1/C2):
- One cross-section, PROFILE, used unchanged by every piece and every LOD. LODs differ only in the
  number of length segments, never in profile points, so neighbours at different LODs meet
  exactly (kit_gate check_edge_profile: identical across LODs and mirror-symmetric).
- The profile is a list of (t, z): t is the lateral offset across the road (m), measured from the
  lane centre `hexgeo.lane_frame(edge, 0.0, ROAD_WIDTH)["center"]` along the lane tangent, and z is
  the height above the cap border ring (z = 0). It is mirror-symmetric about t = 0, so a piece
  rotated by 180 degrees (edge 3 against edge 0) presents the same section. A bend piece meets this
  section at any edge k by sweeping PROFILE from `lane_frame(k, 0.0, ROAD_WIDTH)` along its own
  path: lane_frame(k) is lane_frame(0) rotated by k (test_hexgeo), so no new profile is needed.
- BED_TOP (0.08 m) is the asphalt height at the kerb face; the crown rises to BED_TOP + 0.03 at t = 0.
- Verge sink: the verge drops from its top (BED_TOP - 0.02) at |t| = ROAD_WIDTH/2 + VERGE down a
  31-degree slope to BED_SINK (-0.30 m) at |t| = SINK_T. The meadow cap relief stays in
  [-0.25, 0] (Task 8), so the slope always enters the cap: no crack between road and meadow, and the
  verge grass meets the cap grass in the same world-space tileable set.
- Verge normals (owner bug 2026-10-14, the dark strip beside the road): every road_verge loop, the
  31-degree sink slope included, carries a +Z custom normal (`verge_normals_up`, applied by every
  piece that sweeps PROFILE after its last join and sharp-edge pass). With its true normal the slope
  facing away from the sun shaded at about half the meadow's luminance, and the slope facing it
  brighter, between the kerb and the line where the meadow relief cuts it.

Centre-line dashes (controller C4): period P = 2 * APOTHEM / 5 = 5.5426 m, 2.7713 m on and 2.7713 m
off, with a dash centred on x = 0. Then the pattern is mirror-symmetric about x = 0 and about
x = APOTHEM (= 2.5 P, the middle of a gap), so a rot-3 edge piece continues its rot-0 neighbour,
and the next cell's centre (2 * APOTHEM = 5 P away) is phase-aligned. A 6 m period cannot meet both
mirrors (2 * APOTHEM / 6 = 4.62 is not an integer). The paint is identical in all three LODs and
never touches x = APOTHEM, so it never lies on the edge section.

Bends (rule for road_bend_60 and later): every edge midpoint sits in the middle of a gap. A piece
whose centre-line path between two edge midpoints has arc length L uses n = round(L / P) dashes at
period L / n, with the dashes centred on (k + 0.5) * L / n for k = 0..n-1 measured from either end.
Each end then starts and ends with half a gap, so the pattern continues into any straight or bent
neighbour at both edges. A straight edge-to-edge run (L = 2 * APOTHEM, n = 5) reproduces P exactly.
"""
import math
import os
import pathlib
import sys

import bmesh
import bpy

import numpy as np

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

ROAD_WIDTH, VERGE, BED_TOP, BED_SINK, CENTER_HALF = 6.0, 1.0, 0.08, -0.30, 6.0
KERB_W, KERB_TOP = 0.25, BED_TOP + 0.12
VERGE_TOP = BED_TOP - 0.02
KERB_COLOR = (0.42, 0.41, 0.37)  # assets/kit/manifest.json road_kerb.color (linear)
SINK_T = ROAD_WIDTH / 2 + VERGE + 0.6
CROWN = 0.03
PAINT_W, PAINT_Z = 0.15, BED_TOP + 0.035
DASH_PERIOD = 2.0 * hexgeo.APOTHEM / 5.0
DASH_ON = DASH_PERIOD / 2.0

# Slot names (assets/kit/manifest.json). Span classes for materials_by_span.
ASPHALT, VERGE_SLOT, KERB, PAINT = "road_asphalt", "road_verge", "road_kerb", "road_paint"
SLOTS = (ASPHALT, VERGE_SLOT, KERB, PAINT)


def _half():
    """Right half (t >= 0), from the crown out to the sink."""
    h = ROAD_WIDTH / 2
    edge = h + VERGE
    return [(0.0, BED_TOP + CROWN), (h * 0.5, BED_TOP + CROWN * 2 / 3), (h, BED_TOP),
            (h, KERB_TOP), (h + KERB_W, KERB_TOP), (h + KERB_W, VERGE_TOP),
            (edge, VERGE_TOP), (SINK_T, BED_SINK)]


PROFILE = [(-t, z) for (t, z) in reversed(_half()[1:])] + _half()


def profile(lod):
    """(t, z) points across the road, left to right. LOD-invariant by contract (see module doc)."""
    del lod
    return list(PROFILE)


def span_slot(t0, t1):
    """Slot for the profile span t0 -> t1: asphalt between the kerbs, kerb on its faces and top,
    verge outside."""
    a, b = sorted((abs(t0), abs(t1)))
    h = ROAD_WIDTH / 2
    if b <= h and not (a == b == h):
        return ASPHALT
    if b <= h + KERB_W:
        return KERB
    return VERGE_SLOT


def build(name, x0, x1, segments, lod, materials_by_span):
    """Sweep the profile along +X from x0 to x1 in `segments` length segments. Face material index
    per profile span via materials_by_span(i, t0, t1)."""
    prof = profile(lod)
    bm = bmesh.new()
    rows = []
    for s in range(segments + 1):
        x = x0 + (x1 - x0) * s / segments
        rows.append([bm.verts.new((x, t, z)) for (t, z) in prof])
    # Winding (s,i) -> (s+1,i) -> (s+1,i+1) -> (s,i+1): normal = X x (0, dt, dz) = (0, -dz, dt), so
    # spans with increasing t face +Z and the walls face outward. No recalc_face_normals (C5/M2).
    bed = []
    for s in range(segments):
        for i in range(len(prof) - 1):
            f = bm.faces.new((rows[s][i], rows[s + 1][i], rows[s + 1][i + 1], rows[s][i + 1]))
            f.material_index = materials_by_span(i, prof[i][0], prof[i + 1][0])
            if prof[i + 1][0] > prof[i][0]:
                bed.append(f)
    bm.normal_update()
    bad = [f.normal.z for f in bed if f.normal.z <= 0.0]
    assert not bad, f"{name}: {len(bad)} bed faces face down"
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(name, mesh))


def verge_normals_up(obj):
    """+Z custom normals on every loop of a road_verge face; every other loop keeps its corner normal.
    Call after the last join and set_sharp_from_angle: both rebuild the corner normals. Verge faces
    are made smooth first, since a flat face ignores custom normals."""
    mesh = obj.data
    verge = {i for i, m in enumerate(mesh.materials) if m and m.name == VERGE_SLOT}
    assert verge, f"{obj.name}: no {VERGE_SLOT} slot"
    count = 0
    for p in mesh.polygons:
        if p.material_index in verge:
            p.use_smooth = True
    out = [tuple(n.vector) for n in mesh.corner_normals]
    for p in mesh.polygons:
        if p.material_index in verge:
            for li in p.loop_indices:
                out[li] = (0.0, 0.0, 1.0)
            count += 1
    assert count, f"{obj.name}: no {VERGE_SLOT} faces"
    mesh.normals_split_custom_set(out)
    mesh.update()
    up = [mesh.corner_normals[li].vector.z for p in mesh.polygons if p.material_index in verge for li in p.loop_indices]
    assert min(up) > 0.9999, (obj.name, min(up))


def dash_spans(x0, x1):
    """Dash intervals of the centre line clipped to [x0, x1] (dash k centred on k * DASH_PERIOD)."""
    out = []
    k = math.floor((x0 - DASH_ON / 2) / DASH_PERIOD)
    while k * DASH_PERIOD - DASH_ON / 2 < x1:
        a = max(x0, k * DASH_PERIOD - DASH_ON / 2)
        b = min(x1, k * DASH_PERIOD + DASH_ON / 2)
        if b - a > 1e-4:
            out.append((a, b))
        k += 1
    return out


def paint(name, x0, x1, material_index=0):
    """The dashed centre-line strip, PAINT_W wide at PAINT_Z (5 mm above the crown), +Z facing."""
    bm = bmesh.new()
    w = PAINT_W / 2
    for a, b in dash_spans(x0, x1):
        v = [bm.verts.new(p) for p in ((a, -w, PAINT_Z), (b, -w, PAINT_Z), (b, w, PAINT_Z), (a, w, PAINT_Z))]
        bm.faces.new(v).material_index = material_index
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(name, mesh))


MASK_SIZE = 512  # one masks bake per piece over the 2R square (C3): 16 px/m


def build_piece(piece, x0, x1, segments, generator, sockets):
    """The whole road piece: three LODs (one profile, `segments` length segments each), planar UV0,
    AO bake on the bed before the paint joins, masks, box collider, sockets, GLB and reports.
    `sockets(root)` adds the piece's sockets. Returns (root, lods)."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        ASPHALT: materials.pbr_material(ASPHALT, materials.srgb("#3a3a3c"), 0.75),
        VERGE_SLOT: materials.pbr_material(VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
        # Linear, = the manifest road_kerb colour, so Blender renders show the runtime kerb.
        KERB: materials.pbr_material(KERB, (*KERB_COLOR, 1.0), 0.85),
        PAINT: materials.pbr_material(PAINT, materials.srgb("#e6e2d6"), 0.6),
    }
    index = {name: i for i, name in enumerate(SLOTS)}
    beds, paints = [], []
    for i, n in enumerate(segments):
        bed = build(f"{piece}_lod{i}_bed", x0, x1, n, i, lambda _i, t0, t1: index[span_slot(t0, t1)])
        pt = paint(f"{piece}_lod{i}_paint", x0, x1)
        for o, slots in ((bed, SLOTS), (pt, (PAINT,))):
            for name in slots:
                o.data.materials.append(mats[name])
            uv.planar_cap_uv(o)
        beds.append(bed)
        paints.append(pt)

    signature = bake.receiver_signature([beds[0]])
    texel = uv.measure_texel_density(beds[0], MASK_SIZE)
    if not skip_bake:
        bake.setup_cycles(samples=64)
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        bake.bake_to(beds[0], "AO", ao, margin=12)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        # Texel centres in Blender pixel order (row 0 = V 0): y = 32V - 16. Porosity 0.3 on the
        # asphalt and kerb, 0.7 on the verge; snow capacity 1.0 everywhere; no night emissive.
        c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
        _, ys = np.meshgrid(c, c)
        wet = np.where(np.abs(ys) <= ROAD_WIDTH / 2 + KERB_W, 0.3, 0.7).ravel().astype(np.float32)
        bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)

    lods = []
    for i, (bed, pt) in enumerate(zip(beds, paints)):
        body = geom.join([bed, pt], f"{piece}_lod{i}")
        body.data.shade_smooth()
        body.data.set_sharp_from_angle(angle=math.radians(30.0))
        verge_normals_up(body)
        body.parent = root
        lods.append(body)
        assert [m.name for m in body.data.materials] == list(SLOTS), [m.name for m in body.data.materials]
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    print(f"{piece}: tris per LOD {tris}")

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    sockets(root)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, profile=[list(p) for p in PROFILE],
                           dashPeriodM=DASH_PERIOD, skipped=skip_bake)
    return root, lods
