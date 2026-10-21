"""Kit triangle budgets, so generators can size their LODs instead of trial-building against kit_gate.

Mirrors crates/hex-assets/src/kit_manifest.rs (PieceKind::lod0_budget, LOD1/LOD2_MAX_RATIO) and
crates/hex-play/tests/kit_gate.rs (lane-cut, coast-arc and river-dressing exceptions);
kit_gate::budgets_match_blender_lib pins every number below to those constants. No bpy import.
"""

import math

LOD0_MAX = {"base": 4000, "edge": 4000, "center": 4000, "hero": 12000, "prop": 1500, "tree": 3000}
LOD1_MAX_RATIO = 0.5
LOD2_MAX_RATIO = 0.15
LANE_CUT_LOD2_MAX_RATIO = 0.20
COAST_ARC_LOD2_MAX_RATIO = 0.20
RIVER_DRESSING_LOD0_MAX = 2500
RIVER_DRESSING_PROPS = ("river_straight_center", "river_bend_center", "river_spring_center",
                        "river_mouth_center", "river_mouth_l_center", "river_fork_center")


def lod_budget(kind, piece_id="", lane_cut=False, coast_arc=False):
    """(lod0_max, lod1_max_ratio, lod2_max_ratio) as kit_gate applies them. lane_cut: a base with a
    `socket-lane-*` that has a `cut` (river bases); coast_arc: a coast-corner base (beach_outer_base,
    beach_inner_base). The gate fails LOD i when tris_i > ratio_i * tris_0."""
    lod0 = RIVER_DRESSING_LOD0_MAX if kind == "prop" and piece_id in RIVER_DRESSING_PROPS else LOD0_MAX[kind]
    lod2 = LOD2_MAX_RATIO
    if kind == "base" and lane_cut:
        lod2 = LANE_CUT_LOD2_MAX_RATIO
    elif kind == "base" and coast_arc:
        lod2 = COAST_ARC_LOD2_MAX_RATIO
    return lod0, LOD1_MAX_RATIO, lod2


def lod_targets(lod0_tris, kind, piece_id="", lane_cut=False, coast_arc=False):
    """Largest LOD1 and LOD2 triangle counts that pass for a given LOD0 (floored)."""
    _, r1, r2 = lod_budget(kind, piece_id, lane_cut, coast_arc)
    return math.floor(r1 * lod0_tris), math.floor(r2 * lod0_tris)


def min_lod0(fixed_lod2_tris, kind, piece_id="", lane_cut=False, coast_arc=False):
    """Smallest LOD0 whose LOD2 ceiling holds a LOD2 fixed at fixed_lod2_tris (ring, skirt, swept
    profile at one segment): the gates.md LOD floor, LOD0 >= LOD2_min / ratio. Raises ValueError when
    that exceeds the kind's LOD0 budget, since no LOD0 can then pass."""
    lod0_max, _, r2 = lod_budget(kind, piece_id, lane_cut, coast_arc)
    need = math.ceil(fixed_lod2_tris / r2 - 1e-9)
    if need > lod0_max:
        raise ValueError(f"LOD2 of {fixed_lod2_tris} tris needs LOD0 >= {need}, over the {kind} budget {lod0_max}")
    return need


def check(tris, kind, piece_id="", lane_cut=False, coast_arc=False):
    """Raise AssertionError with every budget the (lod0, lod1, lod2) triangle counts break."""
    lod0_max, r1, r2 = lod_budget(kind, piece_id, lane_cut, coast_arc)
    t0, t1, t2 = tris
    errs = [f"LOD0 {t0} > {lod0_max}"] if t0 > lod0_max else []
    errs += [f"LOD{i} {t} > {r} x LOD0 {t0}" for i, t, r in ((1, t1, r1), (2, t2, r2)) if t > r * t0]
    if errs:
        raise AssertionError(f"{piece_id or kind}: " + "; ".join(errs))
