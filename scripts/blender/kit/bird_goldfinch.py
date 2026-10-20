"""bird_goldfinch: the second boid bird, a European goldfinch, so flocks read as two species (PRD §7.2).
Form from the owner-approved bird references assets-source/refs/birds (ref-1 gliding in profile, ref-3
roosting on an oak branch, ref-2 flock layout): the same compact stylized songbird, here at goldfinch
size and in goldfinch plumage, which no reference image shows (owner question in the pending approval).
Scripted route; the shared builder, frame and map drawing are in _fauna.py (imported, not edited), and
the piece's own steps follow bird_starling.py, which cannot be imported (it builds at import).

Real scale: 0.128 m from beak tip to tail tip, 0.24 m wingspan. Faces runtime +z, +y up, pivot at the
body centre on the shoulder hinge line (the boid's simulated position). Against the starling it has a
larger round head on a shorter body, a stout pale cone of a beak, a shorter, blunter wing and a notched
tail; it is tan-backed with black wings crossed by a broad golden-yellow bar, white-tipped inner flight
feathers, a red face, black cap and nape band, white cheeks and belly, and a white-spotted black tail.
Feather tips and separations live in the albedo and normal map, not in geometry (owner, Q6). Every LOD
has the starling's topology (LOD0 184, LOD1 74, LOD2 22 tris, the ceiling for a boid bird).

Motion contract for the boid vertex shader, TEXCOORD_1 (FLOAT VEC2, every LOD), the starling's:
  U = wing weight: 0 on the body and tail, (|x| - HINGE_X) / (TIP_X - HINGE_X) on the wings (0 at the
      shoulder hinge, 1 at the wing tip). Flap: rotate every U > 0 vertex about the hinge line
      (runtime x = +-HINGE_X, y = HINGE_Z, parallel to z) by +-theta. Perch: scale its offset from the
      hinge line in the x-y plane by FOLD_K, flap it by FOLD_ROLL (down, so the wing lies along the
      upper flank) and move it back by FOLD_BACK * U along -z.
  V = tail weight: 0 at the tail base, 1 at the tail tip (for a tail pitch about the tail base).
The same numbers ride in the root node's extras (`boid`, a JSON string); the hinge, fold and feet
numbers are this piece's own (crates/hex-play/src/boids.rs `BIRDS`)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _fauna as fa  # noqa: E402

PIECE, ATLAS, MASK_SIZE, S = "bird_goldfinch", 512, 128, 5.0
TRI_CEILING = (184, 74, 22)  # bird_starling's LOD tris (its provenance)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

# Body stations (f, half width, half height, centre z), beak tip to tail base.
BODY = [
    (0.056, 0.0, 0.0, 0.0095),
    (0.046, 0.0042, 0.0048, 0.0100),
    (0.042, 0.0098, 0.0108, 0.0118),
    (0.032, 0.0132, 0.0134, 0.0120),
    (0.021, 0.0122, 0.0130, 0.0080),
    (0.008, 0.0168, 0.0178, 0.0010),
    (-0.008, 0.0162, 0.0168, -0.0020),
    (-0.021, 0.0118, 0.0112, 0.0010),
    (-0.031, 0.0055, 0.0048, 0.0040),
]
LOD_BODY = {0: (BODY, 10),
            1: ([BODY[i] for i in (0, 1, 3, 5, 7, 8)], 6),
            2: ([BODY[0], BODY[3], BODY[5], (-0.031, 0.0, 0.0, 0.0040)], 4)}
HINGE_X, HINGE_Z, TIP_X, DIHEDRAL = 0.013, 0.0095, 0.120, np.radians(6.0)
WING0 = [(0.011, 0.014), (0.045, 0.019), (0.088, 0.008), (0.120, -0.016), (0.110, -0.036),
         (0.060, -0.040), (0.011, -0.022)]
WING = {0: WING0, 1: [WING0[i] for i in (0, 1, 3, 5, 6)], 2: [WING0[i] for i in (0, 3, 6)]}
TAIL_BASE, TAIL_TIP, NOTCH = -0.027, -0.072, 0.009
TAIL0 = [(0.0055, TAIL_BASE), (0.0175, TAIL_TIP), (0.0, TAIL_TIP + NOTCH), (-0.0175, TAIL_TIP),
         (-0.0055, TAIL_BASE)]
TAIL = {0: TAIL0, 1: [TAIL0[i] for i in (0, 1, 3, 4)], 2: [(0.0, TAIL_BASE), (0.016, TAIL_TIP), (-0.016, TAIL_TIP)]}
FOLD_K, FOLD_ROLL, FOLD_BACK = 0.25, -60.0, 0.010
PIVOT_TO_FEET = 0.018
PART = {"body": 0, "wing_top": 1, "wing_under": 2, "tail_top": 3, "tail_under": 4, "cap": 5}
EPS = 0.0008


def body_uv(f, arc):
    return (0.02 + (0.056 - f) * S, 0.02 + arc * S)


def cap_uv(p):
    return (0.86 + (p[0] + 0.006) * S, 0.05 + (p[2] + 0.001) * S)


def wing_uv(a, f, side):
    return (0.02 + (a - 0.011) * S, (0.667 if side > 0 else 0.337) + (f + 0.0425) * S)


def tail_uv(x, f, side):
    return (0.60 + (x + 0.018) * S, 0.02 + (-0.025 - f) * S)


def wing_z(a):
    return HINGE_Z + max(a - HINGE_X, 0.0) * np.tan(DIHEDRAL)


def tail_z(f):
    return 0.0045 + (f - TAIL_BASE) * 0.08


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
                 {1: PART["wing_top"], -1: PART["wing_under"]}, wing_uv, mo_wing, nrm, EPS)
    fa.plate(b, TAIL[level], lambda x, f: (x, f, tail_z(f)), {1: PART["tail_top"], -1: PART["tail_under"]},
             tail_uv, mo_tail, (0.0, 0.08, 1.0), EPS)
    return b.finish(f"{PIECE}_lod{level}", mat)


C = {k: fa.hexc(v) for k, v in {
    "back": "#b48a5c", "back_dark": "#9a744d", "breast": "#cfa67a", "belly": "#efe8dc", "rump": "#ebe4d6",
    "cheek": "#eeece6", "black": "#1b1a1e", "edge": "#37363c", "red": "#c2242a", "yellow": "#f0c41c",
    "yellow_under": "#d9b434", "white": "#f2f0ea", "under": "#c3c1bc", "under_dark": "#56565c",
    "beak": "#dcc6b2", "beak_tip": "#4a4248", "eye": "#0b0b0d"}.items()}
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


def band(lo0, lo1, hi1, hi0, x):
    """1 between lo1 and hi1, easing to 0 at lo0 and hi0."""
    return fa.smoothstep(lo0, lo1, x) * fa.smoothstep(hi0, hi1, x)


def paint(P, part, variant):
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    n = len(P)
    rgb = np.zeros((n, 3), np.float32)
    rough = np.full(n, 0.78, np.float32)
    height = np.zeros(n, np.float32)
    # Body: tan back, buff breast sides, white belly and a pale rump.
    zc = np.interp(f, ST[:, 0], ST[:, 3])
    hh = np.maximum(np.interp(f, ST[:, 0], ST[:, 2]), 1e-4)
    v = np.clip((z - zc) / hh, -1.5, 1.5)
    col = fa.mix(C["belly"], C["breast"], fa.smoothstep(-0.60, -0.15, v))
    col = fa.mix(col, fa.mix(C["back_dark"], C["back"], fa.smoothstep(-0.01, 0.012, f)), fa.smoothstep(0.05, 0.40, v))
    col = fa.mix(col, C["rump"], fa.smoothstep(-0.011, -0.019, f) * fa.smoothstep(0.0, 0.45, v))
    # Head: white cheeks and throat, a black cap and a black band behind the cheek, a red face.
    head = fa.smoothstep(0.013, 0.018, f)
    col = fa.mix(col, C["cheek"], head)
    cap = fa.smoothstep(0.12, 0.30, v) * band(0.016, 0.019, 0.0365, 0.0395, f)
    nape = band(0.014, 0.017, 0.0235, 0.0265, f) * fa.smoothstep(-0.50, -0.30, v)
    col = fa.mix(col, C["black"], np.maximum(cap, nape))
    col = fa.mix(col, C["red"], fa.smoothstep(0.0365, 0.0395, f))
    # Feather texture: offset rows of soft scallops, lighter tips.
    k, t = feathers(f, 0.0045)
    scal = np.cos(np.pi * (t * 2 - 1)) * 0.5 + 0.5
    col = col * (0.95 + 0.07 * scal)[:, None]
    hb = 0.00009 * scal
    beak = f > 0.0455
    col[beak] = fa.mix(C["beak"], C["beak_tip"], fa.smoothstep(0.0515, 0.0545, f[beak]))
    d = np.hypot(f - 0.0372, z - 0.0150)
    side = np.abs(x) > 0.006
    eye = side & (d < 0.0026)
    col[eye] = C["eye"]
    body = part == PART["body"]
    rgb[body] = col[body]
    height[body] = hb[body] + np.where(eye, 0.0004 * np.clip(1 - (d / 0.0026) ** 2, 0, 1), 0.0)[body]
    rough[body & beak] = 0.42
    rough[body & eye] = 0.12
    # Wings: black, a broad golden bar across the bases of the flight feathers, white-tipped inner
    # flight feathers, black lesser coverts at the leading edge, tan scapulars by the body.
    a = np.abs(x)
    le, te = le_te(a)
    c = np.clip((f - te) / np.maximum(le - te, 1e-4), 0, 1)  # 0 trailing edge, 1 leading edge
    prim = a > 0.068
    spacing = np.where(prim, 0.0085, 0.0065)
    k, t = feathers(np.where(prim, a + (le - f) * 0.35, a), spacing)
    tip = fa.smoothstep(0.16, 0.0, c)
    sep = np.clip(np.minimum(t, 1 - t) / (0.10 + 0.30 * tip), 0, 1)
    bar = band(0.34, 0.40, 0.60, 0.66, c) * fa.smoothstep(0.114, 0.090, a)
    fcol = C["black"] * (0.70 + 0.30 * sep)[:, None]
    fcol = fa.mix(fcol, C["edge"] * 1.2, tip * sep * 0.45)
    fcol = fa.mix(fcol, C["yellow"] * (0.90 + 0.10 * sep)[:, None], bar)
    spots = fa.smoothstep(0.11, 0.03, c) * fa.smoothstep(0.078, 0.062, a) * sep
    fcol = fa.mix(fcol, C["white"], spots * 0.9)
    ck, ct = feathers(f + a * 0.25, 0.005)
    cov = C["black"] * (0.95 + 0.12 * np.sin(np.pi * ct))[:, None]
    wtop = fa.mix(fcol, cov, fa.smoothstep(0.64, 0.72, c))
    wtop = fa.mix(wtop, C["back"], fa.smoothstep(0.024, 0.015, a))
    wh = 0.00014 * sep * (1 - fa.smoothstep(0.64, 0.72, c)) + 0.00008 * np.sin(np.pi * ct) * fa.smoothstep(0.64, 0.72, c)
    flight = fa.smoothstep(0.62, 0.50, c)
    wund = fa.mix(C["under"], C["under_dark"], flight * (0.7 + 0.3 * sep))
    wund = fa.mix(wund, C["yellow_under"], bar * 0.85)
    wt, wu = part == PART["wing_top"], part == PART["wing_under"]
    rgb[wt], rgb[wu] = wtop[wt], wund[wu]
    height[wt], height[wu] = wh[wt], (0.00012 * sep * flight)[wu]
    rough[wt | wu] = 0.7
    # Tail: black feathers fanned along f with a white spot near each outer tip.
    tk, tt = feathers(x + 0.02, 0.005)
    tsep = np.clip(np.minimum(tt, 1 - tt) / 0.15, 0, 1)
    tcol = fa.mix(C["edge"], C["black"], 0.5 + 0.5 * tsep)
    spot = band(TAIL_TIP + 0.006, TAIL_TIP + 0.009, TAIL_TIP + 0.014, TAIL_TIP + 0.018, f) \
        * fa.smoothstep(0.004, 0.008, np.abs(x)) * tsep
    tcol = fa.mix(tcol, C["white"], spot * 0.9)
    tp = (part == PART["tail_top"]) | (part == PART["tail_under"])
    rgb[tp] = tcol[tp]
    height[tp] = (0.00012 * tsep)[tp]
    rgb[part == PART["cap"]] = C["black"]
    return {"rgb": np.clip(rgb, 0, 1), "rough": rough, "metal": np.zeros(n, np.float32), "height": height,
            "px_per_m": S * ATLAS}


def hooks(part):
    top = np.isin(part, [PART["body"], PART["wing_top"], PART["tail_top"]])
    return np.where(top, 0.2, 0.05).astype(np.float32), np.full(len(part), 0.3, np.float32)


def uv_bounds(obj):
    """UV0 bounds per part on LOD0, so overlapping islands fail the build, not the look."""
    me = obj.data
    uvd = me.uv_layers["UVMap"].data
    parts = me.attributes["part"].data
    box = {}
    for poly in me.polygons:
        p = parts[poly.index].value
        uvs = np.array([uvd[li].uv[:] for li in poly.loop_indices])
        if p == PART["body"] and uvs[:, 0].min() >= 0.8:
            p = PART["cap"]  # the loft maps its end cap (body part) by cap_uv
        lo, hi = box.get(p, (np.full(2, 9.0), np.full(2, -9.0)))
        box[p] = (np.minimum(lo, uvs.min(0)), np.maximum(hi, uvs.max(0)))
    return box


mat = fa.setup(PIECE)
lods = [build(i, mat) for i in range(3)]
boxes = uv_bounds(lods[0])
names = {v: k for k, v in PART.items()}
for p, (lo, hi) in sorted(boxes.items()):
    print(f"{PIECE}: uv {names[p]} u {lo[0]:.3f}..{hi[0]:.3f} v {lo[1]:.3f}..{hi[1]:.3f}")
    assert lo.min() >= 0.0 and hi.max() <= 1.0, (names[p], lo, hi)
# Shared islands (both wings, both tail sides) are intended; distinct parts must not overlap, except
# the two tail sides, which share one island.
islands = {"body": [0], "wing_top": [1], "wing_under": [2], "tail": [3, 4], "cap": [5]}
rects = {k: (np.min([boxes[p][0] for p in v], 0), np.max([boxes[p][1] for p in v], 0)) for k, v in islands.items()}
for i, (ka, (la, ha)) in enumerate(rects.items()):
    for kb, (lb, hb) in list(rects.items())[i + 1:]:
        assert (ha <= lb).any() or (hb <= la).any(), f"{PIECE}: UV islands {ka} and {kb} overlap"
texel = fa.density(lods[0], ATLAS)
for o in lods[1:]:
    o.hide_render = True
stats = fa.paint_maps(lods[0], ATLAS, paint, [(0, None)], out, PIECE, skip_bake, MASK_SIZE, hooks)
for o in lods[1:]:
    o.hide_render = False
EXTRAS = {
    "form": "bird", "species": "goldfinch",
    "frame": "runtime +z forward, +y up; pivot at the body centre on the shoulder hinge line",
    "realLengthM": 0.128, "realSpanM": 0.24, "pivotToFeetM": PIVOT_TO_FEET,
    "tc1": {"u": "wing weight: 0 body and tail, (|x| - hingeX) / (tipX - hingeX) on the wings",
            "v": f"tail weight: 0 at the tail base (z = {TAIL_BASE}), 1 at the tail tip (z = {TAIL_TIP})"},
    "hinge": {"x": HINGE_X, "y": HINGE_Z, "tipX": TIP_X, "tailBaseZ": TAIL_BASE, "tailTipZ": TAIL_TIP},
    "flap": "U > 0: rotate (|x| - hingeX, y - hingeY) about the hinge line by sign(x) * theta",
    "fold": {"k": FOLD_K, "rollDeg": FOLD_ROLL, "back": FOLD_BACK,
             "rule": "U > 0: scale (|x| - hingeX, y - hingeY) by k, then flap it by rollDeg (down), z -= back * U"},
}
tris = fa.export_piece(PIECE, lods, out, __file__, EXTRAS, texel, ATLAS, MASK_SIZE, stats, skip_bake)
assert all(t <= c for t, c in zip(tris, TRI_CEILING)), (tris, TRI_CEILING)
