"""river_rail_bridge_center (P3, PRD section 15 "bridges where rail meets river"): the single track of
`rail_straight_center` carried across the straight river channel at 60 degrees on the stone arch of
the approved `river_road_bridge_center` (refs `assets-source/refs/river_road_bridge`, "stone arch
road bridge crossing the river at 60 degrees", and `assets-source/refs/rail`, A17). It sits on
`river_straight_base`, which carries the whole bed, with the river on edges 0 and 3 and the rail on
edges 1 and 4.

Edge contract. The rail is `_rail.track` along `_rail.straight_frames`, turned by one edge step
(`hexgeo.rotate_xy(.., 1)`), so it runs from the edge-4 midpoint to the edge-1 midpoint and its
section at both borders is `_rail.section()` in every LOD (`_rail.border_section_errors`). Nothing
else reaches a border.

Level deck. The rail keeps rail_straight_center's heights all the way across (ballast top 0.22 m,
rail top 0.46 m): a 0% grade, so a train stands at the same 0.46 m rail top as on every other rail
piece. The bridge is the road bridge's causeway narrowed to the ballast: `_river_bridge`'s module
globals are set once below (the `_spruce` pattern; `_river_bridge.py` itself is unchanged), so its
`causeway`, `parapets` and `dressing` build:
- skew face walls on t = +-W (W = the ballast's outer sink edge, 2.9 m) from FOOT_Z up to DECK_Z
  (-0.31 m, 1 cm under the ballast's sink edge at -0.30 m, the same 1 cm the road bridge leaves
  under its road), the same elliptical arch across the river (springing +-3.6 m at -1.45 m, crown
  -0.45 m: 0.95 m over the -1.4 m water, as on the road bridge) and its barrel;
- parapets on t in PARAPET_T over |y| <= 5 m (the water and lower banks), inner face 2.6 m from the
  track centre (the train is 1.48 m half-wide and sweeps 2.5 m), top PARAPET_TOP; end pillars;
- bank stones and reeds clear of the causeway (|t| > W + 1.2).

Masks are analytic (no Cycles bake), on the planar UV0: rail_straight_center's crib, sleeper and
rail field in the rail's own frame over the ballast band, the river dressing's porosity elsewhere,
and the stone block where every causeway loop is parked (`_river_bridge.STONE_UV`, a world-space
tileable slot). R 1 and B 0 everywhere; A 1 off the cribs.
"""
import json
import math
import pathlib
import sys

import bpy

import numpy as np

from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene, uv

import _rail
import _river_bridge
import _river_dress

W = _rail.SINK_T  # 2.9: the causeway's half-width = the ballast profile's outer edge
PARAPET_T = (W - 0.3, W)
PARAPET_TOP = 1.0  # 0.78 m above the ballast top, 0.54 m above the rail top
PILLAR = 0.2  # pillar half-size: inner face at 2.55 m, outside the 2.5 m swept limit
PILLAR_TOP = 1.2
SWEPT = 2.5  # train swept half-width from the track centre (`_train.py` curve clearance)
TRAIN_HALF_W = 1.48

_river_bridge.W = W
_river_bridge.PARAPET_T = PARAPET_T
_river_bridge.PARAPET_TOP = PARAPET_TOP
_river_bridge.PILLAR = PILLAR
_river_bridge.PILLAR_TOP = PILLAR_TOP
# The causeway top sits 1 cm under the ballast's sink edge, as it does under the road's.
assert abs(_river_bridge.DECK_Z - (_rail.SINK_Z - 0.01)) < 1e-9, (_river_bridge.DECK_Z, _rail.SINK_Z)
assert PARAPET_T[0] > SWEPT and (PARAPET_T[0] + PARAPET_T[1]) / 2 - PILLAR > SWEPT

RAIL_SEGMENTS = (8, 2, 1)  # = rail_straight_center
ARCH_SEGMENTS = _river_bridge.ARCH_SEGMENTS
LANES = (1, 4)
STONE_SLOT = _river_bridge.STONE_SLOT
SLOTS = _rail.SLOTS + (STONE_SLOT,) + tuple(_river_dress.DRESS_SLOTS)
SLOT_INDEX = {s: i for i, s in enumerate(SLOTS)}
MASK_SIZE = _rail.MASK_SIZE


def rail(name, lod, mats):
    """rail_straight_center's LOD `lod` turned by one edge step (lanes 0, 3 -> 1, 4)."""
    m = _rail.track(_rail.straight_frames(RAIL_SEGMENTS[lod]), _rail.straight_sleepers() if lod < 2 else None,
                    top_only=lod == 1)
    m.verts = [(*hexgeo.rotate_xy((x, y), 1), z) for x, y, z in m.verts]
    o = m.obj(name, mats)
    for s in SLOTS[len(_rail.SLOTS):]:
        o.data.materials.append(mats[s])
    _rail.lifted_uv(o)
    o.data.shade_smooth()
    return o


def stone_and_dressing(piece, lod, mats):
    """The road bridge's causeway, parapets (LOD0 and LOD1) and bank dressing at this width."""
    cw = _river_bridge.causeway(f"{piece}_lod{lod}_causeway", ARCH_SEGMENTS[lod])
    stone = [cw] + ([_river_bridge.parapets(f"{piece}_lod{lod}_parapets")] if lod < 2 else [])
    dress, n_st, n_re = _river_bridge.dressing(f"{piece}_lod{lod}_dress", lod, piece)
    dress_index = [p.material_index for p in dress.data.polygons]
    for o in stone + [dress]:
        o.data.materials.clear()
        for s in SLOTS:
            o.data.materials.append(mats[s])
        uv.planar_cap_uv(o)
    for o in stone:
        for p in o.data.polygons:
            p.material_index = SLOT_INDEX[STONE_SLOT]
            p.use_smooth = False
        for d in o.data.uv_layers["UVMap"].data:
            d.uv = _river_bridge.STONE_UV
    for p, i in zip(dress.data.polygons, dress_index):
        p.material_index = i + SLOT_INDEX[_river_dress.STONE]
    return stone, dress, n_st, n_re


# --- masks --------------------------------------------------------------------------------------

def rail_field(r, t):
    """(wet, ao) of `_rail.masks_field("straight")` at along-track r and across-track t."""
    centres = np.array(_rail.sleeper_angles_or_x("straight"))
    along = np.min(np.abs(r[..., None] - centres[None, None, :]), axis=-1)
    on = np.abs(r) <= hexgeo.APOTHEM
    at = np.abs(t)
    gap = np.clip(along - _rail.SLEEPER_HALF_W, 0.0, None)
    crib = 0.78 + 0.17 * np.clip(gap / 0.2, 0.0, 1.0)
    ao = np.ones_like(r)
    ao = np.where(on & (at <= _rail.SLEEPER_HALF_L), crib, ao)
    ao = np.where(on & (at > _rail.SLEEPER_HALF_L) & (at <= _rail.SHOULDER_T), 0.95, ao)
    rails = on & (np.abs(at - _rail.RAIL_T) <= 0.06)
    sleeper = on & (at <= _rail.SLEEPER_HALF_L) & (gap <= 0.0)
    ao = np.where(rails | sleeper, 1.0, ao)
    wet = np.full_like(r, 0.9)
    wet = np.where(sleeper, 0.6, wet)
    wet = np.where(rails, 0.15, wet)
    return wet, ao


def masks_field():
    """(wet, ao) per texel in Blender pixel order: the rail field over the ballast band (|t| <= W),
    the river dressing's porosity elsewhere, and the parked stone block."""
    n = MASK_SIZE
    c = (np.arange(n, dtype=np.float64) + 0.5) / n * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    r = _river_bridge.road_r(xs, ys)
    t = _river_bridge.road_t(xs, ys)
    rail_wet, ao = rail_field(r, t)
    wet = np.where(np.abs(t) <= W, rail_wet, _river_dress.porosity(np.abs(ys)))
    ao = np.where(np.abs(t) <= W, ao, 1.0)
    b = _river_bridge.STONE_BLOCK
    wet[:b, :b] = _river_bridge.STONE_POROSITY
    ao[:b, :b] = 1.0
    return wet.ravel().astype(np.float32), ao.ravel().astype(np.float32)


# --- checks -------------------------------------------------------------------------------------

def clearance(objs):
    """Smallest |t| of any stone vertex above the ballast top (what a train could touch)."""
    best = math.inf
    for o in objs:
        for v in o.data.vertices:
            w = o.matrix_world @ v.co
            if w.z > _rail.BALLAST_TOP:
                best = min(best, abs(_river_bridge.road_t(w.x, w.y)))
    return best


# --- the piece ----------------------------------------------------------------------------------

def build_piece(piece, generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        _rail.BALLAST: materials.pbr_material(_rail.BALLAST, (0.205, 0.2, 0.21, 1.0), 0.95),
        _rail.SLEEPER: materials.pbr_material(_rail.SLEEPER, (*_rail.SLEEPER_COLOR, 1.0), _rail.SLEEPER_ROUGH),
        _rail.STEEL: materials.pbr_material(_rail.STEEL, (*_rail.STEEL_COLOR, 1.0), _rail.STEEL_ROUGH),
        _rail.HEAD: materials.pbr_material(_rail.HEAD, (*_rail.HEAD_COLOR, 1.0), _rail.HEAD_ROUGH),
        STONE_SLOT: materials.pbr_material(STONE_SLOT, materials.srgb("#8f877a"), 0.85),
        _river_dress.STONE: materials.pbr_material(_river_dress.STONE, materials.srgb("#8a8378"), 0.8),
        _river_dress.REED: materials.pbr_material(_river_dress.REED, materials.srgb("#5d7a35"), 0.85),
        _river_dress.CATTAIL: materials.pbr_material(_river_dress.CATTAIL, materials.srgb("#5a3a22"), 0.9),
    }
    lods, rails, stats, clear = [], [], {}, []
    for li in range(3):
        tr = rail(f"{piece}_lod{li}_rail", li, mats)
        stone, dress, n_st, n_re = stone_and_dressing(piece, li, mats)
        stats = {"stones": n_st, "reedClumps": n_re}
        clear.append(clearance(stone))
        rails.append(tr)
        texel = uv.measure_texel_density(tr, MASK_SIZE) if li == 0 else texel
        body = geom.join([tr] + stone + [dress], f"{piece}_lod{li}")
        print(f"{piece}_lod{li}: wind U max {_river_dress.wind_uv(body):.3f}")
        body.data.set_sharp_from_angle(angle=math.radians(30.0))
        assert [mt.name for mt in body.data.materials] == list(SLOTS), [mt.name for mt in body.data.materials]
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}; parapet clearance from the track centre {clear}")
    errs = [e for o in lods for e in _rail.border_section_errors(o, LANES) + _rail.interior_errors(o, LANES)]
    for e in errs[:12]:
        print("CHECK:", e)
    assert not errs, f"{len(errs)} section errors"
    assert min(clear) > SWEPT, clear
    budgets.check(tris, "center", piece)
    for o in lods:
        o.parent = root

    signature = bake.receiver_signature([lods[0]])
    wet, ao = masks_field()
    bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, None, str(out / f"{piece}.masks.png"))
    _, px = bake.load_png(str(out / f"{piece}.masks.png"))
    px[:, 3] = ao
    bake.write_png(MASK_SIZE, px, str(out / f"{piece}.masks.png"))

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    for n, k in enumerate(LANES):
        c = hexgeo.lane_frame(k, 0.0, _rail.LANE_WIDTH)["center"]
        export.add_socket(f"socket-lane-{n}", (*c, 0.0), root, kind="rail", edge=k, offset=0.0,
                          width=_rail.LANE_WIDTH)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=texel,
        reason="no Cycles bake: the masks are an analytic field (the rail's crib field on the ballast band, "
               "the river dressing's porosity, the parked stone block)",
        masks={"snow": 1.0, "porosity": {"ballast": 0.9, "sleeper": 0.6, "rail": 0.15,
                                         "stone": _river_bridge.STONE_POROSITY, "dressing": list(_river_dress.POROSITY)},
               "ao": {"crib": [0.78, 0.95], "shoulder": 0.95, "other": 1.0}, "night": 0.0,
               "stoneUv": list(_river_bridge.STONE_UV), "stoneBlockPx": _river_bridge.STONE_BLOCK},
        bridge={"railLanes": list(LANES), "riverLanes": [0, 3], "angleDeg": 60.0, "railTopM": _rail.RAIL_TOP,
                "ballastTopM": _rail.BALLAST_TOP, "gradePct": 0.0, "causewayHalfWidthM": W,
                "deckZ": _river_bridge.DECK_Z, "archHalfSpanM": _river_bridge.ARCH_A,
                "springZ": _river_bridge.SPRING_Z, "archRiseM": _river_bridge.ARCH_RISE,
                "archCrownZ": _river_bridge.SPRING_Z + _river_bridge.ARCH_RISE,
                "waterZ": -1.4, "crownOverWaterM": _river_bridge.SPRING_Z + _river_bridge.ARCH_RISE + 1.4,
                "parapetT": list(PARAPET_T), "parapetTopM": PARAPET_TOP, "pillarHalfM": PILLAR,
                "pillarTopM": PILLAR_TOP, "parapetHalfLengthAcrossRiverM": _river_bridge.PARAPET_Y,
                "minStoneClearanceFromTrackM": min(clear), "trainHalfWidthM": TRAIN_HALF_W,
                "sweptHalfWidthM": SWEPT},
        dressing=stats, lodTris=tris, skipped=False)
    print(json.dumps({"piece": piece, "tris": tris, "texel": texel}))
    return root, lods
