"""Hero oak, ancient stag-headed variant: the builder behind tree_hero_oak_c.py (PRD §6 Variety, the oak
tile's hero slot). Refs: assets-source/refs/tree_hero_oak (ref-1, owner-approved).

The hero's massive twisted three-strand bole (no lean) under a low, broad crown broken into lobes of
one or two domes on long, low limbs, with a wide gap to the south-east. Its top has died back: two bare
dead leaders (stag's antlers) rise from the bole through the living crown and stand clear above it, with
short dead tines, and a broken dead limb stub juts out of the bole into the south-east gap. Against
tree_hero_oak's round tiered dome and tree_hero_oak_b's forked, leaning V.

The pipeline is `_oak_fork.build` (the hero's look: bark set, canopy master, leaf tiles, palette, canopy
gaps, wind data, AO, 1.2x scale), run unchanged. Its `skeleton` is wrapped through the module global:
the wrapper keeps `_oak_fork`'s strands, roots, limbs and secondaries and appends the bare dead limbs
(level 1 leaders and stubs, level 2 tines), which feed no clump, so they carry bark only."""
from __future__ import annotations

import math
import pathlib
import random

from mathutils import Vector

import _oak_fork
import _tree

_FORK_SKELETON = _oak_fork.skeleton


def _dead_branch(rng: random.Random, start: Vector, end: Vector, r: tuple, level: int, sway: tuple,
                 bow: float, wobble: float, n: int) -> dict:
    """One bare dead branch: a gnarled arc, tapering from r[0] to r[1] (a blunt broken end when r[1]
    stays thick)."""
    pts = _tree._arc(rng, start, end, bow, wobble, n)
    return {"level": level, "points": pts, "radii": _tree._lerp_radius(r[0], r[1], len(pts), 0.8), "sway": sway}


def dead_limbs(seed: str, dead: dict) -> list[dict]:
    """The stag head and the stub. `dead["leaders"]`: (start, end, r0, r1, tines) with tines as
    (t along the leader, direction (x, y, z), length, r0, r1); `dead["stubs"]`: (start, end, r0, r1).
    Blender frame, 1x. Leaders and stubs are level 1, tines level 2 starting at their parent's sway."""
    rng = random.Random(seed + ":dead")
    out: list[dict] = []
    for start, end, r0, r1, tines in dead["leaders"]:
        b = _dead_branch(rng, Vector(start), Vector(end), (r0, r1), 1, (_tree.SWAY_TRUNK, _tree.SWAY_LIMB),
                         0.25, 0.16, 18)
        out.append(b)
        for t, d, length, tr0, tr1 in tines:
            i = int(round(t * (len(b["points"]) - 1)))
            base = b["points"][i]
            s0 = _tree.SWAY_TRUNK + (_tree.SWAY_LIMB - _tree.SWAY_TRUNK) * t
            out.append(_dead_branch(rng, base, base + Vector(d).normalized() * length, (tr0, tr1), 2,
                                    (s0, _tree.SWAY_TWIG), 0.08, 0.05, 6))
    for start, end, r0, r1 in dead["stubs"]:
        out.append(_dead_branch(rng, Vector(start), Vector(end), (r0, r1), 1, (_tree.SWAY_TRUNK, 0.3),
                                0.15, 0.08, 10))
    return out


def skeleton(crown: list[dict], seed: str, s: dict):
    """`_oak_fork.skeleton` plus the bare dead limbs of `s["dead"]`; ids renumbered."""
    skel, lean = _FORK_SKELETON(crown, seed, s)
    assert all(b["level"] == 0 for b in skel[:3]) and len(skel[0]["points"]) == 41, "skel[:3] are the strands"
    skel = skel + dead_limbs(seed, s["dead"])
    assert all(b["level"] <= 2 for b in skel), "level 3 branches never reach the low-poly receiver"
    for i, b in enumerate(skel):
        b["id"] = i
    return skel, lean


def build(piece: str, out: pathlib.Path, generator: str, *, clumps: list[tuple], shape: dict) -> None:
    """`_oak_fork.build` with the dead-limb skeleton."""
    assert shape["lean"] == (0.0, 0.0), "the bole stands upright: the collider is the hero's"
    assert {c[4] for c in clumps} <= {"limb", "heavy", "sec"}, "no fork leaders: the top is dead"
    _oak_fork.skeleton = skeleton
    try:
        _oak_fork.build(piece, out, generator, clumps=clumps, shape=shape)
    finally:
        _oak_fork.skeleton = _FORK_SKELETON
