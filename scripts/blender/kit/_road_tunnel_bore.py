"""road_tunnel_bore: one cell of an open road tunnel (P4 open tunnels, owner 2026-11-14; spec
docs/superpowers/specs/2026-11-14-open-tunnels-design.md). A stone-lined tube from cell edge to cell edge
(x -13.8564 to +13.8564, +x toward edge 0) on the road portal's opening (7.8 m wide, springing 0.6 m,
crown 3.15 m, the same arch polyline per LOD as road_tunnel_portal's lining), carrying the road section of
road_straight_center/edge clipped at the lining (|t| <= 3.9), with six always-lit amber wall lamps.

Frame as road_tunnel_portal (_tunnel_portal.py): Blender (x, y, z) exports as runtime (x, z, -y), s is
Blender y, the pivot is the cell centre at the tunnel's floor level (the portal level), so the road crown is
0.11 m over y = 0 as on the road pieces.

Tiling: both ends cut the same section and the same texture phase (the lining's columns are centred on
x = 0 and 12 of them span the cell, the lamps sit at x = 0 and +-9.2376), so bores join at any count and
meet the portal lining's section at the cell edge. Every bore of one tunnel takes one yaw, so the masonry
runs on across the joints.

Stone: the portal's own wall panel. The generator builds the portal's high-poly templates
(`_tunnel_portal.build_templates`, the same seeds and materials), bakes them as the portal does and copies
the wrap-tileable panel island into this atlas twice: the panel, and the pool, the same pixels plus the lamp's
glow in the emissive map. The lining maps one panel cell per column (2.3094 m along the tunnel) and per four
arch segments (1.3657 m round the girth) at LOD0, about 266 px/m, so the joints between cells and between
bores fall on the panel's wrap.

Lamps (no new real lights): the piece's emissive map, which kit_material applies with factor 1 to every unique
slot, independent of the P5 night mask (masks B stays 0): the lamp glass and a warm pool on the stone round
each lamp glow at all times. The floor's pools are ambient-occlusion lift in the masks (A), since the road
slots sample the shared tileable sets.

Slots: road_tunnel_bore_stone (unique: lining, lamps, extrados) and the four road slots (road_asphalt,
road_verge, road_kerb, road_paint, as the road pieces), each with masks on UV0. Pack `core`, kind `prop`, KTX2
class `piece`."""
from __future__ import annotations

import json
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np

import _road
import _tunnel_portal as tp
from lib import bake, budgets, collider, export, hexgeo, materials, scene

PIECE = "road_tunnel_bore"
AP = hexgeo.APOTHEM
A, J, B, BURY = tp.A, tp.J, tp.B, tp.BURY
CROWN = tp.CROWN
PW, PH, TEXEL, GUT, ATLAS = tp.PW, tp.PH, tp.TEXEL, tp.GUT, tp.ATLAS
NORMAL_SIZE, ORM_SIZE, MASK_SIZE, EMIS_SIZE = 512, 512, 512, 256
PNG_ALBEDO = 512  # the PNG fallback albedo, as road_tunnel_portal (owner O1)

STONE = f"{PIECE}_stone"
SLOTS = (STONE,) + _road.SLOTS

# The lining's columns along x (12 per cell at LOD0, centred on x = 0) and the lamps (every fourth column).
COL0 = 2.0 * AP / 12.0
LAMP_X = (-4.0 * COL0, 0.0, 4.0 * COL0)
LAMP_F = 5.0 / 30.0  # the lamp's girth position: arc fraction from the right springing (LOD0 vertex 5)
# The lamp: a glass box set into the vault (half width along x, height below/above the mount, reach out of
# the wall, depth into it).
LAMP_HW, LAMP_DOWN, LAMP_UP, LAMP_OUT, LAMP_IN = 0.11, 0.18, 0.12, 0.18, 0.32
# Per LOD: arch polyline segments (the portal lining's), arch segments per v-cell and the first cell's
# vertex, columns per lining cell (x COL0), lamp detail (2 box, 1 front and bottom, 0 front), extrados
# segments.
LOD = [
    {"arch": tp.LOD[0]["segs"] * tp.LOD[0]["sub"], "per": 4, "phase": 3, "cols": 1, "lamp": 2, "extrados": 7},
    {"arch": tp.LOD[1]["segs"] * tp.LOD[1]["sub"], "per": 1, "phase": 0, "cols": 2, "lamp": 1, "extrados": 7},
    {"arch": tp.LOD[2]["segs"] * tp.LOD[2]["sub"], "per": 1, "phase": 0, "cols": 4, "lamp": 0, "extrados": 5},
]
# Fewer than 5 extrados segments sag about EXTRADOS between their ends near the springings, so the lining's
# corners poke through the shell (LOD2 at 3 did).
assert [d["arch"] for d in LOD] == [30, 9, 7]
EXTRADOS = 0.45  # the shell's thickness over the intrados (top 3.6 m, under the 4 m cover)

# The floor: the road profile inside the lining (the verge is flat at VERGE_TOP from 3.25 to 4.0).
FLOOR_T = A
FLOOR_PROFILE = ([(-FLOOR_T, _road.VERGE_TOP)] + [p for p in _road.PROFILE if abs(p[0]) <= 3.25 + 1e-9]
                 + [(FLOOR_T, _road.VERGE_TOP)])

# Look (tuned against in-engine shots): the lining's and the floor's ambient occlusion (masks A), the floor
# pools' peak A and radius, the wall glow's peak (x linear albedo) and the lamp's colours (linear).
A_LINING, A_FLOOR, A_POOL, POOL_R = 0.22, 0.30, 0.85, 2.6
GLOW, WARM = 1.1, (1.0, 0.62, 0.30)
GLASS_ALBEDO, GLASS_EMIT, IRON = (0.72, 0.33, 0.045), (1.0, 0.70, 0.36), (0.035, 0.035, 0.038)
WET, SNOW = 0.35, 0.05  # dry under cover, a trace of snow capacity (the hooks gate wants both above 0)


def _islands():
    return [tp.Island("panel", PW, PH), tp.Island("pool", PW, PH),
            tp.Island("floor", 2 * AP, 2 * FLOOR_T, False, (224, 64)),
            tp.Island("glass", 0.5, 0.35), tp.Island("iron", 0.5, 0.35),
            tp.Island("park", 1.0, 1.0, False, (16, 16))]


def lamp_wall(side):
    """The lamp's mount on the intrados (s, h) and the wall's outward normal there."""
    t = tp.ellipse_divisions(LOD[0]["arch"])[round(LAMP_F * LOD[0]["arch"])]
    s, h = tp.intrados(t)
    n = tp.normal(t)
    return (side * s, h), (side * n[0], n[1])


def girth(lod):
    """The lining's girth segments, right leg foot round to left leg foot: [((s0, h0), (s1, h1), cell,
    b0, b1)], cell the v-cell index and b island metres (0..PH) across it."""
    d = LOD[lod]
    arch = [tp.intrados(t) for t in tp.ellipse_divisions(d["arch"])]
    n = len(arch) - 1
    per, ph = d["per"], d["phase"]
    plen = per * sum(math.dist(p, q) for p, q in zip(arch, arch[1:])) / n
    mid = []
    for j in range(n):
        c = math.floor((j - ph) / per)
        k = (j - ph) - c * per
        mid.append((arch[j], arch[j + 1], c, k / per * PH, (k + 1) / per * PH))
    right, h, b, c, left_ = [], J, mid[0][3], mid[0][2], J + BURY
    while left_ > 1e-9:
        if b < 1e-9:
            c, b = c - 1, PH
        take = min(left_, b / PH * plen)
        nb = b - take / plen * PH
        right.insert(0, ((A, h - take), (A, h), c, nb, b))
        h, b, left_ = h - take, nb, left_ - take
    left, h, b, c, rest = [], J, mid[-1][4], mid[-1][2], J + BURY
    while rest > 1e-9:
        if PH - b < 1e-9:
            c, b = c + 1, 0.0
        take = min(rest, (PH - b) / PH * plen)
        nb = b + take / plen * PH
        left.append(((-A, h), (-A, h - take), c, b, nb))
        h, b, rest = h - take, nb, rest - take
    return right + mid + left


def lamp_cells(lod):
    """The v-cells holding the right and left lamps."""
    segs = girth(lod)
    cum = [0.0]
    arch = [g for g in segs if not (abs(abs(g[0][0]) - A) < 1e-9 and abs(abs(g[1][0]) - A) < 1e-9)]
    for g in arch:
        cum.append(cum[-1] + math.dist(g[0], g[1]))
    out = []
    for f in (LAMP_F, 1.0 - LAMP_F):
        at = f * cum[-1]
        j = next(i for i in range(len(arch)) if cum[i + 1] >= at - 1e-9)
        out.append(arch[j][2])
    return set(out)


def strips(lod):
    """The lining's strips along x: [(x0, x1, column index)], column k centred on k * cols * COL0."""
    cu = LOD[lod]["cols"] * COL0
    cuts = [-AP]
    k = math.floor((-AP + cu / 2) / cu) + 1
    while k * cu - cu / 2 < AP - 1e-6:
        x = k * cu - cu / 2
        if x > -AP + 1e-6:
            cuts.append(x)
        k += 1
    cuts.append(AP)
    return [(x0, x1, math.floor(((x0 + x1) / 2 + cu / 2) / cu)) for x0, x1 in zip(cuts, cuts[1:])]


def build_lod(lod, isl, mats):
    d = LOD[lod]
    b = tp.Builder(SLOTS)
    cu = d["cols"] * COL0
    pools = lamp_cells(lod)
    lamp_cols = {round(x / cu) for x in LAMP_X}
    assert all(abs(round(x / cu) * cu - x) < 1e-6 for x in LAMP_X)
    # The lining: one face per strip and girth segment, inward.
    for x0, x1, kc in strips(lod):
        a0, a1 = ((x - (kc * cu - cu / 2)) / cu * PW for x in (x0, x1))
        for (s0, h0), (s1, h1), c, b0, b1 in girth(lod):
            hm, sm = (h0 + h1) / 2, (s0 + s1) / 2
            inward = (0.0, -sm, (J - hm) if hm > J else 0.0)
            i = isl["pool"] if kc in lamp_cols and c in pools else isl["panel"]
            b.face([(x0, s0, h0), (x1, s0, h0), (x1, s1, h1), (x0, s1, h1)],
                   [i.uv(a0, b0), i.uv(a1, b0), i.uv(a1, b1), i.uv(a0, b1)], STONE, inward)
    # The extrados: a coarse outer shell (shadows and a solid look where the cover is cut away).
    park = isl["park"].uv(0.5, 0.5)
    ext = ([(A + EXTRADOS, -BURY)] + [tp.ring_pt(t, EXTRADOS) for t in tp.ellipse_divisions(d["extrados"])]
           + [(-A - EXTRADOS, -BURY)])
    for (s0, h0), (s1, h1) in zip(ext, ext[1:]):
        hm, sm = (h0 + h1) / 2, (s0 + s1) / 2
        b.face([(-AP, s0, h0), (AP, s0, h0), (AP, s1, h1), (-AP, s1, h1)], [park] * 4, STONE,
               (0.0, sm, (hm - J) if hm > J else 0.0))
    # The floor: the road section swept edge to edge (the road pieces' winding: spans of rising t face
    # up, kerb faces toward the road), UV0 on the floor island.
    fl = isl["floor"]
    for (t0, z0), (t1, z1) in zip(FLOOR_PROFILE, FLOOR_PROFILE[1:]):
        b.face([(-AP, t0, z0), (AP, t0, z0), (AP, t1, z1), (-AP, t1, z1)],
               [fl.uv(0.0, t0 + FLOOR_T), fl.uv(2 * AP, t0 + FLOOR_T), fl.uv(2 * AP, t1 + FLOOR_T),
                fl.uv(0.0, t1 + FLOOR_T)], _road.span_slot(t0, t1))
    w = _road.PAINT_W / 2
    for xa, xb in _road.dash_spans(-AP, AP):
        pts = [(xa, -w, _road.PAINT_Z), (xb, -w, _road.PAINT_Z), (xb, w, _road.PAINT_Z), (xa, w, _road.PAINT_Z)]
        b.face(pts, [fl.uv(x + AP, s + FLOOR_T) for x, s, _ in pts], _road.PAINT, (0.0, 0.0, 1.0))
    # The lamps: a glass box set into the vault, its front facing the road.
    gi, ii = isl["glass"], isl["iron"]
    for side in (1, -1):
        (sw, hw), _ = lamp_wall(side)
        s_in, s_back = sw - side * LAMP_OUT, sw + side * LAMP_IN
        z0, z1 = hw - LAMP_DOWN, hw + LAMP_UP
        for xl in LAMP_X:
            xa, xb = xl - LAMP_HW, xl + LAMP_HW
            b.face([(xa, s_in, z0), (xb, s_in, z0), (xb, s_in, z1), (xa, s_in, z1)],
                   [gi.uv(0.0, 0.0), gi.uv(2 * LAMP_HW, 0.0), gi.uv(2 * LAMP_HW, z1 - z0), gi.uv(0.0, z1 - z0)],
                   STONE, (0.0, -side, 0.0))
            if d["lamp"] >= 1:
                b.face([(xa, s_back, z0), (xb, s_back, z0), (xb, s_in, z0), (xa, s_in, z0)],
                       [ii.uv(0.0, 0.0), ii.uv(0.0, 2 * LAMP_HW), ii.uv(LAMP_OUT + LAMP_IN, 2 * LAMP_HW),
                        ii.uv(LAMP_OUT + LAMP_IN, 0.0)], STONE, (0.0, 0.0, -1.0))
            if d["lamp"] >= 2:
                for x, nx in ((xa, -1.0), (xb, 1.0)):
                    b.face([(x, s_back, z0), (x, s_in, z0), (x, s_in, z1), (x, s_back, z1)],
                           [gi.uv(0.0, 0.0), gi.uv(LAMP_OUT + LAMP_IN, 0.0), gi.uv(LAMP_OUT + LAMP_IN, z1 - z0),
                            gi.uv(0.0, z1 - z0)], STONE, (nx, 0.0, 0.0))
    return b.obj(f"{PIECE}_lod{lod}", mats)


def _enc(lin):
    return np.power(np.clip(lin, 0.0, 1.0), 1 / 2.2)


def compose(isl, isl_p, baked):
    n = ATLAS
    alb = np.tile(np.array((0x9a / 255, 0x96 / 255, 0x8c / 255, 1.0), np.float32), (n, n, 1))
    nrm = np.tile(np.array((0.5, 0.5, 1.0, 1.0), np.float32), (n, n, 1))
    orm = np.tile(np.array((1.0, 0.85, 0.0, 1.0), np.float32), (n, n, 1))
    msk = np.tile(np.array((SNOW, WET, 0.0, A_LINING), np.float32), (n, n, 1))
    emi = np.zeros((n, n, 4), np.float32)
    emi[..., 3] = 1.0
    sx0, sy0, sx1, sy1 = isl_p["panel"].px_box()
    for name in ("panel", "pool"):
        x0, y0, x1, y1 = isl[name].px_box()
        assert (x1 - x0, y1 - y0) == (sx1 - sx0, sy1 - sy0)
        if baked is not None:
            alb[y0:y1, x0:x1] = baked["albedo"][sy0:sy1, sx0:sx1]
            nrm[y0:y1, x0:x1] = baked["normal"][sy0:sy1, sx0:sx1]
            orm[y0:y1, x0:x1, 0] = baked["ao"][sy0:sy1, sx0:sx1, 0]
            orm[y0:y1, x0:x1, 1] = baked["rough"][sy0:sy1, sx0:sx1, 0]
    # The wall glow on the pool: the stone's linear albedo x the lamp's warm light, a separable window
    # that is 1 at the lamp (the cell's centre) and 0 with zero slope on the cell's edges.
    p = isl["pool"]
    x0, y0, x1, y1 = p.px_box()
    a = (np.arange(x0, x1) + 0.5 - (p.x0 + GUT)) / TEXEL
    bb = (np.arange(y0, y1) + 0.5 - (p.y0 + GUT)) / TEXEL
    wu = np.clip(1.0 - ((a - PW / 2) / (PW / 2)) ** 2, 0.0, 1.0) ** 2
    wv = np.clip(1.0 - ((bb - PH / 2) / (PH / 2)) ** 2, 0.0, 1.0) ** 2
    win = wv[:, None] * wu[None, :]
    lin = np.power(np.clip(alb[y0:y1, x0:x1, :3], 0.0, 1.0), 2.2)
    emi[y0:y1, x0:x1, :3] = _enc(lin * np.array(WARM, np.float32) * GLOW * win[..., None])
    for name, albedo, emit, rough in (("glass", GLASS_ALBEDO, GLASS_EMIT, 0.2), ("iron", IRON, (0.0, 0.0, 0.0), 0.55)):
        x0, y0, x1, y1 = isl[name].px_box()
        alb[y0:y1, x0:x1, :3] = _enc(np.array(albedo, np.float32))
        nrm[y0:y1, x0:x1] = (0.5, 0.5, 1.0, 1.0)
        orm[y0:y1, x0:x1] = (1.0, rough, 0.0, 1.0)
        emi[y0:y1, x0:x1, :3] = _enc(np.array(emit, np.float32))
        msk[y0:y1, x0:x1] = (SNOW, 0.15, 0.0, 1.0)
    tp._fill_box(msk, isl["park"].px_box(), (SNOW, WET, 0.0, 1.0))
    # The floor: A lifted round the foot of every lamp (smoothstep falloff over POOL_R).
    f = isl["floor"]
    x0, y0, x1, y1 = f.px_box()
    xs = (np.arange(x0, x1) + 0.5 - (f.x0 + GUT)) / f.pw * 2 * AP - AP
    ts = (np.arange(y0, y1) + 0.5 - (f.y0 + GUT)) / f.ph * 2 * FLOOR_T - FLOOR_T
    lift = np.zeros((y1 - y0, x1 - x0), np.float32)
    for side in (1, -1):
        (sw, _), _ = lamp_wall(side)
        for xl in LAMP_X:
            dd = np.sqrt((xs[None, :] - xl) ** 2 + (ts[:, None] - sw) ** 2) / POOL_R
            q = np.clip(1.0 - dd, 0.0, 1.0)
            lift = np.maximum(lift, q * q * (3 - 2 * q))
    msk[y0:y1, x0:x1, 0] = SNOW
    msk[y0:y1, x0:x1, 1] = WET
    msk[y0:y1, x0:x1, 2] = 0.0
    msk[y0:y1, x0:x1, 3] = A_FLOOR + (A_POOL - A_FLOOR) * lift
    return alb, nrm, orm, msk, emi


def bake_panel(out):
    """The portal's templates baked as road_tunnel_portal bakes them; returns (portal islands, pixels)."""
    isl_p = tp.pack(tp._islands())
    highs, receiver = tp.build_templates(isl_p, tp.stone_materials("road_tunnel_portal"))
    signature = bake.receiver_signature([receiver])
    px, stats = None, {}
    if os.environ.get("HEXW_SKIP_BAKE") != "1":
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(PIECE)
        scn.world.light_settings.distance = tp.AO_DIST
        bake.setup_cycles(samples=tp.SAMPLES)
        kw = dict(cage_extrusion=tp.CAGE, max_ray=tp.MAX_RAY, margin=tp.MARGIN)
        imgs = {k: bake.new_image(f"{PIECE}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "ao", "rough")}
        bake.bake_emit_input(highs, receiver, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(receiver, "NORMAL", imgs["normal"], highs=highs, **kw)
        bake.bake_to(receiver, "ROUGHNESS", imgs["rough"], highs=highs, **kw)
        bake.bake_to(receiver, "AO", imgs["ao"], highs=highs, **kw)
        px = {}
        for k, im in imgs.items():
            path = bake.save_png(im, str(out / f"{PIECE}.{k}.tmp.png"))
            px[k] = bake.load_png(path)[1].reshape(ATLAS, ATLAS, 4)
            os.remove(path)
        x0, y0, x1, y1 = isl_p["panel"].px_box()
        stats["seam"] = {f"panel.{k}": tp.seam_errors(px[k], isl_p, ATLAS, "panel") for k in ("albedo", "normal", "ao")}
        stats["albedoLuma"] = float(px["albedo"][y0:y1, x0:x1, :3].mean())
        stats["ao"] = {"mean": float(px["ao"][y0:y1, x0:x1, 0].mean())}
        print(f"{PIECE}: bake stats {json.dumps(stats)}")
        bad = {k: v for k, v in stats["seam"].items() if max(v.values()) > (0.06 if k.endswith("ao") else 0.02)}
        assert not bad, f"the panel island does not wrap: {bad}"
    for o in list(highs) + [receiver]:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    return isl_p, px, signature, stats


def build_piece(generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(PIECE)
    isl = tp.pack(_islands())
    isl_p, baked, signature, stats = bake_panel(out)
    alb, nrm, orm, msk, emi = compose(isl, isl_p, baked)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{PIECE}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, tp._down(nrm, ATLAS, NORMAL_SIZE, renormal=True), str(out / f"{PIECE}.normal.png"))
    bake.write_png(ORM_SIZE, tp._down(orm, ATLAS, ORM_SIZE), str(out / f"{PIECE}.orm.png"))
    bake.write_png(MASK_SIZE, tp._down(msk, ATLAS, MASK_SIZE), str(out / f"{PIECE}.masks.png"))
    bake.write_png(EMIS_SIZE, tp._down(emi, ATLAS, EMIS_SIZE), str(out / f"{PIECE}.emissive.png"), non_color=False)

    mats = [materials.pbr_material(STONE, (0.30, 0.29, 0.27, 1.0), 0.9),
            materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
            materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
            materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
            materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6)]
    root = scene.empty(PIECE)
    lods = [build_lod(i, isl, mats) for i in range(3)]
    tris = [len(o.data.polygons) for o in lods]
    density = {"stone": tp.slot_density(lods[0], 0)}
    floor_min = budgets.min_lod0(tris[2], "prop", PIECE)
    print(f"{PIECE}: tris per LOD {tris} (LOD1 {tris[1] / tris[0]:.3f}, LOD2 {tris[2] / tris[0]:.3f} of LOD0; "
          f"LOD0 floor ceil({tris[2]} / 0.15) = {floor_min}), density {density}")
    budgets.check(tris, "prop", PIECE)
    assert density["stone"]["median"] >= 256.0, density
    # The collider: the tube's box (LOD2's stone), for the kit gate; the walker's corridor comes from the
    # runtime's tunnel, not this box (integration.md).
    tmp = lods[2].copy()
    tmp.data = lods[2].data.copy()
    scene.link(tmp)
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index != 0], context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    col = collider.collider_from(tmp, "box", f"{PIECE}_collider")
    bpy.data.objects.remove(tmp, do_unlink=True)
    for o in lods + [col]:
        o.parent = root
    export.export_glb(root, str(out / f"{PIECE}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, PIECE, generator)
    rep_path = out / "export-report.json"
    rep = json.loads(rep_path.read_text())
    rep["pngFallbackSize"] = {"albedo": PNG_ALBEDO}
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    (sw, hw), _ = lamp_wall(1)
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"),
        samples=tp.SAMPLES, marginPx=tp.MARGIN, cageExtrusionM=tp.CAGE, maxRayDistanceM=tp.MAX_RAY, aoDistanceM=tp.AO_DIST,
        channels={"albedo": {"size": ATLAS, "colorspace": "srgb"}, "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                  "orm": {"size": ORM_SIZE, "colorspace": "linear"}, "masks": {"size": MASK_SIZE, "colorspace": "linear"},
                  "emissive": {"size": EMIS_SIZE, "colorspace": "srgb"}},
        texelDensityPxPerM=density, bakeStats=stats, lodTris=tris, lod2Drops={},
        islands={k: [v.x0, v.y0, v.pw, v.ph] for k, v in sorted(isl.items())},
        bore={"ends": [round(-AP, 4), round(AP, 4)], "openingHalfSpan": A, "springing": J, "crown": round(CROWN, 4),
              "liningFoot": -BURY, "floorProfile": [list(p) for p in FLOOR_PROFILE], "lampX": [round(x, 4) for x in LAMP_X],
              "lampWall": [round(sw, 4), round(hw, 4)], "liningAo": A_LINING, "floorAo": [A_FLOOR, A_POOL],
              "glow": GLOW, "panelSource": "road_tunnel_portal templates (_tunnel_portal.build_templates)",
              "lod2Floor": floor_min},
        skipped=skip_bake)
    return root, lods
