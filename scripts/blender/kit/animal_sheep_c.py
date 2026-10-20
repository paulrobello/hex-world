"""animal_sheep_c: animal_sheep grazing, its head down with the muzzle in the grass, a static farm prop
(PRD §6 Variety; the sheep counterpart of animal_cow_b; owner-approved references
assets-source/refs/farm_animals ref-2 sheep and ref-3 overhead herd, where six sheep graze; owner
decisions A17: black face and legs, creamy fleece, ref-2's smooth form). Scripted route; the pose and
rest-paint maps are in _animals_var.py, the builder in _animals.py (both reused unchanged).

The same sheep as animal_sheep: its body, legs and tail stations, palette and paint are copied here
unchanged (animal_sheep.py is frozen with the approved piece). The head and ears turn HEAD_DEG nose
down as one rigid part and move forward and down until the muzzle's lowest point is MUZZLE_CLEAR above
the ground; the neck is a new loft from animal_sheep's neck root, out of the chest and down into the
turned head. Every sample paints at its rest position on animal_sheep (the head and ears by the inverse
turn, the neck by `loft_map`, the rest as is), so the hood, the wool line on the legs, the eyes and the
nose are animal_sheep's own.

Real scale: 0.78 m at the withers (the fleece top); the muzzle stands MUZZLE_CLEAR off the grass ahead
of the front hooves. Faces runtime +z, +y up, +x the sheep's left; pivot on the ground under the body
centre. Kind `prop`, one slot `animal_sheep_c_body` (unique 1024 px albedo, 512 px normal and ORM,
128 px masks).

Motion contract (TEXCOORD_1, Blender UV map `motion`, FLOAT VEC2 on every primitive of every LOD;
`_animals.check_motion` re-reads it from the GLB), as animal_sheep's for this pose:
  U = leg weight: 0 off the legs; on a leg (HIP_Y - y) / HIP_Y with HIP_Y = 0.42 m.
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

PIECE = "animal_sheep_c"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
AO_DIST = 0.2
HIP_Y = 0.42
NECK_PIVOT = (0.20, 0.50)
# The pose: the head and ears turn HEAD_DEG about HEAD_JOINT (animal_sheep's neck end, inside the
# head), then move so HEAD_JOINT lands at f = HEAD_F with the muzzle's lowest point at MUZZLE_CLEAR.
HEAD_DEG, HEAD_JOINT, HEAD_F, MUZZLE_CLEAR = -22.0, (0.0, 0.55, 0.71), 0.70, 0.015
# The neck's way in: its tangent at the head (f, z), and the length of the curve's end handles (m).
NECK_IN, NECK_HANDLES = (0.62, -0.78), (0.16, 0.14)
NECK_STATIONS = 5
# animal_sheep's neck sizes along its arc (fraction, half width, half height top, bottom, e).
NECK_KEYS = [(0.0, 0.17, 0.18, 0.18, 2.2), (0.52, 0.16, 0.16, 0.17, 2.2), (0.84, 0.12, 0.12, 0.12, 2.2),
             (1.0, 0.07, 0.07, 0.07, 2.1)]
NECK_KEEP = 0.19  # rest paint: the neck root keeps animal_sheep's arc length this far (m), the exit

# animal_sheep's stations (animal_sheep.py SPEC, unchanged).
REST = {
    "body": an.Part([
        ((0, 0.49, 0.51), 0.0, 0.0, 0.0, 2.0),
        ((0, 0.46, 0.51), 0.16, 0.16, 0.17, 2.1),
        ((0, 0.39, 0.505), 0.26, 0.24, 0.26, 2.2),
        ((0, 0.23, 0.50), 0.31, 0.27, 0.29, 2.2),
        ((0, 0.02, 0.50), 0.33, 0.28, 0.30, 2.25),
        ((0, -0.19, 0.505), 0.33, 0.28, 0.30, 2.25),
        ((0, -0.35, 0.51), 0.28, 0.26, 0.27, 2.2),
        ((0, -0.45, 0.515), 0.18, 0.18, 0.18, 2.1),
        ((0, -0.49, 0.515), 0.0, 0.0, 0.0, 2.0),
    ]),
    "neck": an.Part([
        ((0, 0.22, 0.56), 0.17, 0.18, 0.18, 2.2),
        ((0, 0.40, 0.62), 0.16, 0.16, 0.17, 2.2),
        ((0, 0.50, 0.68), 0.12, 0.12, 0.12, 2.2),
        ((0, 0.55, 0.71), 0.07, 0.07, 0.07, 2.1),
    ], frame="path"),
    "head": an.Part([
        ((0, 0.47, 0.82), 0.0, 0.0, 0.0, 2.0),
        ((0, 0.49, 0.79), 0.085, 0.06, 0.09, 2.2),
        ((0, 0.54, 0.73), 0.10, 0.07, 0.10, 2.3),
        ((0, 0.59, 0.66), 0.075, 0.06, 0.075, 2.2),
        ((0, 0.64, 0.60), 0.06, 0.05, 0.06, 2.2),
        ((0, 0.68, 0.56), 0.055, 0.045, 0.045, 2.2),
        ((0, 0.70, 0.535), 0.035, 0.025, 0.03, 2.1),
        ((0, 0.705, 0.525), 0.0, 0.0, 0.0, 2.0),
    ], frame="path"),
    "front_leg": an.Part([
        ((0.13, 0.25, 0.38), 0.065, 0.075, 0.07, 2.2),
        ((0.13, 0.25, 0.26), 0.055, 0.065, 0.06, 2.2),
        ((0.13, 0.26, 0.15), 0.035, 0.038, 0.036, 2.2),
        ((0.13, 0.26, 0.045), 0.038, 0.045, 0.036, 2.3),
        ((0.13, 0.265, 0.0), 0.042, 0.05, 0.036, 2.4),
        ((0.13, 0.265, 0.0), 0.0, 0.0, 0.0, 2.0),
    ], N=(0.0, 1.0, 0.0)),
    "back_leg": an.Part([
        ((0.13, -0.27, 0.38), 0.07, 0.08, 0.075, 2.2),
        ((0.13, -0.28, 0.26), 0.06, 0.07, 0.065, 2.2),
        ((0.13, -0.30, 0.15), 0.035, 0.038, 0.038, 2.2),
        ((0.13, -0.29, 0.045), 0.038, 0.045, 0.036, 2.3),
        ((0.13, -0.285, 0.0), 0.042, 0.05, 0.036, 2.4),
        ((0.13, -0.285, 0.0), 0.0, 0.0, 0.0, 2.0),
    ], N=(0.0, 1.0, 0.0)),
    "ear": an.Part([
        ((0.06, 0.51, 0.765), 0.022, 0.012, 0.012, 2.0),
        ((0.11, 0.50, 0.77), 0.03, 0.012, 0.012, 2.0),
        ((0.155, 0.49, 0.765), 0.022, 0.010, 0.010, 2.0),
        ((0.175, 0.485, 0.76), 0.0, 0.0, 0.0, 2.0),
    ], X=(0.0, 0.0, 1.0), N=(0.0, 1.0, 0.0)),
    "tail": an.Part([
        ((0, -0.40, 0.60), 0.06, 0.06, 0.06, 2.0),
        ((0, -0.50, 0.60), 0.07, 0.065, 0.065, 2.0),
        ((0, -0.53, 0.55), 0.06, 0.055, 0.055, 2.0),
        ((0, -0.535, 0.50), 0.0, 0.0, 0.0, 2.0),
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

# Per LOD: part -> (station indices, ring sides). animal_sheep's, with the longer neck's stations and
# LOD2's head on four stations (three drew a beak, animal_cow friction note 6) paid for by three-sided
# legs beyond 400 m, as on animal_sheep_b, and the neck on its two end stations, as animal_sheep's.
LODS = {
    0: {"body": (range(9), 16), "neck": (range(NECK_STATIONS), 10), "head": (range(8), 10),
        "front_leg": (range(6), 7), "back_leg": (range(6), 7), "ear": (range(4), 5), "tail": (range(4), 6)},
    1: {"body": ([0, 1, 3, 4, 5, 7, 8], 10), "neck": ([0, 2, 4], 7), "head": ([0, 2, 4, 5, 7], 7),
        "front_leg": ([0, 2, 4, 5], 5), "back_leg": ([0, 2, 4, 5], 5), "ear": ([0, 1, 3], 4),
        "tail": ([0, 2, 3], 4)},
    2: {"body": ([0, 2, 4, 6, 8], 7), "neck": ([0, 4], 5), "head": ([0, 2, 5, 7], 5),
        "front_leg": ([0, 4, 5], 3), "back_leg": ([0, 4, 5], 3)},
}
GAIT = {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5}

C = {k: fa.hexc(v) for k, v in {
    "wool": "#e8dcc0", "wool_shade": "#cbbb98", "wool_light": "#f2ead6", "face": "#2a2624",
    "face_light": "#3a3431", "hoof": "#1e1b1a", "ear_in": "#5a463e", "eye": "#0a0808", "eye_shine": "#3c2c22",
    "nose": "#1a1615"}.items()}


def paint_rest(P, part):
    """animal_sheep.py `paint`, unchanged, at rest positions on animal_sheep."""
    n = len(P)
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    head = part == an.PART["head"]
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    ears = np.isin(part, [an.PART["ear_l"], an.PART["ear_r"]])
    lump = an.fbm(P, 0.07, 21)
    tone = an.fbm(P, 0.25, 4)
    wool = fa.mix(fa.mix(C["wool"], C["wool_light"], fa.smoothstep(0.55, 0.8, lump)), C["wool_shade"],
                  0.55 * fa.smoothstep(0.55, 0.15, tone))
    wool = fa.mix(wool, C["wool_shade"], 0.35 * fa.smoothstep(0.36, 0.22, z))
    height = 0.006 * lump + 0.002 * an.fbm(P, 0.025, 9)
    rough = np.full(n, 0.72, np.float32)
    wl = np.zeros(n)
    wl[~(head | legs | ears)] = 1.0
    hs, _, hx, hn = REST["head"].local(P[head])
    sf = hs / REST["head"].Lc
    warp = an.fbm(P[head], 0.05, 13) - 0.5
    wl[head] = fa.smoothstep(0.30, 0.22, sf + 0.06 * warp - 0.08 * np.clip(hn, 0, 1))
    lw = an.fbm(P[legs], 0.04, 17) - 0.5
    wl[legs] = fa.smoothstep(0.25, 0.29, z[legs] + 0.03 * lw)
    face = fa.mix(C["face"], C["face_light"], 0.5 * an.fbm(P, 0.06, 2))
    rgb = fa.mix(face, wool, wl)
    height = height * wl
    rough = (0.55 + 0.17 * wl).astype(np.float32)
    rgb[ears] = fa.mix(C["face"], C["ear_in"], 0.35)
    Ph = P[head]
    hrgb, hr, hh = rgb[head], rough[head], height[head]
    for s in (1, -1):
        eye = REST["head"].point(2, np.pi / 2 * s - 0.5 * s)
        d = np.linalg.norm(Ph - eye, axis=1)
        hrgb[d < 0.016] = C["eye_shine"]
        hrgb[d < 0.012] = C["eye"]
        hr[d < 0.016] = 0.15
        hh[d < 0.016] += 0.002 * np.clip(1 - (d[d < 0.016] / 0.016) ** 2, 0, 1)
        nos = REST["head"].point(6, 0.6 * s)
        d = np.linalg.norm((Ph - nos) / np.array([0.008, 0.01, 0.007]), axis=1)
        hrgb[d < 1] = C["nose"]
        hh[d < 1] -= 0.0012 * (1 - d[d < 1] ** 2)
    rgb[head], rough[head], height[head] = hrgb, hr, hh
    hoof = legs & (z < 0.045)
    rgb[hoof] = C["hoof"]
    rough[hoof] = 0.4
    up = np.clip((z - 0.58) / 0.2, 0, 1)
    snow = np.where(legs, 0.05, 0.1 + 0.6 * up).astype(np.float32)
    wet = np.where(wl > 0.5, 0.45, 0.25).astype(np.float32)
    return {"rgb": np.clip(rgb, 0, 1).astype(np.float32), "rough": rough, "metal": np.zeros(n, np.float32),
            "height": height.astype(np.float32), "snow": snow, "wet": wet}


REST_PAINT = av.RestPaint(paint_rest, {
    "head": HEAD.inv, "ear": HEAD.inv,
    "neck": lambda P: av.loft_map(SPEC["neck"], REST["neck"], P, keep=NECK_KEEP)})


def paint(P, part):
    """animal_sheep's paint at rest positions; snow capacity from the posed height (the lowered head
    takes animal_sheep's flank value, the top of the neck its back's)."""
    out = REST_PAINT(P, part)
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    up = np.clip((P[:, 2] - 0.58) / 0.2, 0, 1)
    out["snow"] = np.where(legs, 0.05, 0.1 + 0.6 * up).astype(np.float32)
    return out


NECK_BASE = av.neck_base(SPEC, NECK_PIVOT)
print(f"{PIECE}: head joint {np.round(_end, 3)}, muzzle tip {np.round(SPEC['head'].c[-1], 3)}, "
      f"poll {np.round(SPEC['head'].c[0], 3)}, neck base {np.round(NECK_BASE, 3)}, "
      f"neck length {SPEC['neck'].Lc:.3f} m (animal_sheep {REST['neck'].Lc:.3f} m)")
motion = an.Motion(HIP_Y, NECK_PIVOT, NECK_BASE, SPEC["head"].c[-1][1:], SPEC["ear"].c[0])
real = {"withersM": 0.78, "muzzleClearM": MUZZLE_CLEAR,
        "muzzleTip": [round(float(v), 3) for v in SPEC["head"].c[-1]]}
av.build(PIECE, SPEC, LODS, motion, paint, out, skip_bake, AO_DIST, real, GAIT, __file__,
         extra={"pose": f"grazing: animal_sheep with its head turned {HEAD_DEG:g} deg nose down to the grass"})
