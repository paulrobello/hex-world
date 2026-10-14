"""bird_starling: a starling-sized songbird boid in flight (owner-approved references
assets-source/refs/birds ref-1, gliding in profile, and ref-3, roosting on an oak branch; ref-2 is flock
layout only). Scripted route; the shared builder, frame and map drawing are in _fauna.py.

Real scale: 0.197 m from beak tip to tail tip, 0.40 m wingspan. Faces runtime +z, +y up, pivot at the
body centre on the shoulder hinge line (the boid's simulated position). Glide pose: wings spread with
6 degrees of dihedral. Slate-blue head, back and coverts, rufous breast, cream belly, charcoal flight
and tail feathers; the feather tips and separations live in the albedo and normal map, not in
geometry (owner, Q6). LOD0 is 194 tris (owner budget 100-200).

Motion contract for the boid vertex shader, TEXCOORD_1 (FLOAT VEC2, every LOD):
  U = wing weight: 0 on the body and tail, (|x| - HINGE_X) / (TIP_X - HINGE_X) on the wings (0 at the
      shoulder hinge, 1 at the wing tip). Flap: rotate every U > 0 vertex about the hinge line
      (runtime x = +-HINGE_X, y = HINGE_Z, parallel to z) by +-theta. Perch: scale its offset from the
      hinge line in the x-y plane by FOLD_K, flap it by FOLD_ROLL (down, so the wing lies along the
      upper flank with its coverts forward and flight feathers back, as in ref-3) and move it back by
      FOLD_BACK * U along -z.
  V = tail weight: 0 at the tail base, 1 at the tail tip (for a tail pitch about the tail base).
The same numbers ride in the root node's extras (`boid`, a JSON string)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _fauna as fa  # noqa: E402

PIECE, ATLAS, MASK_SIZE, S = "bird_starling", 512, 128, 3.6
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

# Body stations (f, half width, half height, centre z), beak tip to tail base.
BODY = [
    (0.085, 0.0, 0.0, 0.012),
    (0.068, 0.0055, 0.0055, 0.013),
    (0.060, 0.013, 0.013, 0.016),
    (0.047, 0.018, 0.018, 0.017),
    (0.032, 0.018, 0.020, 0.012),
    (0.014, 0.025, 0.027, 0.002),
    (-0.008, 0.025, 0.026, -0.002),
    (-0.032, 0.019, 0.019, 0.001),
    (-0.052, 0.009, 0.008, 0.006),
]
LOD_BODY = {0: (BODY, 10),
            1: ([BODY[i] for i in (0, 1, 3, 5, 7, 8)], 6),
            2: ([BODY[0], BODY[3], BODY[5], (-0.052, 0.0, 0.0, 0.006)], 4)}
HINGE_X, HINGE_Z, TIP_X, DIHEDRAL = 0.020, 0.014, 0.200, np.radians(6.0)
WING0 = [(0.016, 0.020), (0.075, 0.026), (0.140, 0.008), (0.200, -0.036), (0.150, -0.060),
         (0.085, -0.052), (0.016, -0.030)]
WING = {0: WING0, 1: [WING0[i] for i in (0, 1, 3, 5, 6)], 2: [WING0[i] for i in (0, 3, 6)]}
TAIL_BASE, TAIL_TIP = -0.046, -0.112
TAIL0 = [(0.009, TAIL_BASE), (0.026, TAIL_TIP), (0.0, -0.106), (-0.026, TAIL_TIP), (-0.009, TAIL_BASE)]
TAIL = {0: TAIL0, 1: [TAIL0[i] for i in (0, 1, 3, 4)], 2: [(0.0, TAIL_BASE), (0.024, TAIL_TIP), (-0.024, TAIL_TIP)]}
FOLD_K, FOLD_ROLL, FOLD_BACK = 0.25, -60.0, 0.015
PART = {"body": 0, "wing_top": 1, "wing_under": 2, "tail_top": 3, "tail_under": 4, "cap": 5}
EPS = 0.0012


def body_uv(f, arc):
    return (0.02 + (0.085 - f) * S, 0.02 + arc * S)


def cap_uv(p):
    return (0.86 + (p[0] + 0.01) * S, 0.05 + (p[2] + 0.004) * S)


def wing_z(a):
    return HINGE_Z + max(a - HINGE_X, 0.0) * np.tan(DIHEDRAL)


def tail_z(f):
    return 0.007 + (f - TAIL_BASE) * 0.08


def mo_body(p):
    return (0.0, 0.0)


def mo_wing(p):
    return (float(np.clip((abs(p[0]) - HINGE_X) / (TIP_X - HINGE_X), 0.0, 1.0)), 0.0)


def mo_tail(p):
    return (0.0, float(np.clip((TAIL_BASE - p[1]) / (TAIL_BASE - TAIL_TIP), 0.0, 1.0)))


def build(level, mat):
    b = fa.Builder()
    stations, n = LOD_BODY[level]
    fa.loft(b, stations, n, PART["body"], body_uv, mo_body, cap_uv)
    for s in (1, -1):
        nrm = (-s * np.sin(DIHEDRAL), 0.0, np.cos(DIHEDRAL))
        fa.plate(b, WING[level], lambda a, f, s=s: (s * a, f, wing_z(a)),
                 {1: PART["wing_top"], -1: PART["wing_under"]},
                 lambda a, f, side: (0.02 + (a - 0.016) * S, (0.665 if side > 0 else 0.335) + (f + 0.062) * S),
                 mo_wing, nrm, EPS)
    fa.plate(b, TAIL[level], lambda x, f: (x, f, tail_z(f)), {1: PART["tail_top"], -1: PART["tail_under"]},
             lambda x, f, side: (0.56 + (x + 0.028) * S, 0.02 + (-0.044 - f) * S), mo_tail,
             (0.0, 0.08, 1.0), EPS)
    return b.finish(f"{PIECE}_lod{level}", mat)


C = {k: fa.hexc(v) for k, v in {
    "back": "#6c84a2", "crown": "#7590ae", "coverts": "#86a0bd", "flight": "#2d2e33", "edge": "#4e5258",
    "secondary": "#5f7590",
    "under": "#b7bbc1", "under_dark": "#6c6f75", "breast": "#b95d2e", "throat": "#c86f3c",
    "belly": "#e9e0c6", "tail": "#34373c", "beak": "#38353a", "eye": "#0c0c0e", "ring": "#c8c2b4"}.items()}
ST = np.array(sorted(BODY[1:], key=lambda s: s[0]))


def le_te(a):
    """Leading and trailing edge f of the LOD0 wing at span a."""
    le = np.interp(a, [p[0] for p in WING0[:4]], [p[1] for p in WING0[:4]])
    te = np.interp(a, [p[0] for p in WING0[3:][::-1]], [p[1] for p in WING0[3:][::-1]])
    return le, te


def feathers(coord, spacing, lo=0.0):
    """Feather index and position across it (0..1) for a coordinate measured in metres."""
    k = np.floor((coord - lo) / spacing)
    return k, (coord - lo) / spacing - k


def paint(P, part, variant):
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    n = len(P)
    rgb = np.zeros((n, 3), np.float32)
    rough = np.full(n, 0.78, np.float32)
    height = np.zeros(n, np.float32)
    # Body.
    zc = np.interp(f, ST[:, 0], ST[:, 3])
    hh = np.maximum(np.interp(f, ST[:, 0], ST[:, 2]), 1e-4)
    v = np.clip((z - zc) / hh, -1.5, 1.5)
    blue = fa.mix(C["back"], C["crown"], fa.smoothstep(0.02, 0.06, f))
    warm = fa.mix(C["breast"], C["throat"], fa.smoothstep(0.03, 0.06, f))
    warm = fa.mix(C["belly"], warm, fa.smoothstep(-0.026, 0.0, f))
    # Blue reaches lower on the head (to the gape) than on the body (upper flank).
    edge = np.where(f > 0.04, -0.30, 0.05) + (f - 0.04) * 0.0
    col = fa.mix(warm, blue, fa.smoothstep(edge - 0.12, edge + 0.08, v))
    col = fa.mix(col, C["belly"], fa.smoothstep(-0.03, -0.05, f) * fa.smoothstep(0.1, -0.3, v))
    # Feather texture: offset rows of soft scallops, lighter tips.
    k, t = feathers(f * 1.0 + 0.0 * x, 0.006)
    scal = np.cos(np.pi * (t * 2 - 1)) * 0.5 + 0.5
    col = col * (0.95 + 0.07 * scal)[:, None]
    hb = 0.00012 * scal
    beak = f > 0.0655
    col[beak] = fa.mix(C["beak"], C["beak"] * 1.35, fa.smoothstep(0.075, 0.068, f[beak]))
    ze = 0.0215
    d = np.hypot(f - 0.052, z - ze)
    side = np.abs(x) > 0.008
    eye = side & (d < 0.0047)
    col[eye] = C["ring"]
    col[side & (d < 0.0036)] = C["eye"]
    body = part == PART["body"]
    rgb[body] = col[body]
    height[body] = hb[body] + np.where(eye, 0.0006 * np.clip(1 - (d / 0.0047) ** 2, 0, 1), 0.0)[body]
    rough[body & beak] = 0.42
    rough[body & side & (d < 0.0036)] = 0.12
    # Wings: coverts forward and inboard, charcoal flight feathers with lighter edges and tips.
    a = np.abs(x)
    le, te = le_te(a)
    c = np.clip((f - te) / np.maximum(le - te, 1e-4), 0, 1)  # 0 trailing edge, 1 leading edge
    prim = a > 0.11
    spacing = np.where(prim, 0.013, 0.010)
    k, t = feathers(np.where(prim, a + (le - f) * 0.35, a), spacing)
    tip = fa.smoothstep(0.16, 0.0, c)  # trailing tips: the gaps widen into notches
    sep = np.clip(np.minimum(t, 1 - t) / (0.10 + 0.30 * tip), 0, 1)  # dark line between feathers
    flight = np.clip(fa.smoothstep(0.60, 0.48, c) + fa.smoothstep(0.115, 0.14, a), 0, 1)
    # Inner flight feathers (secondaries) stay slate; the outer hand (primaries) is charcoal (ref-1).
    dark = fa.mix(C["secondary"], C["flight"], fa.smoothstep(0.085, 0.12, a))
    fcol = dark * (0.62 + 0.38 * sep)[:, None]
    fcol = fa.mix(fcol, C["edge"] * 1.25, tip * sep * 0.55)
    ck, ct = feathers(f + a * 0.25, 0.007)
    cov = C["coverts"] * (0.93 + 0.09 * np.sin(np.pi * ct))[:, None]
    wtop = fa.mix(cov, fcol, flight)
    wtop = fa.mix(wtop, C["back"], fa.smoothstep(0.035, 0.018, a))
    wh = 0.00018 * sep * flight + 0.0001 * np.sin(np.pi * ct) * (1 - flight)
    wund = fa.mix(C["under"], C["under_dark"], flight * (0.7 + 0.3 * sep))
    wt, wu = part == PART["wing_top"], part == PART["wing_under"]
    rgb[wt], rgb[wu] = wtop[wt], wund[wu]
    height[wt], height[wu] = wh[wt], (0.00015 * sep * flight)[wu]
    rough[wt | wu] = 0.7
    # Tail: charcoal feathers fanned along f, blue-grey at the base, lighter tips.
    tk, tt = feathers(x + 0.03, 0.0075)
    tsep = np.clip(np.minimum(tt, 1 - tt) / 0.15, 0, 1)
    tcol = fa.mix(C["edge"], C["tail"], 0.5 + 0.5 * tsep)
    tcol = fa.mix(tcol, C["back"], fa.smoothstep(-0.062, -0.048, f))
    tcol = fa.mix(tcol, C["edge"] * 1.15, fa.smoothstep(-0.100, -0.110, f) * (1 - tsep) * 0.6)
    tp = (part == PART["tail_top"]) | (part == PART["tail_under"])
    rgb[tp] = tcol[tp]
    height[tp] = (0.00015 * tsep)[tp]
    rgb[part == PART["cap"]] = C["tail"]
    return {"rgb": np.clip(rgb, 0, 1), "rough": rough, "metal": np.zeros(n, np.float32), "height": height,
            "px_per_m": S * ATLAS}


def hooks(part):
    top = np.isin(part, [PART["body"], PART["wing_top"], PART["tail_top"]])
    return np.where(top, 0.2, 0.05).astype(np.float32), np.full(len(part), 0.3, np.float32)


mat = fa.setup(PIECE)
lods = [build(i, mat) for i in range(3)]
texel = fa.density(lods[0], ATLAS)
for o in lods[1:]:
    o.hide_render = True
stats = fa.paint_maps(lods[0], ATLAS, paint, [(0, None)], out, PIECE, skip_bake, MASK_SIZE, hooks)
for o in lods[1:]:
    o.hide_render = False
EXTRAS = {
    "form": "bird", "frame": "runtime +z forward, +y up; pivot at the body centre on the shoulder hinge line",
    "realLengthM": 0.197, "realSpanM": 0.40, "pivotToFeetM": 0.026,
    "tc1": {"u": "wing weight: 0 body and tail, (|x| - hingeX) / (tipX - hingeX) on the wings",
            "v": "tail weight: 0 at the tail base (z = -0.046), 1 at the tail tip (z = -0.112)"},
    "hinge": {"x": HINGE_X, "y": HINGE_Z, "tipX": TIP_X, "tailBaseZ": TAIL_BASE, "tailTipZ": TAIL_TIP},
    "flap": "U > 0: rotate (|x| - hingeX, y - hingeY) about the hinge line by sign(x) * theta",
    "fold": {"k": FOLD_K, "rollDeg": FOLD_ROLL, "back": FOLD_BACK,
             "rule": "U > 0: scale (|x| - hingeX, y - hingeY) by k, then flap it by rollDeg (down), z -= back * U"},
    "placeholder": {"lengthM": 1.5125, "spanM": 2.875, "scaleToMatch": 7.2},
}
fa.export_piece(PIECE, lods, out, __file__, EXTRAS, texel, ATLAS, MASK_SIZE, stats, skip_bake)
