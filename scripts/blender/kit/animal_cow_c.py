"""animal_cow_c: animal_cow in a Hereford-like coat, the standing cow slot's hero variant (PRD §6
Variety, hex_core::HERO_VARIANTS; owner-approved references assets-source/refs/farm_animals ref-1 cow
and ref-3 overhead herd; owner decisions A17: hornless, brown and white, ref-2's smooth sculpted form,
no fur detail in geometry). Scripted route; the builder, atlas and map drawing are in _animals.py,
reused unchanged.

The same cow as animal_cow: its body, neck, head, legs, ears and tail stations are copied here
unchanged (animal_cow.py is a monolithic script, frozen with the approved piece), so the pose, size,
collider box and scatter footprint are animal_cow's. Only the coat differs: a deep red-brown body with
a white face and poll, a white crest behind the poll, a white dewlap, brisket and underline, white
socks below the knees and hocks, and a white tail switch; the ears, the cheeks behind the eyes and a
patch round each eye stay red. All three LODs are animal_cow's: LOD2 draws the head on four stations
(three drew a beak, animal_cow friction note 6) paid for by three-sided legs beyond 400 m, so no LOD
has more triangles than animal_cow's.

Real scale: 1.45 m at the withers, 2.32 m from muzzle tip to rump (the tail hangs 0.07 m behind it).
Faces runtime +z, +y up, +x the cow's left; pivot on the ground under the body centre. Kind `prop`,
one slot `animal_cow_c_body` (unique 1024 px albedo, 512 px normal and ORM, 128 px masks).

Motion contract (TEXCOORD_1, Blender UV map `motion`, FLOAT VEC2 on every primitive of every LOD;
`_animals.check_motion` re-reads it from the GLB): animal_cow's, unchanged (see animal_cow.py).
  U = leg weight: 0 off the legs; on a leg (HIP_Y - y) / HIP_Y with HIP_Y = 1.0 m.
  V = where U > 0, the leg code (0.125 FL, 0.375 FR, 0.625 BL, 0.875 BR; left = runtime +x). Where
      U = 0, the head weight along the axis from NECK_PIVOT to the muzzle tip, 0 on the plane through
      NECK_BASE and 1 at the tip; the ears take their root's value.
The same numbers ride in the root node's extras (`animal`, a JSON string)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _animals as an  # noqa: E402
import _fauna as fa  # noqa: E402

PIECE = "animal_cow_c"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
AO_DIST = 0.35
HIP_Y = 1.0
NECK_PIVOT, NECK_BASE = (0.35, 0.98), (0.82, 1.20)

# animal_cow's stations (animal_cow.py SPEC, unchanged).
SPEC = {
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
# Per LOD: part -> (station indices, ring sides). animal_cow's, LOD2 included: the head on four
# stations [0, 2, 5, 7] and three-sided legs.
LODS = {
    0: {"body": (range(10), 18), "neck": (range(4), 12), "head": (range(8), 12),
        "front_leg": (range(8), 8), "back_leg": (range(8), 8), "ear": (range(4), 6), "tail": (range(8), 6)},
    1: {"body": ([0, 2, 3, 4, 6, 8, 9], 12), "neck": ([0, 2, 3], 8), "head": ([0, 2, 4, 5, 7], 8),
        "front_leg": ([0, 1, 3, 5, 6, 7], 6), "back_leg": ([0, 1, 3, 5, 6, 7], 6), "ear": ([0, 1, 3], 4),
        "tail": ([0, 2, 3, 5, 7], 4)},
    2: {"body": ([0, 2, 5, 7, 9], 8), "neck": ([0, 3], 5), "head": ([0, 2, 5, 7], 5),
        "front_leg": ([0, 2, 6, 7], 3), "back_leg": ([0, 2, 6, 7], 3)},
}
GAIT = {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5}
# animal_cow's triangles per LOD: the ceiling for this variant (brief: same pose and size).
COW_TRIS = (1058, 468, 138)

C = {k: fa.hexc(v) for k, v in {
    "white": "#ebe6dc", "white_shade": "#d9d2c6", "red": "#7b351c", "red_dark": "#5f2611",
    "muzzle": "#c2b2aa", "nostril": "#4d3c38", "hoof": "#3d3733", "eye": "#16100c", "eye_ring": "#3a2418"}.items()}

# The underline: white below this height (m) along the body (f, m), rising from the belly through the
# brisket at the chest front and up the throat to the jaw.
UNDER_F = [-0.95, -0.62, -0.30, 0.20, 0.45, 0.70, 0.95, 1.15]
UNDER_Z = [0.70, 0.80, 0.77, 0.77, 0.80, 0.93, 1.12, 1.28]
DEWLAP_X = 0.11  # half width (m) of the underline ahead of the forelegs (brisket and dewlap)
# The crest: white over the top of the neck behind the poll, joining the white poll ((x, f, z) centre,
# radii).
CREST = ((0.0, 1.04, 1.47), (0.17, 0.25, 0.14))
SOCK_Z = 0.42  # white below this height on every leg (the knees and hocks)


def paint(P, part):
    n = len(P)
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    warp = an.fbm(P, 0.22, 11) - 0.5
    head = part == an.PART["head"]
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    ears = np.isin(part, [an.PART["ear_l"], an.PART["ear_r"]])
    tail = part == an.PART["tail"]
    # White weight from position fields, so every edge runs on across the part junctions.
    zl = np.interp(f, UNDER_F, UNDER_Z)
    under = fa.smoothstep(0.012, -0.012, z - zl + 0.06 * warp)
    # Ahead of the forelegs the underline narrows to the brisket and the dewlap under the neck, so the
    # chest and neck sides stay red.
    front = fa.smoothstep(0.40, 0.55, f)
    narrow = fa.smoothstep(DEWLAP_X + 0.03, DEWLAP_X - 0.03, np.abs(x) + 0.05 * warp)
    under = under * (1.0 - front * (1.0 - narrow))
    crest = fa.smoothstep(-0.012, 0.012, an.ellipsoid_field(P, *CREST) + 0.25 * warp)
    ww = np.maximum(under, crest)
    # Legs: white socks below the knees and hocks; above them the legs stay red.
    ww[legs] = fa.smoothstep(0.012, -0.012, z[legs] - SOCK_Z + 0.06 * warp[legs])
    # Face: white from the poll to the muzzle, red on the cheeks behind the eyes and round each eye.
    hs, _, hx, hn = SPEC["head"].local(P[head])
    sf = hs / SPEC["head"].Lc
    wh = warp[head]
    # The head loft's first rings (sf < 0.15) form the cap seen from above behind the poll, so the
    # cheek starts past them and the poll stays white into the crest.
    cheek = fa.smoothstep(0.34, 0.27, sf + 0.06 * wh) * fa.smoothstep(0.12, 0.18, sf) \
        * fa.smoothstep(0.55, 0.68, np.abs(hx)) * fa.smoothstep(0.12, -0.02, hn) \
        * fa.smoothstep(-0.62, -0.45, hn)
    hw = 1.0 - cheek
    Ph = P[head]
    for s in (1, -1):
        eye = SPEC["head"].point(2, np.pi / 2 * s - 0.45 * s)
        d = np.linalg.norm(Ph - eye, axis=1)
        hw = np.minimum(hw, fa.smoothstep(0.050, 0.060, d + 0.03 * wh))
    ww[head] = hw
    ww[ears] = 0.0
    sw = tail & (z < 0.76)
    ww[sw] = 1.0
    tone = an.fbm(P, 0.5, 5)
    wb = 1.0 - ww
    rgb = fa.mix(fa.mix(C["white"], C["white_shade"], 0.6 * tone),
                 fa.mix(C["red"], C["red_dark"], 0.45 * tone), wb)
    rough = (0.62 - 0.04 * wb).astype(np.float32)
    height = 0.0012 * (an.fbm(P, 0.12, 3) - 0.5)
    # Muzzle, nostrils and eyes on the head (animal_cow's).
    hrgb, hr, hh = rgb[head], rough[head], height[head]
    muz = sf > 0.80 + 0.03 * warp[head]
    hrgb[muz] = C["muzzle"]
    hr[muz] = 0.48
    for s in (1, -1):
        nos = SPEC["head"].point(6, 0.55 * s)
        d = np.linalg.norm((Ph - nos) / np.array([0.016, 0.02, 0.014]), axis=1)
        hrgb[d < 1] = C["nostril"]
        hh[d < 1] -= 0.002 * (1 - d[d < 1] ** 2)
        eye = SPEC["head"].point(2, np.pi / 2 * s - 0.45 * s)
        d = np.linalg.norm(Ph - eye, axis=1)
        hrgb[d < 0.03] = C["eye_ring"]
        hrgb[d < 0.022] = C["eye"]
        hr[d < 0.022] = 0.15
        hh[d < 0.03] += 0.004 * np.clip(1 - (d[d < 0.03] / 0.03) ** 2, 0, 1)
    rgb[head], rough[head], height[head] = hrgb, hr, hh
    hoof = legs & (z < 0.072)
    rgb[hoof] = C["hoof"]
    rough[hoof] = 0.45
    height[sw] += 0.002 * (an.fbm(P[sw] * np.array([1, 1, 0.2]), 0.02, 7) - 0.5)
    up = np.clip((z - 1.15) / 0.3, 0, 1)
    snow = np.where(legs, 0.05, 0.08 + 0.6 * up).astype(np.float32)
    wet = np.full(n, 0.25, np.float32)
    return {"rgb": np.clip(rgb, 0, 1).astype(np.float32), "rough": rough, "metal": np.zeros(n, np.float32),
            "height": height.astype(np.float32), "snow": snow, "wet": wet}


an.check_buried(SPEC, LODS)
mat = fa.setup(PIECE)
atlas = an.make_atlas(SPEC)
motion = an.Motion(HIP_Y, NECK_PIVOT, NECK_BASE, SPEC["head"].c[-1][1:], SPEC["ear"].c[0])
lods = [an.build_lod(PIECE, i, SPEC, LODS, atlas, motion, mat) for i in range(3)]
tris = [len(o.data.polygons) for o in lods]
print(f"{PIECE}: tris {tris}, animal_cow {COW_TRIS}")
assert all(t <= c for t, c in zip(tris, COW_TRIS)), (tris, COW_TRIS)
texel = an.density(lods[0])
for o in lods[1:]:
    o.hide_render = True
stats = an.paint_maps(lods[0], SPEC, paint, out, PIECE, skip_bake, AO_DIST, atlas.sig)
for o in lods[1:]:
    o.hide_render = False
extras = an.make_extras(SPEC, motion, {"withersM": 1.45, "muzzleToRumpM": 2.325}, GAIT)
extras["coat"] = "animal_cow in a Hereford-like coat: red-brown with a white face, crest, underline and socks"
an.export_piece(PIECE, lods, out, __file__, extras, texel, atlas.sig, stats, skip_bake, AO_DIST, motion)
