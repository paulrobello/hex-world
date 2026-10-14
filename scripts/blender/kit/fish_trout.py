"""fish_trout: a stylized-real trout boid (owner-approved reference assets-source/refs/fish ref-3, with
both of its palettes; ref-2 is school layout only, ref-1 is rejected). Scripted route; the shared
builder, frame and map drawing are in _fauna.py.

Real scale: 0.40 m from snout to the caudal fork tips. Faces runtime +z, +y up, pivot at the body centre
on the length axis (the boid's simulated position). Streamlined body with a forked caudal fin, dorsal,
adipose and anal fins, and paired pectoral and pelvic fins, every fin a thin double-sided plate.

Two palettes, one texture set: the albedo atlas carries the ocean palette (blue-grey back, silver
sides, white belly, grey fins) in its left half (u 0 to 0.5) and the river palette (olive-brown back,
golden-olive sides with dark spots, cream belly, orange-red lower fins) in its right half. UV0 maps
every LOD into the left half; the river variant samples at UV0 + (0.5, 0). The normal, ORM and masks
maps carry identical content in both halves, so a StandardMaterial with
`uv_transform = Affine2::from_translation(vec2(0.5, 0.0))` (the transform the kit already applies for
tileUvScale) or a per-instance offset in the boid shader selects the river fish with no extra sampled
texture. ORM differs only where the palettes do (the ocean flanks are slightly metallic).

Motion contract for the boid vertex shader, TEXCOORD_1 (FLOAT VEC2, every LOD):
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

PIECE, ATLAS, MASK_SIZE, S = "fish_trout", 512, 128, 2.2
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"

# Body stations (f, half width, half height, centre z), snout to caudal peduncle.
BODY = [
    (0.200, 0.0, 0.0, -0.004),
    (0.192, 0.008, 0.010, -0.003),
    (0.172, 0.015, 0.022, 0.000),
    (0.140, 0.020, 0.033, 0.002),
    (0.100, 0.023, 0.041, 0.003),
    (0.050, 0.024, 0.045, 0.003),
    (0.000, 0.023, 0.044, 0.002),
    (-0.045, 0.019, 0.036, 0.002),
    (-0.085, 0.012, 0.024, 0.002),
    (-0.112, 0.006, 0.016, 0.003),
]
LOD_BODY = {0: (BODY, 12), 1: ([BODY[i] for i in (0, 2, 3, 5, 7, 9)], 8), 2: ([BODY[i] for i in (0, 3, 6, 9)], 4)}
WAG_F, TAIL_TIP = 0.06, -0.200
PART = {"body": 0, "caudal": 1, "dorsal": 2, "adipose": 3, "anal": 4, "pelvic": 5, "pectoral": 6, "cap": 7}
# Vertical fins in the x = 0 plane: outline (f, z).
CAUDAL = [(-0.104, 0.012), (-0.200, 0.052), (-0.170, 0.002), (-0.200, -0.046), (-0.104, -0.008)]
DORSAL0 = [(0.040, 0.043), (0.010, 0.086), (-0.026, 0.056), (-0.020, 0.041)]
ADIPOSE = [(-0.068, 0.030), (-0.082, 0.042), (-0.090, 0.026)]
ANAL0 = [(-0.042, -0.030), (-0.058, -0.062), (-0.082, -0.052), (-0.074, -0.020)]
# Paired fins: outline (a along the fin, b across it) from a root on the flank.
PECT0 = [(0.0, 0.0), (0.030, 0.006), (0.042, 0.020), (0.008, 0.016)]
PELV0 = [(0.006, 0.0), (-0.022, 0.022), (-0.030, 0.016), (-0.020, 0.0)]
PECT_ROOT, PELV_ROOT = (0.016, 0.112, -0.022), (0.010, -0.002, -0.034)
FINS = {
    0: {"caudal": CAUDAL, "dorsal": DORSAL0, "adipose": ADIPOSE, "anal": ANAL0, "pect": PECT0, "pelv": PELV0},
    1: {"caudal": CAUDAL, "dorsal": DORSAL0, "anal": [ANAL0[i] for i in (0, 1, 3)],
        "pect": [PECT0[i] for i in (0, 2, 3)]},
    2: {"caudal": CAUDAL, "dorsal": [DORSAL0[i] for i in (0, 1, 3)]},
}
EPS = 0.0008


def body_uv(f, arc):
    return (0.02 + arc * S, 0.02 + (0.200 - f) * S)


def cap_uv(p):
    return (0.44 + (p[0] + 0.006) * S, 0.90 + (p[2] + 0.013) * S)


def mo(p):
    return (float(np.clip((WAG_F - p[1]) / (WAG_F - TAIL_TIP), 0.0, 1.0)), 0.0)


def unit(v):
    v = np.asarray(v, float)
    return v / np.linalg.norm(v)


def paired(root, s, dir_a, dir_b):
    """place(a, b) and the outward normal of a paired fin on side s (+1 left, -1 right)."""
    da, db = unit((dir_a[0] * s, dir_a[1], dir_a[2])), unit((dir_b[0] * s, dir_b[1], dir_b[2]))
    r = np.array((root[0] * s, root[1], root[2]))
    nrm = np.cross(da, db)
    if nrm[0] * s < 0:
        nrm = -nrm
    return (lambda a, b: tuple(r + a * da + b * db)), tuple(nrm)


def build(level, mat):
    b = fa.Builder()
    stations, n = LOD_BODY[level]
    fa.loft(b, stations, n, PART["body"], body_uv, mo, cap_uv)
    fins = FINS[level]
    vert = lambda f, z: (0.0, f, z)  # noqa: E731
    fa.plate(b, fins["caudal"], vert, PART["caudal"],
             lambda f, z, s: (0.02 + (z + 0.048) * S, 0.735 + (-0.102 - f) * S), mo, (1.0, 0.0, 0.0), EPS)
    fa.plate(b, fins["dorsal"], vert, PART["dorsal"],
             lambda f, z, s: (0.29 + (z - 0.040) * S, 0.02 + (0.042 - f) * S), mo, (1.0, 0.0, 0.0), EPS)
    if "adipose" in fins:
        fa.plate(b, fins["adipose"], vert, PART["adipose"],
                 lambda f, z, s: (0.42 + (z - 0.024) * S, 0.02 + (-0.066 - f) * S), mo, (1.0, 0.0, 0.0), EPS)
    if "anal" in fins:
        fa.plate(b, fins["anal"], vert, PART["anal"],
                 lambda f, z, s: (0.29 + (z + 0.064) * S, 0.20 + (-0.040 - f) * S), mo, (1.0, 0.0, 0.0), EPS)
    for s in (1, -1):
        if "pect" in fins:
            place, nrm = paired(PECT_ROOT, s, (0.35, -1.0, -0.30), (0.25, 0.0, -1.0))
            fa.plate(b, fins["pect"], place, PART["pectoral"], lambda a, bb, side: (0.29 + a * S, 0.32 + bb * S),
                     mo, nrm, EPS)
        if "pelv" in fins:
            place, nrm = paired(PELV_ROOT, s, (0.0, 1.0, 0.0), (np.sin(np.radians(35)), 0.0, -np.cos(np.radians(35))))
            fa.plate(b, fins["pelv"], place, PART["pelvic"], lambda a, bb, side: (0.40 + bb * S, 0.20 + (0.008 - a) * S),
                     mo, nrm, EPS)
    return b.finish(f"{PIECE}_lod{level}", mat)


PALETTES = {
    "ocean": {"back": "#56768d", "upper": "#8ba3b5", "side": "#cdd6dd", "belly": "#eef1f2", "fin": "#5e7b8e",
              "fin_edge": "#47606f", "low_fin": "#c4cbd1", "low_root": "#aab3ba", "iris": "#b8bcc0"},
    "river": {"back": "#4f4c24", "upper": "#7d7638", "side": "#b3a861", "belly": "#e3dbbf", "fin": "#6b6533",
              "fin_edge": "#4c4722", "low_fin": "#cf5f34", "low_root": "#a39349", "iris": "#b39a52"},
}
PAL = {k: {c: fa.hexc(v) for c, v in p.items()} for k, p in PALETTES.items()}
ST = np.array(sorted(BODY[1:], key=lambda s: s[0]))
# River spots (f, v, radius m), upper flank and back, from a fixed seed.
_rng = np.random.default_rng(7)
SPOTS = np.stack([_rng.uniform(-0.085, 0.150, 22), _rng.uniform(-0.15, 0.95, 22), _rng.uniform(0.0026, 0.0042, 22)], 1)


def paint(P, part, variant):
    p = PAL[variant]
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    n = len(P)
    zc = np.interp(f, ST[:, 0], ST[:, 3])
    hh = np.maximum(np.interp(f, ST[:, 0], ST[:, 2]), 1e-4)
    v = np.clip((z - zc) / hh, -1.5, 1.5)
    col = fa.mix(p["belly"], p["side"], fa.smoothstep(-0.75, -0.30, v))
    col = fa.mix(col, p["upper"], fa.smoothstep(0.05, 0.42, v))
    col = fa.mix(col, p["back"], fa.smoothstep(0.42, 0.82, v))
    height = np.zeros(n, np.float32)
    # Gill cover: a backward-bowed edge, the plate raised a little in front of it.
    dg = f - (0.122 + 0.012 * (1 - v ** 2))
    col = col * (1 - 0.16 * fa.smoothstep(0.0022, 0.0, np.abs(dg)))[:, None]
    height += np.where(dg > 0, 0.00035, 0.0) * (np.abs(v) < 0.95)
    # Mouth: a short downturned line from the snout tip.
    vm = -0.18 - (0.199 - f) / 0.023 * 0.2
    mouth = (f > 0.176) & (f < 0.199) & (np.abs(v - vm) < 0.06)
    col[mouth] *= 0.6
    # Lateral line, and the river fish's faint dark mid-body band.
    col = col * (1 + 0.05 * fa.smoothstep(0.02, 0.0, np.abs(v - 0.05)) * (f < 0.11))[:, None]
    if variant == "river":
        col = col * (1 - 0.08 * fa.smoothstep(0.25, 0.0, np.abs(v - 0.10)) * fa.smoothstep(0.12, 0.06, f))[:, None]
        for sf, sv, r in SPOTS:
            sz = np.interp(sf, ST[:, 0], ST[:, 3]) + sv * np.interp(sf, ST[:, 0], ST[:, 2])
            d = np.hypot(f - sf, z - sz)
            col = fa.mix(col, fa.hexc("#3b3524"), 0.85 * fa.smoothstep(r, r * 0.55, d))
    # Eye: dark pupil, metallic iris ring, slight bulge.
    fe, ve = 0.168, 0.30
    ze = np.interp(fe, ST[:, 0], ST[:, 3]) + ve * np.interp(fe, ST[:, 0], ST[:, 2])
    de = np.hypot(f - fe, z - ze)
    side = np.abs(x) > 0.006
    col = np.where((side & (de < 0.0075))[:, None], fa.hexc("#22262a"), col)
    col = np.where((side & (de < 0.0068))[:, None], p["iris"], col)
    col = np.where((side & (de < 0.0046))[:, None], fa.hexc("#0b0c0e"), col)
    height += np.where(side & (de < 0.0075), 0.0008 * np.clip(1 - (de / 0.0075) ** 2, 0, 1), 0.0)
    rgb = col.copy()
    body = part == 0
    rough = np.where(body, 0.32 if variant == "ocean" else 0.42, 0.5).astype(np.float32)
    rough[body & side & (de < 0.0068)] = 0.1
    silver = fa.smoothstep(-0.8, -0.2, v) * fa.smoothstep(0.7, 0.2, v)
    metal = np.where(body, silver * (0.35 if variant == "ocean" else 0.08), 0.0).astype(np.float32)
    # Fins: rays fanned from each fin's root, darker edges; the lower fins of the river fish are orange.
    upper_fin = np.isin(part, [PART["caudal"], PART["dorsal"], PART["adipose"]])
    caudal = part == PART["caudal"]
    ang = np.where(caudal, np.arctan2(z - 0.002, -(f + 0.104)), np.arctan2(z - 0.040, 0.03 - f))
    rays = 0.5 + 0.5 * np.sin(ang * 44)
    root_d = np.where(caudal, np.hypot(f + 0.104, z - 0.002) / 0.1, np.clip((z - 0.040) / 0.045, 0, 1))
    fin_col = fa.mix(p["fin"], p["fin_edge"], fa.smoothstep(0.55, 1.0, root_d))
    fin_col = fin_col * (0.94 + 0.08 * rays)[:, None]
    rgb[upper_fin] = fin_col[upper_fin]
    height[upper_fin] = (0.00012 * rays)[upper_fin]
    low = np.isin(part, [PART["anal"], PART["pelvic"], PART["pectoral"]])
    ax = np.abs(x)
    d_pect = np.hypot(np.hypot(ax - PECT_ROOT[0], f - PECT_ROOT[1]), z - PECT_ROOT[2]) / 0.045
    d_pelv = np.hypot(np.hypot(ax - PELV_ROOT[0], f - PELV_ROOT[1]), z - PELV_ROOT[2]) / 0.032
    bottom = np.interp(f, ST[:, 0], ST[:, 3]) - np.interp(f, ST[:, 0], ST[:, 2])
    d_anal = np.clip((bottom - z) / 0.03, 0, 1)
    rd = np.where(part == PART["pectoral"], d_pect, np.where(part == PART["pelvic"], d_pelv, d_anal))
    lray = 0.5 + 0.5 * np.sin(rd * 40)
    low_col = fa.mix(p["low_root"], p["low_fin"], fa.smoothstep(0.1, 0.45, rd)) * (0.95 + 0.07 * lray)[:, None]
    rgb[low] = low_col[low]
    height[low] = (0.0001 * lray)[low]
    rgb[part == PART["cap"]] = p["fin"]
    return {"rgb": np.clip(rgb, 0, 1), "rough": rough, "metal": metal, "height": height, "px_per_m": S * ATLAS}


def hooks(part):
    # Under water at every sea level: no snow (the slot is SUBMERGED in the hooks gate); always wet.
    return np.zeros(len(part), np.float32), np.full(len(part), 1.0, np.float32)


mat = fa.setup(PIECE)
lods = [build(i, mat) for i in range(3)]
texel = fa.density(lods[0], ATLAS)
stats = fa.paint_maps(lods[0], ATLAS, paint, [(0, "ocean"), (ATLAS // 2, "river")], out, PIECE, skip_bake,
                      MASK_SIZE, hooks)
EXTRAS = {
    "form": "fish", "frame": "runtime +z forward, +y up; pivot at the body centre on the length axis",
    "realLengthM": 0.40,
    "tc1": {"u": "wag weight: 0 forward of z = 0.06, linear to 1 at the caudal fork tips (z = -0.20)",
            "v": "0 (reserved)"},
    "wag": "x += U^2 * amplitude * sin(phase)",
    "variants": {"ocean": [0.0, 0.0], "river": [0.5, 0.0], "rule": "UV0 offset per instance; one texture set"},
    "placeholder": {"lengthM": 1.08, "scaleToMatch": 2.7},
}
fa.export_piece(PIECE, lods, out, __file__, EXTRAS, texel, ATLAS, MASK_SIZE, stats, skip_bake)
