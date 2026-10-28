"""Scatter oaks for the forest tiles (tree_oak, tree_oak_b): the hero oak's look at scatter size and
the `piece` texture budget. Refs: assets-source/refs/forest ("spruce among oaks").

The approved hero helpers (`_tree`, `_oak`, `_leaves`) are reused unchanged: their functions read
their module globals at call time, so `configure` points them at this piece (seed, canopy sectors,
shell rings, crown tiers) and asserts the invariants the pipeline relies on. Only the monolithic
steps of `tree_hero_oak.py` (the atlas pack, masks, canopy gaps, LOD joins) are copied here.

Differences from the hero:
- A scripted, single-trunk skeleton (`skeleton`): one tapering, leaning trunk running up into a
  top clump as the leader, short buttress roots, limbs to the low tier, secondaries to the middle
  tier, leaders to the other top clumps. Same sway convention (`_tree` SWAY_*).
- A 1024 atlas (KTX2 class `piece`), so the clumps are smaller (radius ~1.3-1.5 m) and the shared
  canopy master is SECTORS = 4-fold: one sector shell island then covers a quarter dome, which
  holds the canopy at >= 256 px/m. The master is `_oak.canopy_high` with the hero's seed (same
  leaves and palette), 360 leaves per sector (the hero's 1440 in all).
- The fringe cards are `_oak.leaf_cards` scaled about their centres to CARD_SCALE (the 256 px leaf
  tiles hold 256 px/m on cards up to ~0.97 m); the leaf tiles are the hero's (`_leaves`, hero seed).
- masks at 512 (bark AO ~16 px/m), and no live .blend: every high poly is built here.

Leaf roughness is matte (owner A35 O1, 2026-10-27): the shell's 0.62 and the leaf strip's
0.50-0.66 go through `_verge_bush.matte` (texels under 0.7 map linearly onto 0.76-0.86) before
the canopy gaps blend toward 1.0, as the verge bushes and woodland shrubs were matted."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv
import _leaves
import _oak
import _tree
import _verge_bush
from _shared import island_texels

HERO = "tree_hero_oak"  # the leaf tiles' and canopy master's seed: every oak shows the hero's leaves
ATLAS, MASK_SIZE = 1024, 512
# ORM ships box-filtered to 512 (as rock_small_b/c): at 1024 the four maps measured 1,513,229 B of
# BC7, over the 1,500,000 B piece budget; albedo (alpha edges, measured density) and normal (leaf
# relief) stay at 1024.
ORM_SIZE = 512
BARK_SET_PX, BARK_TILE_M = 1024, 2.0  # the `bark` tileable set (scripts/blender/kit/mat_bark.py)
BARK_UV_PER_M = 1 / 32  # masks AO at MASK_SIZE / 32 = 16 px/m; tileUvScale = 1 / (uvPerM x 2 m) = 16
TILE_UV_SCALE = 1 / (BARK_UV_PER_M * BARK_TILE_M)
SECTORS = 4
# Shell rings (normal elevation psi in degrees, columns per sector): a coarser `_oak.SHELL_RINGS`
# for clumps about 0.6x the hero's, 16 columns round the rim.
SHELL_RINGS = ((-60, 2), (-30, 3), (0, 4), (30, 4), (60, 3))
CANOPY_LEAVES_PER_SECTOR = 360
FRINGE_PER_CLUMP = 6
CARD_SCALE = 0.65  # `_oak.leaf_cards` draws 1.1-1.5 m cards: 0.72-0.98 m here
CAGE, MAX_RAY, MARGIN, SAMPLES = 0.25, 0.5, 8, 32  # bark AO, as the hero
SHELL_CAGE, SHELL_RAY = 0.45, 0.9  # canopy master leaves stack up to ~0.25 m off the dome (hero)
AO_DIST, SHELL_AO_DIST = 0.35, 0.3
SHELL_PACK_MARGIN = 0.02  # UV between the bark column and the shell: 10 masks texels at 512
# Packed share of the bark column tried, tightest first; a slim tree's trunk island is wider than a
# tight column of its small total area, so the looser shares follow.
BARK_FILLS = (0.6, 0.55, 0.5, 0.45, 0.4, 0.35, 0.3, 0.25, 0.2, 0.15)
SHELL_UNWRAP = "MINIMUM_STRETCH"
GROUND_CULL_Z = -0.27  # below the forest relief's floor (-0.25 m)
CANOPY_SNOW, WETNESS, NIGHT = 0.85, 0.5, 0.0  # masks R scale, G, B, as the hero
GAP_TONE = np.array([0x1F, 0x38, 0x12], np.float32) / 255  # `_oak.canopy_high`'s under tone (hero)
GAP_RAMP, GAP_ALBEDO, GAP_ROUGH = (0.01, 0.14), 0.5, 1.0


def configure(piece: str, tiers: tuple) -> None:
    """Point the hero helpers at `piece`: seeds (branch, card and clump phases, clump lumps), the
    canopy sector count, the shell rings and the crown tiers."""
    _tree.SEED = piece
    _oak.SEED = piece
    _tree.SECTORS = _oak.SECTORS = SECTORS
    _oak.SHELL_RINGS = SHELL_RINGS
    _oak.SHELL_COLUMNS = tuple(c for _, c in SHELL_RINGS)
    _oak.SHELL_PHIS = tuple(round(math.atan((_oak.DOME_TOP if p >= 0 else _oak.DOME_BASE) / _oak.DOME_WIDE
                                            * math.tan(math.radians(p))), 4) for p, _ in SHELL_RINGS)
    _tree.TIERS = tiers
    # Invariants the copied steps rely on.
    assert _tree.SECTORS == _oak.SECTORS == SECTORS
    assert len(_oak.SHELL_PHIS) == len(_oak.SHELL_COLUMNS)
    assert (_oak.R0, _oak.DOME_WIDE, _oak.DOME_TOP, _oak.DOME_BASE) == (_tree.R0, _tree.DOME_WIDE, _tree.DOME_TOP,
                                                                       _tree.DOME_BASE)
    assert _tree.LEAF_V0 == 0.75 and _tree.LEAF_TILES == _leaves.TILES == 4
    assert round((1 - _tree.LEAF_V0) * ATLAS) == ATLAS // _leaves.TILES  # square leaf tiles


def _smoothstep(x: float) -> float:
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def _at_z(pts: list[Vector], z: float) -> Vector:
    """Point on a polyline (rising in z) at height z."""
    for a, b in zip(pts, pts[1:]):
        if a.z <= z <= b.z:
            return a.lerp(b, (z - a.z) / max(b.z - a.z, 1e-9))
    return pts[-1].copy()


def skeleton(crown: list[dict], seed: str, s: dict) -> list[dict]:
    """Branches as `_tree.skeleton` returns them ({id, level, points, radii, sway}), for one trunk:
    level 0 the trunk (running up into the first top-tier clump as the leader) and the roots;
    level 1 limbs (one per low-tier clump) and leaders (the other top clumps); level 2 secondaries
    (from the nearest limb to each middle-tier clump). Branch ends sit inside their clump."""
    rng = random.Random(seed + ":skeleton")
    out: list[dict] = []
    tops = [c for c in crown if c["tier"] == 2]
    top = tops[0]["centre"] - Vector((0, 0, 0.4))
    pts, radii = [], []
    n = 28
    for k in range(n + 1):
        t = k / n
        z = -0.3 + (top.z + 0.3) * t
        lean = _smoothstep((z - 1.0) / (top.z - 1.0)) if top.z > 1.0 else t
        w = 0.24 * math.sin(5.0 * t + 1.3) * t * (1 - t)  # 0 at the base: the trunk axis stays on the pivot
        p = Vector((top.x * lean + w, top.y * lean - w * 0.6, z))
        pts.append(p)
        radii.append(s["trunk_r1"] + (s["trunk_r0"] - s["trunk_r1"]) * (1 - t) ** 1.2
                     + s["flare"] * max(0.0, 1 - max(z, 0.0) / 0.9) ** 2)
    trunk, trunk_radii = pts, radii
    out.append({"level": 0, "points": pts, "radii": radii, "sway": (0.0, _tree.SWAY_TRUNK)})

    def buried_r(parent_radius: float, sides: int, want: float) -> float:
        """A child's start radius that stays inside the parent's inscribed low tube (less
        `cull_buried`'s 0.02 m margin), so its start ring and cap are culled: no pale collar where
        a branch leaves its parent (pitfalls, "Branch starts at one height read as a collar")."""
        return min(want, 0.9 * (parent_radius * math.cos(math.pi / sides) - 0.02))

    def trunk_r_at(z: float) -> float:
        k_ = min(range(len(trunk)), key=lambda j: abs(trunk[j].z - z))
        return trunk_radii[k_]
    # Buttress roots (as `_oak.skeleton`): start on the trunk centreline, thinner than its inscribed
    # 6-sided tube so the start ring and cap are culled, leave it at spread heights, run down along
    # it first (outward power 1.5) and sink below the ground at the tip.
    for i in range(s["roots"]):
        a = 2 * math.pi * (i + rng.uniform(-0.2, 0.2)) / s["roots"]
        reach = rng.uniform(*s["root_reach"])
        z0 = s["root_z0"][0] + (s["root_z0"][1] - s["root_z0"][0]) * _tree.hash01(f"root_z0:{i}")
        c0 = _at_z(trunk, z0)
        rp, rr = [], []
        for k in range(13):
            t = k / 12
            e = Vector((reach * math.cos(a), reach * math.sin(a)))
            h = c0.xy + (e - c0.xy) * t ** 1.5
            rp.append(Vector((h.x, h.y, z0 * (1 - t) ** 2.0 - 0.35 * t)))
            rise = s["root_r"][0] + (s["root_r"][1] - s["root_r"][0]) * _smoothstep(t / 0.22)
            rr.append(min(rise, 0.04 + (s["root_r"][1] + 0.03 - 0.04) * (1 - t) ** 0.6))
        out.append({"level": 0, "points": _tree._smooth(rp), "radii": rr, "sway": (0.0, 0.0)})
    limbs = []
    for cl in [c for c in crown if c["tier"] == 0]:
        c = cl["centre"]
        a = math.atan2(c.y, c.x)
        z0 = rng.uniform(*s["limb_z"])
        start = _at_z(trunk, z0)  # on the trunk's centreline
        end = c - Vector((0, 0, 0.5))
        lp = _tree._arc(rng, start, end, 0.15, s["wobble"], 14)
        for k, p in enumerate(lp):  # the hero's limbs: low first, rising into the clump
            t = k / (len(lp) - 1)
            p.z = start.z + (end.z - start.z) * t ** 1.6 + (p.z - (start.z + (end.z - start.z) * t))
        r0 = buried_r(trunk_r_at(z0), 6, s["limb_r"][0])
        lr_ = _tree._lerp_radius(r0, s["limb_r"][1], len(lp), 0.8)
        limbs.append((lp, lr_))
        out.append({"level": 1, "points": lp, "radii": lr_, "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    for cl in [c for c in crown if c["tier"] == 1]:
        c = cl["centre"]
        best, best_r = min(limbs, key=lambda q: (q[0][len(q[0]) // 2] - c).length)
        t = 0.45
        k_ = int(t * (len(best) - 1))
        base = best[k_]
        s2 = _tree.SWAY_TRUNK + (_tree.SWAY_LIMB - _tree.SWAY_TRUNK) * t
        r0 = buried_r(best_r[k_], 5, s["sec_r"][0])
        out.append({"level": 2, "points": _tree._arc(rng, base, c - Vector((0, 0, 0.3)), 0.3, s["wobble"], 10),
                    "radii": _tree._lerp_radius(r0, s["sec_r"][1], 11), "sway": (s2, _tree.SWAY_TWIG)})
    for cl in tops[1:]:
        c = cl["centre"]
        start = _at_z(trunk, s["leader_z"])
        r0 = buried_r(trunk_r_at(s["leader_z"]), 6, s["leader_r"][0])
        out.append({"level": 1, "points": _tree._arc(rng, start, c - Vector((0, 0, 0.4)), 0.2, s["wobble"], 12),
                    "radii": _tree._lerp_radius(r0, s["leader_r"][1], 13, 0.9),
                    "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    for i, b in enumerate(out):
        b["id"] = i
    return out


def _set_pins(mesh, value):
    pin = mesh.attributes.get(".pn.UVMap") or mesh.attributes.new(".pn.UVMap", "BOOLEAN", "CORNER")
    pin.data.foreach_set("value", [value] * len(mesh.loops))


def uv_area(obj) -> float:
    uvd, total = obj.data.uv_layers["UVMap"].data, 0.0
    for poly in obj.data.polygons:
        t = [uvd[li].uv for li in poly.loop_indices]
        for i in range(1, len(t) - 1):
            total += abs((t[i].x - t[0].x) * (t[i + 1].y - t[0].y) - (t[i + 1].x - t[0].x) * (t[i].y - t[0].y)) / 2
    return total


def pack_below_strip(obj, rotate, scale, margin, margin_method="SCALED", right=1.0, left=0.0):
    """`tree_hero_oak.pack_below_strip`: pack obj's UVMap islands into left < u < right,
    v < LEAF_V0, against pinned placeholder rectangles. Returns the islands' v max, or None when
    they do not fit."""
    rects = ([(0.0, _tree.LEAF_V0, 1.0, 1.0)] + ([(right, 0.0, 1.0, _tree.LEAF_V0)] if right < 1.0 else [])
             + ([(0.0, 0.0, left, _tree.LEAF_V0)] if left > 0.0 else []))
    ph_mesh = bpy.data.meshes.new("leaf_strip_placeholder")
    ph_mesh.from_pydata([(x, y, -90) for _ in rects for x, y in ((0, 0), (1, 0), (1, 1), (0, 1))], [],
                        [[4 * i, 4 * i + 1, 4 * i + 2, 4 * i + 3] for i in range(len(rects))])
    ph = scene.link(bpy.data.objects.new("leaf_strip_placeholder", ph_mesh))
    ph_uv = ph_mesh.uv_layers.new(name="UVMap")
    corners = [c for u0, v0, u1, v1 in rects for c in ((u0, v0), (u1, v0), (u1, v1), (u0, v1))]
    for loop, co in zip(ph_mesh.loops, corners):
        ph_uv.data[loop.index].uv = co
    _set_pins(ph_mesh, True)
    scene.select_only([obj, ph], obj)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=rotate, scale=scale, margin_method=margin_method, margin=margin,
                            pin=True, pin_method="LOCKED")
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.data.objects.remove(ph, do_unlink=True)
    bpy.context.view_layer.update()
    a = np.empty(len(obj.data.loops) * 2)
    obj.data.uv_layers["UVMap"].data.foreach_get("uv", a)
    vmax = float(a[1::2].max())
    fits = (vmax < _tree.LEAF_V0 and a.min() >= 0.0 and float(a[0::2].max()) <= right and a.max() <= 1.0
            and float(a[0::2].min()) >= left)
    return vmax if fits else None


def dilate(mask, n):
    m = mask.copy()
    for _ in range(n):
        g = m.copy()
        g[1:] |= m[:-1]
        g[:-1] |= m[1:]
        g[:, 1:] |= m[:, :-1]
        g[:, :-1] |= m[:, 1:]
        m = g
    return m


def stats(a):
    return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
            "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}


def srgb_to_lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def lin_to_srgb(c):
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.maximum(c, 0.0031308) ** (1 / 2.4) - 0.055)


def copy_of(src, name):
    o = src.copy()
    o.data = src.data.copy()
    o.name = o.data.name = name
    return scene.link(o)


def scale_cards(obj, factor):
    """Scale every card (face) of obj about its centre."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    for f in bm.faces:
        c = f.calc_center_median()
        for v in f.verts:
            v.co = c + (v.co - c) * factor
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def card_subset(src, every, scale_, name):
    """`tree_hero_oak.card_subset`: every `every`-th card, scaled about its centre (LOD1/LOD2)."""
    bm = bmesh.new()
    bm.from_mesh(src.data)
    bm.faces.ensure_lookup_table()
    drop = [f for i, f in enumerate(bm.faces) if i % every]
    for f in [f for i, f in enumerate(bm.faces) if not i % every]:
        c = f.calc_center_median()
        for v in f.verts:
            v.co = c + (v.co - c) * scale_
    bmesh.ops.delete(bm, geom=drop, context="FACES")
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    for m_ in src.data.materials:
        mesh.materials.append(m_)
    return scene.link(bpy.data.objects.new(name, mesh))


def radial_max(objs) -> float:
    """Largest horizontal vertex distance from the trunk axis (Blender x, y) over objs."""
    return max(math.hypot(v.co.x, v.co.y) for o in objs for v in o.data.vertices)


def build(piece: str, out: pathlib.Path, generator: str, *, tiers: tuple, shape: dict) -> None:
    """The scatter-oak pipeline for `piece`: skeleton and crown, LOD0 bark, canopy shells and
    fringe, atlas pack, bakes (canopy master -> shell, bark AO -> masks), maps, LODs, collider,
    export and reports into `out`. `generator` is the piece's `__file__`."""
    configure(piece, tiers)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    scn = bpy.context.scene
    scn.world = scn.world or bpy.data.worlds.new(piece)
    canopy_mat_name = f"{piece}_canopy"
    bark_mat = materials.pbr_material(f"{piece}_bark", materials.srgb("#6b5444"), 0.85)
    leaf_mat = materials.pbr_material(f"{piece}_leaves", materials.srgb("#4f7a2c"), 0.6)
    canopy_mat = materials.pbr_material(canopy_mat_name, materials.srgb("#3f6a24"), 0.62)
    crown = _tree.layout(piece)
    skel = skeleton(crown, piece, shape)

    # LOD0 bark receiver: closed tubes per level, metric UVs, buried / in-crown / underground faces culled.
    trunk_and_roots = [b for b in skel if b["level"] == 0]
    limbs = [b for b in skel if b["level"] == 1]
    secs = [b for b in skel if b["level"] == 2]
    groups = ((trunk_and_roots[:1], 0, 6, shape["trunk_step"]), (trunk_and_roots[1:], 0, 5, 0.55),
              (limbs, 1, 5, shape["limb_step"]), (secs, 2, 4, shape["sec_step"]))
    groups = tuple(g for g in groups if g[0])
    bark_sides = {b["id"]: n for bs, _, n, _ in groups for b in bs}
    parts = [_tree.tube_mesh(f"bark_{i}", bs, {lv: n}, {lv: st}) for i, (bs, lv, n, st) in enumerate(groups)]
    for part, (bs, _, n, st) in zip(parts, groups):
        _oak.metric_tube_uv(part, bs, n, st)
    bark_low = geom.join(parts, "bark_low")
    faces_before = len(bark_low.data.polygons)
    culled = _tree.cull_buried(bark_low, skel, bark_sides)
    culled_crown = _tree.cull_inside_crown(bark_low, crown)
    culled_ground = _oak.cull_below(bark_low, GROUND_CULL_Z)
    print(f"{piece}: culled {culled} buried, {culled_crown} in-crown and {culled_ground} underground "
          f"of {faces_before} bark faces")
    bark_low.data.materials.append(bark_mat)
    lod.triangulate(bark_low)
    me = bark_low.data
    bv = np.empty(len(me.loops) * 2)
    me.uv_layers["UVMap"].data.foreach_get("uv", bv)
    me.uv_layers["UVMap"].data.foreach_set("uv", bv * BARK_UV_PER_M)
    bark_area = uv_area(bark_low)

    # Canopy shell: one SECTORS-th of the closed dome, one island, packed beside the bark column.
    shell = _oak.dome_sector("shell_base")
    scene.select_only([shell])
    shell.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.unwrap(method=SHELL_UNWRAP, margin=0.01)
    bpy.ops.uv.pack_islands(rotate=True, margin=0.01)
    bpy.ops.object.mode_set(mode="OBJECT")
    shell_uv = np.empty(len(shell.data.loops) * 2)
    shell.data.uv_layers["UVMap"].data.foreach_get("uv", shell_uv)
    shell_vmax = bark_vmax = None
    for fill in BARK_FILLS:
        bark_right = min(1.0, bark_area / fill / _tree.LEAF_V0)
        bark_vmax = pack_below_strip(bark_low, rotate=False, scale=False, margin=0.003, right=bark_right)
        if bark_vmax is None:
            continue
        shell.data.uv_layers["UVMap"].data.foreach_set("uv", shell_uv)
        shell_vmax = pack_below_strip(shell, rotate=True, scale=True, margin=SHELL_PACK_MARGIN,
                                      margin_method="FRACTION", left=bark_right + SHELL_PACK_MARGIN)
        if shell_vmax is not None:
            break
    assert shell_vmax is not None, f"bark islands (UV area {bark_area:.4f}) and the shell fit no column"
    shell_area = uv_area(shell)
    print(f"{piece}: bark UV area {bark_area:.4f}, column u < {bark_right:.4f} (fill {fill}), shell UV area "
          f"{shell_area:.4f}")
    _tree.bark_wind_uv(bark_low)
    _tree.replicate_sectors(shell)
    shell.data.materials.append(canopy_mat)
    lod.triangulate(shell)
    _tree.shell_smooth_normals(shell)

    # Fringe: the hero's cards on these clumps, scaled to fit the 256 px leaf tiles.
    leaves_low = _oak.leaf_cards("leaves_low", crown, seed=piece, per_clump=FRINGE_PER_CLUMP)
    scale_cards(leaves_low, CARD_SCALE)
    leaves_low.data.materials.append(leaf_mat)
    _tree.leaf_wind_uv(leaves_low, skel)
    # LOD1/LOD2 keep whole cards: subset the quads, then triangulate every LOD's cards.
    leaves1 = card_subset(leaves_low, 2, 1.25, "leaves_l1")
    leaves2 = card_subset(leaves_low, 4, 1.5, "leaves_l2")
    for o in (leaves_low, leaves1, leaves2):
        lod.triangulate(o)

    # Maps: canopy master -> shell (albedo through EMIT, normal, AO), bark AO -> masks.A.
    strip_px = ATLAS - int(round(_tree.LEAF_V0 * ATLAS))
    leaf_alb, leaf_nrm, leaf_rough = _leaves.strip(strip_px, HERO)
    assert leaf_alb.shape[1] == ATLAS, leaf_alb.shape
    albedo = np.empty((ATLAS, ATLAS, 4), np.float32)
    albedo[:] = (*leaf_alb[..., :3].reshape(-1, 3).mean(0), 1.0)
    normal = np.empty((ATLAS, ATLAS, 4), np.float32)
    normal[:] = (0.5, 0.5, 1.0, 1.0)
    orm = np.empty((ATLAS, ATLAS, 4), np.float32)
    orm[:] = (1.0, 0.62, 0.0, 1.0)
    bake_stats = {}
    if not skip_bake:
        canopy_high = _oak.canopy_high(f"{piece}_canopy_high", seed=HERO, per_sector=CANOPY_LEAVES_PER_SECTOR)
        canopy_high.data.materials.append(_tree.canopy_high_material(f"{piece}_canopy_high"))
        bark_high = _tree.tube_mesh(f"{piece}_bark_high", skel, {0: 20, 1: 12, 2: 8}, {0: 0.08, 1: 0.12, 2: 0.15})
        bark_high.data.materials.append(bark_mat)
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=SHELL_CAGE, max_ray=SHELL_RAY, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.shell_{k}", ATLAS, non_color=k != "albedo")
                for k in ("albedo", "normal", "ao")}
        bake.bake_emit_input([canopy_high], shell, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(shell, "NORMAL", imgs["normal"], highs=[canopy_high], **kw)
        scn.world.light_settings.distance = SHELL_AO_DIST
        bake.bake_to(shell, "AO", imgs["ao"], highs=[canopy_high], **kw)
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.shell_{k}.tmp.png"))
            px[k] = bake.load_png(p)[1].reshape(ATLAS, ATLAS, 4)
            os.remove(p)
        albedo[:] = px["albedo"]
        albedo[..., 3] = 1.0
        normal[:] = px["normal"]
        normal[..., 3] = 1.0
        orm[..., 0] = px["ao"][..., 0]
        scn.world.light_settings.distance = AO_DIST
        ao_img = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        bake.bake_to(bark_low, "AO", ao_img, highs=[bark_high], cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        ao_tmp = bake.save_png(ao_img, str(out / f"{piece}.ao.tmp.png"))
        ao = bake.load_png(ao_tmp)[1][:, 0].reshape(MASK_SIZE, MASK_SIZE).copy()
        os.remove(ao_tmp)
        bpy.data.objects.remove(canopy_high, do_unlink=True)
        bpy.data.objects.remove(bark_high, do_unlink=True)
        bpy.context.view_layer.update()
    else:
        ao = np.ones((MASK_SIZE, MASK_SIZE), np.float32)

    # masks (raw UV0): bark texels carry the bark AO; canopy texels A = 1 (AO in ORM.R) and snow R from
    # the canopy AO; the leaf strip A = 1.
    bark_isl = island_texels(bark_low, MASK_SIZE)
    canopy_isl = island_texels(shell, MASK_SIZE)
    canopy_isl_atlas = island_texels(shell, ATLAS)
    bark_zone = dilate(bark_isl, 6)
    canopy_zone = dilate(canopy_isl, 6)
    overlap = int((bark_isl & canopy_zone).sum())
    assert overlap == 0, f"bark islands within 6 masks texels of the canopy islands: {overlap}"
    f_ = ATLAS // MASK_SIZE
    canopy_ao_m = orm[..., 0].reshape(MASK_SIZE, f_, MASK_SIZE, f_).mean(axis=(1, 3))
    mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
    mk[...] = (1.0, WETNESS, NIGHT, 1.0)
    mk[..., 3] = np.where(bark_zone, ao, 1.0)
    mk[..., 0] = np.where(canopy_zone, CANOPY_SNOW * np.clip((canopy_ao_m - 0.3) / 0.5, 0.0, 1.0), 1.0)
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))
    if not skip_bake:
        bake_stats["barkAo"] = stats(ao[bark_isl])
        _, hidden = _oak.hidden_texels(bark_low, skel, bark_sides, MASK_SIZE)
        bake_stats["barkAoVisible"] = stats(ao[bark_isl & ~hidden])
        bake_stats["barkAoVisible"]["hiddenTexels"] = int((bark_isl & hidden).sum())
        bake_stats["canopyAo"] = stats(orm[..., 0][canopy_isl_atlas])
        bake_stats["canopyMasks"] = {"aMin": float(mk[..., 3][canopy_isl].min()),
                                     "snowMean": float(mk[..., 0][canopy_isl].mean())}

    # Canopy gaps (hero): texels showing the dark dome take a darker albedo and roughness 1.
    gap_zone = dilate(canopy_isl_atlas, MARGIN)
    gap_w = np.where(gap_zone, np.clip((GAP_RAMP[1] - np.abs(albedo[..., :3] - GAP_TONE).max(-1))
                                       / (GAP_RAMP[1] - GAP_RAMP[0]), 0.0, 1.0), 0.0).astype(np.float32)
    gap_lin = srgb_to_lin(albedo[..., :3]) - (gap_w * (1.0 - GAP_ALBEDO))[..., None] * srgb_to_lin(GAP_TONE)
    albedo[..., :3] = np.where(gap_w[..., None] > 0, lin_to_srgb(np.clip(gap_lin, 0.0, 1.0)), albedo[..., :3])
    orm[..., 1] = _verge_bush.matte(orm[..., 1])
    orm[..., 1] += gap_w * (GAP_ROUGH - orm[..., 1])
    if not skip_bake:
        gap_texels = canopy_isl_atlas & (gap_w > 0.5)
        bake_stats["canopyGaps"] = {"share": float(gap_texels.sum() / canopy_isl_atlas.sum()),
                                    "albedoScaleLinear": GAP_ALBEDO, "roughness": GAP_ROUGH}
    albedo[-strip_px:] = leaf_alb
    normal[-strip_px:, :, :3] = leaf_nrm
    orm[-strip_px:, :, 0] = 1.0
    orm[-strip_px:, :, 1] = _verge_bush.matte(leaf_rough)
    bake.write_png(ATLAS, albedo.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(ATLAS, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    fo = ATLAS // ORM_SIZE
    orm_small = orm.reshape(ORM_SIZE, fo, ORM_SIZE, fo, 4).mean(axis=(1, 3))  # linear: a plain box filter
    bake.write_png(ORM_SIZE, orm_small.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    cov, m = [], leaf_alb[..., 3]
    while min(m.shape) >= 1:
        cov.append(round(float((m > 0.5).mean()), 4))
        if min(m.shape) < 2:
            break
        h2, w2 = m.shape[0] // 2 * 2, m.shape[1] // 2 * 2
        m = m[:h2, :w2].reshape(h2 // 2, 2, w2 // 2, 2).mean(axis=(1, 3))
    bake_stats["leafAlphaCoverageByMip"] = cov

    # Canopy LOD0: one lumped copy of the shell per clump; closed copies feed the LOD1/LOD2 shells.
    shells = []
    for cl in crown:
        o = _oak.place_shell(shell, cl, f"shell_{cl['id']:02d}")
        _tree.shell_wind_uv(o, cl, skel)
        shells.append(o)
    shell_srcs = [copy_of(o, f"{o.name}_src") for o in shells]
    canopy_low = geom.join(shells, "canopy_low")
    bpy.data.objects.remove(shell, do_unlink=True)
    bpy.context.view_layer.update()  # removed objects linger in view_layer.objects as None (pitfalls)
    shell_faces = len(canopy_low.data.polygons)
    culled_shell = _oak.cull_shell_hidden(canopy_low, crown)
    print(f"{piece}: culled {culled_shell} hidden of {shell_faces} shell faces; bake stats {bake_stats}")

    texel = {bark_mat.name: uv.measure_texel_density(bark_low, TILE_UV_SCALE * BARK_SET_PX),
             canopy_mat_name: uv.measure_texel_density(canopy_low, ATLAS),
             leaf_mat.name: uv.measure_texel_density(leaves_low, ATLAS)}
    ao_density = uv.measure_texel_density(bark_low, MASK_SIZE)
    print(f"{piece}: texel {texel}, bark AO texels {ao_density}")

    bark1, bark2 = copy_of(bark_low, "bark_l1"), copy_of(bark_low, "bark_l2")

    def lod_canopy(per_clump, name):
        ps = []
        for src in shell_srcs:
            o = copy_of(src, f"{src.name}_{name}")
            lod.decimate_to(o, per_clump)
            ps.append(o)
        return geom.join(ps, name)

    lods = []
    parts0 = [geom.tri_count(o) for o in (bark_low, canopy_low, leaves_low)]
    t0 = sum(parts0)
    for i, (b, lv) in enumerate(((bark_low, leaves_low), (bark1, leaves1), (bark2, leaves2))):
        c = canopy_low
        if i:
            budget = int(t0 * (0.5 if i == 1 else 0.15)) - geom.tri_count(lv) - 4
            c = lod_canopy(int(budget * (0.55 if i == 1 else 0.6)) // len(crown), f"canopy_l{i}")
            lod.decimate_to(b, budget - geom.tri_count(c))
        o = geom.join([b, c, lv], f"{piece}_lod{i}")
        _tree.clamp_wind_uv(o)
        _oak.shell_normals(o, canopy_mat_name, crown)
        _tree.set_canopy_normals(o, leaf_mat.name, canopy_mat_name)
        _oak.junction_normals(o, skel, bark_mat.name)
        lods.append(o)
    for src in shell_srcs:
        bpy.data.objects.remove(src, do_unlink=True)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: LOD tris {tris}, LOD0 bark/canopy/fringe {parts0}")
    budgets.check(tris, "tree", piece)
    for name, t in texel.items():
        assert t["median"] >= 256, (name, t)

    # Collider: hull of the trunk's lowest 3 m (a stroll-blocker, D14), 8 points per level, staggered.
    trunk = skel[0]
    ring_pts = []
    for zi, z in enumerate((0.0, 1.0, 2.0, 3.0)):
        p = _at_z(trunk["points"], z)
        k_ = min(range(len(trunk["points"])), key=lambda j: abs(trunk["points"][j].z - z))
        r = trunk["radii"][k_]
        ring_pts += [(p.x + r * math.cos(2 * math.pi * (k + 0.5 * zi) / 8),
                      p.y + r * math.sin(2 * math.pi * (k + 0.5 * zi) / 8), z) for k in range(8)]
    cbm = bmesh.new()
    for q in ring_pts:
        cbm.verts.new(q)
    bmesh.ops.convex_hull(cbm, input=cbm.verts)
    cme = bpy.data.meshes.new("collider_src")
    cbm.to_mesh(cme)
    cbm.free()
    csrc = scene.link(bpy.data.objects.new("collider_src", cme))
    col = collider.collider_from(csrc, "hull", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)

    height = max(v.co.z for v in lods[0].data.vertices)
    footprint = {"ground": round(radial_max([col]), 3), "crown": round(radial_max(lods), 3),
                 "height": round(height, 3)}
    print(f"{piece}: footprint {footprint} (scatter FOOTPRINTS: ground = collider, crown = max over LODs)")

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    sig = bake.receiver_signature([lods[0]])
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator, budget_class="piece")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           texelDensityPxPerM=texel, footprint=footprint,
                           canopyShell={"cageExtrusionM": SHELL_CAGE, "maxRayDistanceM": SHELL_RAY,
                                        "aoDistanceM": SHELL_AO_DIST, "sectors": SECTORS, "clumps": len(crown),
                                        "columnsPerSector": list(_oak.SHELL_COLUMNS),
                                        "ringLatitudes": list(_oak.SHELL_PHIS),
                                        "leavesPerSector": CANOPY_LEAVES_PER_SECTOR, "uvArea": shell_area,
                                        "culledHiddenFaces": {"culled": culled_shell, "of": shell_faces}},
                           barkTile={"set": "bark", "tileUvScale": TILE_UV_SCALE, "uvPerM": BARK_UV_PER_M,
                                     "aoTexelDensityPxPerM": ao_density},
                           fringe={"perClump": FRINGE_PER_CLUMP, "cardScale": CARD_SCALE, "leafSeed": HERO},
                           tiers=[list(t) for t in tiers], shape=shape,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": ATLAS, "colorspace": "linear"},
                                     "orm": {"size": ORM_SIZE, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), bakeStats=bake_stats,
                           culledReceiverFaces={"buried": culled, "inCrown": culled_crown,
                                                "underground": culled_ground, "of": faces_before},
                           lodPartsTris=parts0, skipped=skip_bake)
