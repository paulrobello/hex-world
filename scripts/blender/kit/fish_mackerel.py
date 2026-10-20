"""fish_mackerel: the second boid fish, an Atlantic mackerel, so sea water (river mouth, coast and ocean
tiles) reads as a different species from the river trout (PRD §7.2). Form from the owner-approved fish
references assets-source/refs/fish (ref-3's stylized-real body and its satin finish, ref-2's school of
dark blue-grey torpedo silhouettes over sand): the same smooth sculpted fish, here at mackerel shape and in
mackerel colours, which no reference image shows (owner question in the pending approval). Scripted
route; the shared builder, frame and map drawing are in _fauna.py (imported, not edited), and the piece's
own steps follow fish_trout.py, which cannot be imported (it builds at import).

Real scale: 0.40 m from snout to the caudal fork tips, the trout's length, and inside the trout's height
(runtime y -0.062..0.086 m), so the confinement band built around the trout (boids::plan TROUT_TOP,
TROUT_BELLY, FISH_HALF_H, LEVEL_BAND) holds for it unchanged. Faces runtime +z, +y up, pivot at the body
centre on the length axis (the boid's simulated position). Against the trout it is slimmer and rounder in
section with a pointed snout, a slim caudal peduncle and a deeply forked tail, two well separated dorsal
fins, five finlets above and five below between them and the tail, and small paired fins. Iridescent
blue-green back crossed by dark wavy bars, silver flanks with a faint warm sheen, a white belly, dusky
fins. Every LOD stays at or under fish_trout's tris (LOD0 246, LOD1 94, LOD2 30).

One palette (sea water only): the trout draws the river-palette fish and this piece the ocean-palette
fish (boids::FISH), so UV0 uses the whole atlas and the boid shader's river lift never applies to it.

Motion contract for the boid vertex shader, TEXCOORD_1 (FLOAT VEC2, every LOD), the trout's:
  U = wag weight: 0 forward of WAG_F (runtime z = 0.06), rising linearly to 1 at the caudal fork tips
      (z = -0.20). Wag: x += U^2 * amplitude * sin(phase).
  V = 0 (reserved).
The same numbers ride in the root node's extras (`boid`, a JSON string)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _fauna as fa  # noqa: E402

PIECE, ATLAS, MASK_SIZE, S = "fish_mackerel", 512, 128, 2.6
TRI_CEILING = (246, 94, 30)  # fish_trout's LOD tris (its provenance)
TROUT_Y = (-0.062, 0.086)  # fish_trout's LOD0 runtime y span at real scale (boids::plan TROUT_TOP / BELLY)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

# Body stations (f, half width, half height, centre z), snout to caudal peduncle.
BODY = [
    (0.200, 0.0, 0.0, -0.002),
    (0.186, 0.0065, 0.0075, -0.002),
    (0.162, 0.0145, 0.0185, 0.000),
    (0.122, 0.0205, 0.0290, 0.001),
    (0.062, 0.0235, 0.0355, 0.002),
    (0.000, 0.0225, 0.0340, 0.002),
    (-0.060, 0.0165, 0.0245, 0.002),
    (-0.104, 0.0090, 0.0120, 0.002),
    (-0.130, 0.0050, 0.0060, 0.002),
]
LOD_BODY = {0: (BODY, 12), 1: ([BODY[i] for i in (0, 2, 4, 6, 8)], 8), 2: ([BODY[i] for i in (0, 3, 6, 8)], 4)}
WAG_F, TAIL_TIP = 0.06, -0.200
PART = {"body": 0, "caudal": 1, "dorsal": 2, "dorsal2": 3, "anal": 4, "pelvic": 5, "pectoral": 6, "cap": 7,
        "finlet": 8}
# Vertical fins in the x = 0 plane: outline (f, z). A deep fork with narrow lobes.
CAUDAL0 = [(-0.122, 0.007), (-0.200, 0.048), (-0.181, 0.014), (-0.160, 0.001), (-0.181, -0.012),
           (-0.200, -0.044), (-0.122, -0.005)]
CAUDAL_LOW = [CAUDAL0[i] for i in (0, 1, 3, 5, 6)]
DORSAL0 = [(0.078, 0.033), (0.064, 0.061), (0.046, 0.051), (0.030, 0.035)]
DORSAL2 = [(-0.026, 0.029), (-0.040, 0.050), (-0.056, 0.023)]
ANAL = [(-0.026, -0.025), (-0.040, -0.046), (-0.056, -0.019)]
# Paired fins: outline (a along the fin, b across it) from a root on the flank.
PECT0 = [(0.0, 0.0), (0.032, 0.005), (0.036, 0.014), (0.006, 0.012)]
PELV0 = [(0.004, 0.0), (-0.018, 0.016), (-0.022, 0.006)]
PECT_ROOT, PELV_ROOT = (0.017, 0.118, -0.006), (0.008, 0.100, -0.028)
FINLETS, FINLET_F0, FINLET_STEP = 5, -0.064, 0.0115
FINS = {
    0: {"caudal": CAUDAL0, "dorsal": DORSAL0, "dorsal2": DORSAL2, "anal": ANAL, "pect": PECT0, "pelv": PELV0,
        "finlets": True},
    1: {"caudal": CAUDAL_LOW, "dorsal": [DORSAL0[i] for i in (0, 1, 3)], "pect": [PECT0[i] for i in (0, 2, 3)]},
    2: {"caudal": CAUDAL_LOW, "dorsal": [DORSAL0[i] for i in (0, 1, 3)]},
}
EPS = 0.0008
ST = np.array(sorted(BODY[1:], key=lambda s: s[0]))


def top(f):
    return float(np.interp(f, ST[:, 0], ST[:, 3] + ST[:, 2]))


def bottom(f):
    return float(np.interp(f, ST[:, 0], ST[:, 3] - ST[:, 2]))


def body_uv(f, arc):
    return (0.02 + arc * S, 0.02 + (0.200 - f) * S)


def cap_uv(p):
    return (0.30 + (p[0] + 0.006) * S, 0.93 + (p[2] + 0.005) * S)


def mo(p):
    return (float(np.clip((WAG_F - p[1]) / (WAG_F - TAIL_TIP), 0.0, 1.0)), 0.0)


def unit(v):
    v = np.asarray(v, float)
    return v / np.linalg.norm(v)


def paired(root, s, dir_a, dir_b):
    """place(a, b) and the outward normal of a paired fin on side s (+1 left, -1 right); fish_trout's."""
    da, db = unit((dir_a[0] * s, dir_a[1], dir_a[2])), unit((dir_b[0] * s, dir_b[1], dir_b[2]))
    r = np.array((root[0] * s, root[1], root[2]))
    nrm = np.cross(da, db)
    if nrm[0] * s < 0:
        nrm = -nrm
    return (lambda a, b: tuple(r + a * da + b * db)), tuple(nrm)


def finlet(k, upper):
    """Finlet k (0 nearest the second dorsal or the anal fin): a small triangle raking back, its base 2 mm
    inside the body's top (upper) or bottom line."""
    f0 = FINLET_F0 - k * FINLET_STEP
    line, sign = (top, 1.0) if upper else (bottom, -1.0)
    return [(f0, line(f0) - sign * 0.002), (f0 - 0.0105, line(f0 - 0.0105) + sign * 0.0075),
            (f0 - 0.0075, line(f0 - 0.0075) - sign * 0.002)]


def finlet_uv(k, upper):
    f0 = FINLET_F0 - k * FINLET_STEP
    u0, v0 = 0.32 + k * 0.06, (0.29 if upper else 0.36)
    if upper:
        base = top(f0) - 0.003
        return lambda f, z, s: (u0 + (f0 - f) * S, v0 + (z - base) * S)
    base = bottom(f0) + 0.003
    return lambda f, z, s: (u0 + (f0 - f) * S, v0 + (base - z) * S)


def build(level, mat):
    b = fa.Builder()
    stations, n = LOD_BODY[level]
    fa.loft(b, stations, n, PART["body"], body_uv, mo, cap_uv)
    fins = FINS[level]
    vert = lambda f, z: (0.0, f, z)  # noqa: E731
    x_axis = (1.0, 0.0, 0.0)
    fa.plate(b, fins["caudal"], vert, PART["caudal"],
             lambda f, z, s: (0.32 + (z + 0.052) * S, 0.02 + (-0.120 - f) * S), mo, x_axis, EPS)
    fa.plate(b, fins["dorsal"], vert, PART["dorsal"],
             lambda f, z, s: (0.64 + (z - 0.030) * S, 0.02 + (0.082 - f) * S), mo, x_axis, EPS)
    if "dorsal2" in fins:
        fa.plate(b, fins["dorsal2"], vert, PART["dorsal2"],
                 lambda f, z, s: (0.76 + (z - 0.021) * S, 0.02 + (-0.024 - f) * S), mo, x_axis, EPS)
    if "anal" in fins:
        fa.plate(b, fins["anal"], vert, PART["anal"],
                 lambda f, z, s: (0.86 + (z + 0.048) * S, 0.02 + (-0.024 - f) * S), mo, x_axis, EPS)
    if fins.get("finlets"):
        for k in range(FINLETS):
            for upper in (True, False):
                fa.plate(b, finlet(k, upper), vert, PART["finlet"], finlet_uv(k, upper), mo, x_axis, EPS)
    for s in (1, -1):
        if "pect" in fins:
            # Held out from the flank (the trout's lower root lets its fin clear the body at a flatter
            # angle; flat against this mid-flank root, its shared island baked AO 0.27).
            place, nrm = paired(PECT_ROOT, s, (0.35, -1.0, -0.20), (0.85, 0.0, -1.0))
            fa.plate(b, fins["pect"], place, PART["pectoral"], lambda a, bb, side: (0.64 + a * S, 0.20 + bb * S),
                     mo, nrm, EPS)
        if "pelv" in fins:
            place, nrm = paired(PELV_ROOT, s, (0.0, 1.0, 0.0), (np.sin(np.radians(35)), 0.0, -np.cos(np.radians(35))))
            fa.plate(b, fins["pelv"], place, PART["pelvic"],
                     lambda a, bb, side: (0.78 + (a + 0.024) * S, 0.20 + bb * S), mo, nrm, EPS)
    # The concave caudal fork: Builder.finish's ear clipping projects each ngon along its stored face
    # normal, which bmesh leaves unset on new faces, and then fans the fork across its notch.
    b.bm.normal_update()
    return b.finish(f"{PIECE}_lod{level}", mat)


C = {k: fa.hexc(v) for k, v in {
    "back": "#1d4d5a", "back_hi": "#2d6f7b", "crown": "#173e4a", "bar": "#0d1b22", "upper": "#6f93a0",
    "side": "#cdd6da", "sheen": "#d8d2bf", "belly": "#f3f4f2", "fin": "#4f5e66", "fin_edge": "#36434a",
    "low_fin": "#b3bcc1", "low_root": "#98a3a9", "iris": "#c9cdd0"}.items()}


def paint(P, part, variant):
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    n = len(P)
    zc = np.interp(f, ST[:, 0], ST[:, 3])
    hh = np.maximum(np.interp(f, ST[:, 0], ST[:, 2]), 1e-4)
    v = np.clip((z - zc) / hh, -1.5, 1.5)
    # Belly to back: white belly, silver flanks with a faint warm band, blue-silver upper flank, teal back.
    col = fa.mix(C["belly"], C["side"], fa.smoothstep(-0.70, -0.25, v))
    col = fa.mix(col, C["sheen"], 0.35 * fa.smoothstep(-0.25, -0.02, v) * fa.smoothstep(0.22, 0.02, v))
    col = fa.mix(col, C["upper"], fa.smoothstep(0.10, 0.30, v))
    col = fa.mix(col, fa.mix(C["back_hi"], C["back"], fa.smoothstep(0.45, 0.85, v)), fa.smoothstep(0.26, 0.46, v))
    col = fa.mix(col, C["crown"], fa.smoothstep(0.80, 1.0, v) * fa.smoothstep(0.12, 0.16, f))
    # Dark wavy bars over the back, from behind the head to the peduncle, down to the upper flank.
    phase = (f + 0.010 * np.sin(v * 7.0 + f * 35.0) - 0.012 * (v - 0.6)) / 0.0105
    line = fa.smoothstep(0.20, 0.08, np.abs(phase - np.floor(phase) - 0.5) - 0.06)
    fork = 0.6 + 0.4 * fa.smoothstep(-0.3, 0.3, np.sin(f * 190.0 + v * 4.0))
    mask = fa.smoothstep(0.30, 0.48, v) * fa.smoothstep(0.128, 0.104, f) * fa.smoothstep(-0.126, -0.104, f)
    col = fa.mix(col, C["bar"], 0.85 * line * fork * mask)
    # Mouth: a short line back from the pointed snout.
    vm = -0.20 - (0.199 - f) / 0.024 * 0.15
    mouth = (f > 0.172) & (f < 0.199) & (np.abs(v - vm) < 0.06)
    col[mouth] *= 0.55
    # Eye: dark pupil, silver iris ring, slight bulge.
    fe, ve = 0.170, 0.22
    ze = np.interp(fe, ST[:, 0], ST[:, 3]) + ve * np.interp(fe, ST[:, 0], ST[:, 2])
    de = np.hypot(f - fe, z - ze)
    side = np.abs(x) > 0.005
    col = np.where((side & (de < 0.0080))[:, None], fa.hexc("#20262a"), col)
    col = np.where((side & (de < 0.0072))[:, None], C["iris"], col)
    col = np.where((side & (de < 0.0048))[:, None], fa.hexc("#0b0c0e"), col)
    height = np.zeros(n, np.float32)
    height += np.where(side & (de < 0.0080), 0.0008 * np.clip(1 - (de / 0.0080) ** 2, 0, 1), 0.0)
    rgb = col.copy()
    body = part == PART["body"]
    rough = np.where(body, 0.30, 0.5).astype(np.float32)
    rough[body & side & (de < 0.0072)] = 0.1
    silver = fa.smoothstep(-0.8, -0.2, v) * fa.smoothstep(0.45, 0.10, v)
    metal = np.where(body, silver * 0.40, 0.0).astype(np.float32)
    # Fins: dusky, rays fanned from each fin's root, darker edges; pale paired fins.
    caudal = part == PART["caudal"]
    dorsal = part == PART["dorsal"]
    upper_fin = np.isin(part, [PART["caudal"], PART["dorsal"], PART["dorsal2"], PART["anal"], PART["finlet"]])
    ang = np.where(caudal, np.arctan2(z - 0.001, -(f + 0.122)), np.arctan2(np.abs(z) - 0.030, 0.054 - f))
    rays = 0.5 + 0.5 * np.sin(ang * np.where(caudal, 46.0, 30.0))
    root_d = np.where(caudal, np.hypot(f + 0.122, z - 0.001) / 0.085,
                      np.where(dorsal, np.clip((z - 0.034) / 0.027, 0, 1),
                               np.clip((np.abs(z) - np.abs(np.where(z > 0, top(0.0), bottom(0.0))) * 0.7) / 0.02, 0, 1)))
    fin_col = fa.mix(C["fin"], C["fin_edge"], fa.smoothstep(0.55, 1.0, root_d))
    fin_col = fin_col * (0.94 + 0.08 * np.where(caudal | dorsal, rays, 0.5))[:, None]
    rgb[upper_fin] = fin_col[upper_fin]
    height[caudal | dorsal] = (0.00012 * rays)[caudal | dorsal]
    low = np.isin(part, [PART["pelvic"], PART["pectoral"]])
    ax = np.abs(x)
    d_pect = np.hypot(np.hypot(ax - PECT_ROOT[0], f - PECT_ROOT[1]), z - PECT_ROOT[2]) / 0.038
    d_pelv = np.hypot(np.hypot(ax - PELV_ROOT[0], f - PELV_ROOT[1]), z - PELV_ROOT[2]) / 0.026
    rd = np.where(part == PART["pectoral"], d_pect, d_pelv)
    lray = 0.5 + 0.5 * np.sin(rd * 40)
    low_col = fa.mix(C["low_root"], C["low_fin"], fa.smoothstep(0.1, 0.5, rd)) * (0.95 + 0.07 * lray)[:, None]
    low_col = fa.mix(low_col, C["fin"], fa.smoothstep(0.7, 1.0, rd) * 0.5)
    rgb[low] = low_col[low]
    height[low] = (0.0001 * lray)[low]
    rgb[part == PART["cap"]] = C["fin"]
    return {"rgb": np.clip(rgb, 0, 1), "rough": rough, "metal": metal, "height": height, "px_per_m": S * ATLAS}


def hooks(part):
    # Under water at every sea level: no snow (the slot is SUBMERGED in the hooks gate); always wet.
    return np.zeros(len(part), np.float32), np.full(len(part), 1.0, np.float32)


def uv_bounds(obj):
    """UV0 bounds per part on LOD0 (the loft's end cap, a body face, by its cap_uv position)."""
    me = obj.data
    uvd = me.uv_layers["UVMap"].data
    parts = me.attributes["part"].data
    box = {}
    for poly in me.polygons:
        p = parts[poly.index].value
        uvs = np.array([uvd[li].uv[:] for li in poly.loop_indices])
        if p == PART["body"] and uvs[:, 1].min() >= 0.92:
            p = PART["cap"]
        key = (p, int(round((uvs[:, 0].min() - 0.32) / 0.06)), uvs[:, 1].min() > 0.34) if p == PART["finlet"] else p
        lo, hi = box.get(key, (np.full(2, 9.0), np.full(2, -9.0)))
        box[key] = (np.minimum(lo, uvs.min(0)), np.maximum(hi, uvs.max(0)))
    return box


def bounds(obj):
    """Creature-frame (x, f, z) min and max of obj's vertices."""
    co = np.array([v.co[:] for v in obj.data.vertices])
    co = np.stack([co[:, 0], -co[:, 1], co[:, 2]], 1)
    return co.min(0), co.max(0)


mat = fa.setup(PIECE)
lods = [build(i, mat) for i in range(3)]
for i, o in enumerate(lods):
    lo, hi = bounds(o)
    print(f"{PIECE}: lod{i} x {lo[0]:.4f}..{hi[0]:.4f} f {lo[1]:.4f}..{hi[1]:.4f} z {lo[2]:.4f}..{hi[2]:.4f}")
    # The trout's envelope: confinement (band height, skim gap, level band) is built around it.
    assert TROUT_Y[0] <= lo[2] and hi[2] <= TROUT_Y[1], (i, lo, hi)
    assert -0.2005 <= lo[1] and hi[1] <= 0.2005, (i, lo, hi)
boxes = uv_bounds(lods[0])
names = {v: k for k, v in PART.items()}
for key, (lo, hi) in sorted(boxes.items(), key=lambda kv: str(kv[0])):
    name = names[key[0]] if isinstance(key, tuple) else names[key]
    print(f"{PIECE}: uv {name} {key} u {lo[0]:.3f}..{hi[0]:.3f} v {lo[1]:.3f}..{hi[1]:.3f}")
    assert lo.min() >= 0.0 and hi.max() <= 1.0, (key, lo, hi)
items = list(boxes.items())
for i, (ka, (la, ha)) in enumerate(items):
    for kb, (lb, hb) in items[i + 1:]:
        # Islands keep a 0.01 gap (5 texels, past the AO bake's 4 px margin).
        assert (ha + 0.01 <= lb).any() or (hb + 0.01 <= la).any(), f"{PIECE}: UV islands {ka} and {kb} overlap"
texel = fa.density(lods[0], ATLAS)
stats = fa.paint_maps(lods[0], ATLAS, paint, [(0, None)], out, PIECE, skip_bake, MASK_SIZE, hooks)
EXTRAS = {
    "form": "fish", "species": "mackerel", "water": "sea",
    "frame": "runtime +z forward, +y up; pivot at the body centre on the length axis",
    "realLengthM": 0.40,
    "tc1": {"u": "wag weight: 0 forward of z = 0.06, linear to 1 at the caudal fork tips (z = -0.20)",
            "v": "0 (reserved)"},
    "wag": "x += U^2 * amplitude * sin(phase)",
    "variants": {"ocean": [0.0, 0.0], "rule": "one palette: drawn for the ocean-palette (sea-spawned) fish only"},
}
tris = fa.export_piece(PIECE, lods, out, __file__, EXTRAS, texel, ATLAS, MASK_SIZE, stats, skip_bake)
assert all(t <= c for t, c in zip(tris, TRI_CEILING)), (tris, TRI_CEILING)
