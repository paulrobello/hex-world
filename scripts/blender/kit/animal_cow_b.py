"""animal_cow_b: animal_cow grazing, its head down to the grass, a static farm prop (PRD §6 Variety;
owner-approved references assets-source/refs/farm_animals ref-1 cow and ref-3 overhead herd, where two
cows graze; owner decisions A17: hornless, brown and white, ref-2's smooth form). Scripted route; the
pose and rest-paint maps are in _animals_var.py, the builder in _animals.py (both reused unchanged).

The same cow as animal_cow: its body, legs and tail stations, palette, patches and paint are copied
here unchanged (animal_cow.py is frozen with the approved piece). The head and ears turn HEAD_DEG
nose down as one rigid part and move forward and down until the muzzle's lowest point is
MUZZLE_CLEAR above the ground; the neck is a new loft from animal_cow's neck root, over the shoulder
and down into the turned head. Every sample paints at its rest position on animal_cow (the head and
ears by the inverse turn, the neck by `loft_map`, the rest as is), so the patches, blaze, eyes and
muzzle are animal_cow's own.

Real scale: 1.45 m at the withers; the muzzle stands MUZZLE_CLEAR off the grass about 0.6 m ahead of
the front hooves. Faces runtime +z, +y up, +x the cow's left; pivot on the ground under the body
centre. Kind `prop`, one slot `animal_cow_b_body` (unique 1024 px albedo, 512 px normal and ORM, 128 px
masks).

Motion contract (TEXCOORD_1, Blender UV map `motion`, FLOAT VEC2 on every primitive of every LOD;
`_animals.check_motion` re-reads it from the GLB), as animal_cow's for this pose:
  U = leg weight: 0 off the legs; on a leg (HIP_Y - y) / HIP_Y with HIP_Y = 1.0 m.
  V = where U > 0, the leg code (0.125 FL, 0.375 FR, 0.625 BL, 0.875 BR; left = runtime +x).
      Where U = 0, the head weight: 0 on the body and tail, a position field along the axis from
      NECK_PIVOT to the posed muzzle tip, clamp((t - t_base) / (t_tip - t_base), 0, 1), with t_base
      the largest t of any neck surface point inside the body, so the buried neck root and the
      neck's exit from the chest stay at 0. The ears take their root's value. A head raise pitches
      V > 0 about the axis parallel to x through NECK_PIVOT, nose up, by theta x V (extras
      `grazeDegToGround` is 0: the muzzle is already down).
The same numbers ride in the root node's extras (`animal`, a JSON string)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _animals as an  # noqa: E402
import _animals_var as av  # noqa: E402
import _fauna as fa  # noqa: E402

PIECE = "animal_cow_b"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
AO_DIST = 0.35
HIP_Y = 1.0
NECK_PIVOT = (0.35, 0.98)
# The pose: the head and ears turn HEAD_DEG about HEAD_JOINT (animal_cow's neck end, inside the
# head), then move so HEAD_JOINT lands at f = HEAD_F with the muzzle's lowest point at MUZZLE_CLEAR.
HEAD_DEG, HEAD_JOINT, HEAD_F, MUZZLE_CLEAR = -30.0, (0.0, 1.10, 1.30), 1.12, 0.015
# The neck's way in: its tangent at the head (f, z), and the length of the curve's end handles (m).
NECK_IN, NECK_HANDLES = (0.62, -0.78), (0.32, 0.30)
NECK_STATIONS = 7
# animal_cow's neck sizes along its arc (fraction, half width, half height top, bottom, e).
NECK_KEYS = [(0.0, 0.22, 0.24, 0.26, 2.4), (0.45, 0.20, 0.23, 0.29, 2.4), (0.80, 0.16, 0.19, 0.22, 2.3),
             (1.0, 0.11, 0.13, 0.13, 2.2)]
NECK_KEEP = 0.40  # rest paint: the neck root keeps animal_cow's arc length this far (m), the exit

# animal_cow's stations (animal_cow.py SPEC, unchanged).
REST = {
    "body": an.Part([
        ((0, 0.90, 1.05), 0.0, 0.0, 0.0, 2.0),
        ((0, 0.86, 1.05), 0.15, 0.20, 0.22, 2.2),
        ((0, 0.74, 1.06), 0.26, 0.34, 0.37, 2.4),
        ((0, 0.50, 1.05), 0.31, 0.40, 0.40, 2.6),
        ((0, 0.18, 1.04), 0.34, 0.38, 0.44, 2.7),
        ((0, -0.22, 1.04), 0.34, 0.39, 0.41, 2.7),
        ((0, -0.58, 1.06), 0.31, 0.41, 0.31, 2.8),
        ((0, -0.80, 1.08), 0.25, 0.36, 0.27, 2.6),
        ((0, -0.90, 1.10), 0.14, 0.24, 0.18, 2.3),
        ((0, -0.925, 1.10), 0.0, 0.0, 0.0, 2.0),
    ]),
    "neck": an.Part([
        ((0, 0.45, 1.13), 0.22, 0.24, 0.26, 2.4),
        ((0, 0.80, 1.18), 0.20, 0.23, 0.30, 2.4),
        ((0, 1.00, 1.25), 0.16, 0.19, 0.22, 2.3),
        ((0, 1.10, 1.30), 0.11, 0.13, 0.13, 2.2),
    ], frame="path"),
    "head": an.Part([
        ((0, 1.02, 1.47), 0.0, 0.0, 0.0, 2.0),
        ((0, 1.06, 1.42), 0.13, 0.09, 0.15, 2.3),
        ((0, 1.14, 1.32), 0.165, 0.12, 0.18, 2.4),
        ((0, 1.23, 1.20), 0.13, 0.10, 0.13, 2.3),
        ((0, 1.31, 1.10), 0.105, 0.085, 0.105, 2.3),
        ((0, 1.38, 1.02), 0.12, 0.09, 0.10, 2.4),
        ((0, 1.415, 0.97), 0.095, 0.055, 0.065, 2.2),
        ((0, 1.425, 0.95), 0.0, 0.0, 0.0, 2.0),
    ], frame="path"),
    "front_leg": an.Part([
        ((0.18, 0.55, 0.99), 0.10, 0.15, 0.13, 2.2),
        ((0.19, 0.55, 0.70), 0.11, 0.155, 0.13, 2.2),
        ((0.19, 0.56, 0.50), 0.09, 0.11, 0.095, 2.2),
        ((0.19, 0.57, 0.36), 0.075, 0.075, 0.075, 2.2),
        ((0.19, 0.57, 0.17), 0.065, 0.065, 0.065, 2.2),
        ((0.19, 0.58, 0.065), 0.072, 0.085, 0.066, 2.3),
        ((0.19, 0.59, 0.0), 0.078, 0.095, 0.066, 2.4),
        ((0.19, 0.59, 0.0), 0.0, 0.0, 0.0, 2.0),
    ], N=(0.0, 1.0, 0.0)),
    "back_leg": an.Part([
        ((0.17, -0.58, 0.99), 0.10, 0.15, 0.15, 2.2),
        ((0.18, -0.60, 0.74), 0.11, 0.17, 0.14, 2.2),
        ((0.19, -0.66, 0.52), 0.085, 0.10, 0.09, 2.2),
        ((0.19, -0.70, 0.42), 0.07, 0.07, 0.085, 2.2),
        ((0.19, -0.68, 0.20), 0.06, 0.06, 0.06, 2.2),
        ((0.19, -0.66, 0.065), 0.07, 0.08, 0.066, 2.3),
        ((0.19, -0.65, 0.0), 0.076, 0.09, 0.066, 2.4),
        ((0.19, -0.65, 0.0), 0.0, 0.0, 0.0, 2.0),
    ], N=(0.0, 1.0, 0.0)),
    "ear": an.Part([
        ((0.07, 1.095, 1.385), 0.035, 0.018, 0.018, 2.0),
        ((0.17, 1.075, 1.40), 0.06, 0.02, 0.02, 2.0),
        ((0.25, 1.055, 1.40), 0.045, 0.016, 0.016, 2.0),
        ((0.29, 1.045, 1.395), 0.0, 0.0, 0.0, 2.0),
    ], X=(0.0, 0.0, 1.0), N=(0.0, 1.0, 0.0)),
    "tail": an.Part([
        ((0, -0.72, 1.24), 0.05, 0.05, 0.05, 2.0),
        ((0, -0.91, 1.31), 0.04, 0.04, 0.04, 2.0),
        ((0, -0.98, 1.22), 0.028, 0.028, 0.028, 2.0),
        ((0, -1.00, 0.95), 0.021, 0.021, 0.021, 2.0),
        ((0, -1.00, 0.74), 0.024, 0.024, 0.024, 2.0),
        ((0, -1.00, 0.64), 0.05, 0.045, 0.045, 2.0),
        ((0, -1.00, 0.54), 0.035, 0.03, 0.03, 2.0),
        ((0, -1.00, 0.49), 0.0, 0.0, 0.0, 2.0),
    ], frame="path"),
}


def head_map() -> av.Map:
    """The rigid head turn, with the muzzle's lowest surface point at MUZZLE_CLEAR."""
    m = av.rigid(HEAD_DEG, HEAD_JOINT, (0.0, HEAD_F, 0.0))
    h = av.moved(REST["head"], m, "path")
    low = min(float(h.point(i, k * np.pi / 32)[2]) for i in range(len(h.c)) for k in range(64))
    return av.rigid(HEAD_DEG, HEAD_JOINT, (0.0, HEAD_F, MUZZLE_CLEAR - low))


HEAD = head_map()
_end = HEAD(HEAD_JOINT)
_in = np.array([0.0, *NECK_IN]) / np.linalg.norm(NECK_IN)
_root = REST["neck"].c[0]
_out = (REST["neck"].c[1] - _root) / np.linalg.norm(REST["neck"].c[1] - _root)
NECK_CTRL = [_root, _root + _out * NECK_HANDLES[0], _end - _in * NECK_HANDLES[1], _end]
SPEC = dict(REST)
SPEC["head"] = av.moved(REST["head"], HEAD, "path")
SPEC["ear"] = av.moved(REST["ear"], HEAD, "fixed")
SPEC["neck"] = av.curve_part(NECK_CTRL, NECK_KEYS, NECK_STATIONS)
assert len(SPEC["head"].c) == 8 and len(SPEC["neck"].c) == NECK_STATIONS

# Per LOD: part -> (station indices, ring sides). animal_cow's, with the longer neck's stations and
# LOD2's head on four stations (animal_cow's three drew a beak; its friction note 6).
LODS = {
    0: {"body": (range(10), 18), "neck": (range(NECK_STATIONS), 12), "head": (range(8), 12),
        "front_leg": (range(8), 8), "back_leg": (range(8), 8), "ear": (range(4), 6), "tail": (range(8), 6)},
    1: {"body": ([0, 2, 3, 4, 6, 8, 9], 12), "neck": ([0, 2, 4, 6], 8), "head": ([0, 2, 4, 5, 7], 8),
        "front_leg": ([0, 1, 3, 5, 6, 7], 6), "back_leg": ([0, 1, 3, 5, 6, 7], 6), "ear": ([0, 1, 3], 4),
        "tail": ([0, 2, 3, 5, 7], 4)},
    2: {"body": ([0, 2, 5, 7, 9], 8), "neck": ([0, 3, 6], 5), "head": ([0, 2, 5, 7], 5),
        "front_leg": ([0, 2, 6, 7], 4), "back_leg": ([0, 2, 6, 7], 4)},
}
GAIT = {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5}

C = {k: fa.hexc(v) for k, v in {
    "white": "#ebe6dc", "white_shade": "#d9d2c6", "brown": "#7c4424", "brown_dark": "#5c3019",
    "muzzle": "#c2b2aa", "nostril": "#4d3c38", "hoof": "#3d3733", "eye": "#16100c", "eye_ring": "#3a2418",
    "switch": "#4f2a16"}.items()}

# Brown patches: (centre (x, f, z), radii); x < 0 centres sit on the right side only.
PATCHES = [
    ((0.0, 0.80, 1.28), (0.40, 0.42, 0.34)),   # neck and shoulder
    ((0.0, 1.10, 1.38), (0.26, 0.20, 0.20)),   # crown and ears
    ((0.10, 0.02, 1.30), (0.44, 0.40, 0.32)),  # mid back, lower on the left
    ((-0.10, -0.10, 1.32), (0.40, 0.34, 0.28)),
    ((0.0, -0.80, 1.26), (0.42, 0.22, 0.30)),  # rump
    ((0.36, -0.36, 1.02), (0.10, 0.11, 0.10)),  # small left flank spot
    ((-0.36, 0.30, 0.98), (0.10, 0.14, 0.09)),  # right shoulder spot
]


def paint_rest(P, part):
    """animal_cow.py `paint`, unchanged, at rest positions on animal_cow."""
    n = len(P)
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    warp = an.fbm(P, 0.22, 11) - 0.5
    field = np.full(n, -1.0)
    for c, r in PATCHES:
        field = np.maximum(field, an.ellipsoid_field(P, c, r))
    wb = fa.smoothstep(-0.012, 0.012, field + 0.30 * warp)
    head = part == an.PART["head"]
    hs, _, hx, hn = REST["head"].local(P[head])
    sf = hs / REST["head"].Lc
    wh = warp[head]
    blaze = fa.smoothstep(0.46, 0.40, np.abs(hx) - 0.12 * wh) * fa.smoothstep(-0.42, -0.30, hn) \
        * fa.smoothstep(0.24, 0.30, sf)
    jaw = fa.smoothstep(-0.50, -0.60, hn + 0.1 * wh) * fa.smoothstep(0.50, 0.58, sf)
    crown = fa.smoothstep(0.30, 0.24, sf) * fa.smoothstep(-0.28, -0.16, hn)
    hb = np.maximum(wb[head] * (1 - np.maximum(blaze, jaw)), crown)
    wb[head] = hb
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    wb[legs] = 0.0
    wb[np.isin(part, [an.PART["ear_l"], an.PART["ear_r"]])] = 1.0
    tone = an.fbm(P, 0.5, 5)
    rgb = fa.mix(fa.mix(C["white"], C["white_shade"], 0.6 * tone),
                 fa.mix(C["brown"], C["brown_dark"], 0.55 * tone), wb)
    rough = (0.62 - 0.04 * wb).astype(np.float32)
    height = 0.0012 * (an.fbm(P, 0.12, 3) - 0.5)
    hrgb, hr, hh = rgb[head], rough[head], height[head]
    muz = sf > 0.80 + 0.03 * warp[head]
    hrgb[muz] = C["muzzle"]
    hr[muz] = 0.48
    Ph = P[head]
    for s in (1, -1):
        nos = REST["head"].point(6, 0.55 * s)
        d = np.linalg.norm((Ph - nos) / np.array([0.016, 0.02, 0.014]), axis=1)
        hrgb[d < 1] = C["nostril"]
        hh[d < 1] -= 0.002 * (1 - d[d < 1] ** 2)
        eye = REST["head"].point(2, np.pi / 2 * s - 0.45 * s)
        d = np.linalg.norm(Ph - eye, axis=1)
        hrgb[d < 0.03] = C["eye_ring"]
        hrgb[d < 0.022] = C["eye"]
        hr[d < 0.022] = 0.15
        hh[d < 0.03] += 0.004 * np.clip(1 - (d[d < 0.03] / 0.03) ** 2, 0, 1)
    rgb[head], rough[head], height[head] = hrgb, hr, hh
    hoof = legs & (z < 0.072)
    rgb[hoof] = C["hoof"]
    rough[hoof] = 0.45
    tail = part == an.PART["tail"]
    sw = tail & (z < 0.76)
    rgb[sw] = C["switch"]
    height[sw] += 0.002 * (an.fbm(P[sw] * np.array([1, 1, 0.2]), 0.02, 7) - 0.5)
    up = np.clip((z - 1.15) / 0.3, 0, 1)
    snow = np.where(legs, 0.05, 0.08 + 0.6 * up).astype(np.float32)
    wet = np.full(n, 0.25, np.float32)
    return {"rgb": np.clip(rgb, 0, 1).astype(np.float32), "rough": rough, "metal": np.zeros(n, np.float32),
            "height": height.astype(np.float32), "snow": snow, "wet": wet}


REST_PAINT = av.RestPaint(paint_rest, {
    "head": HEAD.inv, "ear": HEAD.inv,
    "neck": lambda P: av.loft_map(SPEC["neck"], REST["neck"], P, keep=NECK_KEEP)})


def paint(P, part):
    """animal_cow's paint at rest positions; snow capacity from the posed height (the lowered head
    takes animal_cow's flank value, the back of the neck its back's)."""
    out = REST_PAINT(P, part)
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    up = np.clip((P[:, 2] - 1.15) / 0.3, 0, 1)
    out["snow"] = np.where(legs, 0.05, 0.08 + 0.6 * up).astype(np.float32)
    return out


NECK_BASE = av.neck_base(SPEC, NECK_PIVOT)
print(f"{PIECE}: head joint {np.round(_end, 3)}, muzzle tip {np.round(SPEC['head'].c[-1], 3)}, "
      f"neck base {np.round(NECK_BASE, 3)}, neck length {SPEC['neck'].Lc:.3f} m")
motion = an.Motion(HIP_Y, NECK_PIVOT, NECK_BASE, SPEC["head"].c[-1][1:], SPEC["ear"].c[0])
real = {"withersM": 1.45, "muzzleClearM": MUZZLE_CLEAR,
        "muzzleTip": [round(float(v), 3) for v in SPEC["head"].c[-1]]}
av.build(PIECE, SPEC, LODS, motion, paint, out, skip_bake, AO_DIST, real, GAIT, __file__,
         extra={"pose": f"grazing: animal_cow with its head turned {HEAD_DEG:g} deg nose down to the grass"})
