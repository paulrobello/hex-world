"""outcrop_granite_b: a second whaleback granite outcrop (ref-2), about 8 x 4.8 m and 1.3 m high, so
that two outcrops in one view no longer show the same fracture pattern (outcrop_granite redo, open
question Q2: "every outcrop is the same mesh, rotated and scaled"). Built ahead of owner review in
outcrop_granite's material family, scale class, pipeline (`_boulder.build`) and budgets (refs:
assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a runtime
scatter prop drawn as outcrop_granite's variant: make kit-build PIECE=outcrop_granite_b.

What differs from outcrop_granite:
- Silhouette and mass. Its own seed (the piece id) gives other cleave planes, and `warp` bends the
  plan into a kidney, tapers a low -x tail and lifts a blunt +x end with a saddle behind it, so the
  high point sits off centre instead of on a centred crest.
- Fractures. A curved transverse joint cuts a raised slab off the high end (ref-2's stepped front
  slab), a long joint runs from it down the tail, a short branch leaves that toward the -y rim, and a
  hairline lies near the +y rim, all from their own `SEED`.

The fracture carving, refinement, top swell and varied lichen are outcrop_granite's, shared through
`_outcrop.install` (`_outcrop.py`), which runs `warp` first; `_boulder.py`, `_rock.py` and the other
pieces are unchanged."""
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import bmesh
from mathutils import Vector

import _boulder
import _outcrop
from _boulder import Stone
from _outcrop import Fracture

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
SEED = "outcrop_granite_b-fractures"
SEMI_X = 3.95  # the stone's long semi-axis (m)
# `warp`, with xn = x / SEMI_X: y -> y (1 + TAPER xn) + BEND (xn^2 - 0.35) (a kidney plan, wide at +x),
# and height above ground times 1 + RISE xn - DIP exp(-((xn + 0.15) / 0.35)^2) (a high +x end, a saddle).
BEND, TAPER, RISE, DIP = 0.55, 0.22, 0.28, 0.08

FRACTURES = [
    # The main joint: a curved cut across the high +x end, ending inside both rims. Its +x side (left
    # of a->b) is ref-2's raised front slab: the step holds level across the slab (`hold`, `fade`), and
    # its edge rolls down into the joint over 3.2 half-widths (`bevel`), so it reads as a drop off a
    # fractured slab, not a ridge beside the groove (pass 2, coordinator 2026-11-13).
    Fracture(a=(1.45, 2.10), b=(2.10, -1.95), depth=0.13, width=0.12, step=0.09, fade=4.0,
             wobble=0.22, ends=(0.16, 0.12), bevel=3.2, hold=1.8),
    # A long joint from the main one down the low tail, ending inside the -x rim.
    Fracture(a=(0.0, 0.0), b=(-3.05, 0.55), depth=0.10, width=0.10, step=0.045, fade=1.8,
             wobble=0.17, ends=(0.0, 0.26), parent=0, at=0.46),
    # A short branch off the long joint toward the -y rim.
    Fracture(a=(0.0, 0.0), b=(-0.95, -1.70), depth=0.065, width=0.07, step=0.03, fade=1.0,
             wobble=0.20, ends=(0.0, 0.38), parent=1, at=0.40),
    # A hairline near the +y rim, dying out at both ends.
    Fracture(a=(-1.70, 1.25), b=(0.15, 1.75), depth=0.035, width=0.04, step=0.0, fade=1.0,
             wobble=0.22, ends=(0.30, 0.30)),
]


def warp(bm: bmesh.types.BMesh) -> None:
    """Bends, tapers and lifts the stone (`BEND`, `TAPER`, `RISE`, `DIP`); heights scale only above
    ground, so the z = 0 plane stays where it is."""
    for v in bm.verts:
        x, y, z = v.co
        xn = max(-1.2, min(1.2, x / SEMI_X))
        y = y * (1.0 + TAPER * xn) + BEND * (xn * xn - 0.35)
        if z > 0.0:
            z *= 1.0 + RISE * xn - DIP * math.exp(-((xn + 0.15) / 0.35) ** 2)
        v.co = Vector((x, y, z))


PATHS = _outcrop.install("outcrop_granite_b", FRACTURES, SEED, pre=warp)

_boulder.build("outcrop_granite_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(SEMI_X, 2.75, 1.36), centre=(0.0, 0.0, -0.15), planes=13, keep=(0.85, 0.95),
                  top_el=(1.25, 1.5), tilt=(0.07, -0.05))],
    bury=-0.32, lod_tris=(1398, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.10, joints=tuple(FRACTURES))
