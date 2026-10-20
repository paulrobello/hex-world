"""animal_sheep: a black-faced sheep with a creamy fleece, a static farm prop (PRD P3; owner-approved
references assets-source/refs/farm_animals ref-2 sheep and ref-3 overhead flock; owner decisions A17:
black face and legs, creamy fleece, ref-2's smooth sculpted form as the low-poly target, no fur detail in
geometry). Scripted route; the builder, atlas and map drawing are in _animals.py.

Real scale: 0.78 m at the withers (the fleece top), 1.21 m from muzzle tip to rump. Faces runtime +z,
+y up, +x the sheep's left; pivot on the ground under the body centre. Kind `prop`, one slot
`animal_sheep_body` (unique 1024 px albedo, 512 px normal and ORM, 128 px masks). The fleece is one
smooth barrel; its wool reads from soft lumps in the normal map and tone in the albedo only.

Motion contract for a later walk and graze vertex shader, TEXCOORD_1 (Blender UV map `motion`, FLOAT
VEC2 on every primitive of every LOD; `_animals.check_motion` re-reads it from the GLB):
  U = leg weight: 0 everywhere off the legs. On a leg U = (HIP_Y - y) / HIP_Y with HIP_Y = 0.42 m, so 0
      at the hip pivot height and 1 at the hoof (y = 0). Every leg vertex, the buried top ring
      included, sits below HIP_Y, so U > 0 exactly on the legs.
  V = where U > 0, the leg code: 0.125 front-left, 0.375 front-right, 0.625 back-left, 0.875
      back-right (left = runtime +x). A walk swings each leg about the axis parallel to x through its
      hip (extras `hips`) by swing(t + gaitPhase[leg]); the diagonal pairs share a phase (walk).
      Where U = 0, V = the head weight: 0 on the body, the tail and the neck root, rising along the
      neck to 1 at the muzzle tip. It is a position field: V = clamp((t - t_base) / (t_tip - t_base),
      0, 1), t the projection on the axis from NECK_PIVOT to the muzzle tip, t_base that of NECK_BASE
      (where the neck leaves the fleece). The ears take the value at their root, so they ride the head
      rigidly. A graze pitches V > 0 about the axis parallel to x through NECK_PIVOT, nose down, by
      theta x V; extras `grazeDegToGround` is the theta that brings the muzzle tip to 0.08 m.
The same numbers ride in the root node's extras (`animal`, a JSON string)."""
import os
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _animals as an  # noqa: E402
import _fauna as fa  # noqa: E402

PIECE = "animal_sheep"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
AO_DIST = 0.2
HIP_Y = 0.42
NECK_PIVOT, NECK_BASE = (0.20, 0.50), (0.42, 0.60)

SPEC = {
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
# Per LOD: part -> (station indices, ring sides). LOD2 draws the head on four stations (three,
# [0, 2, 7], drew a beak) and pays for it with three-sided legs, as animal_sheep_b and _c do.
LODS = {
    0: {"body": (range(9), 16), "neck": (range(4), 10), "head": (range(8), 10),
        "front_leg": (range(6), 7), "back_leg": (range(6), 7), "ear": (range(4), 5), "tail": (range(4), 6)},
    1: {"body": ([0, 1, 3, 4, 5, 7, 8], 10), "neck": ([0, 2, 3], 7), "head": ([0, 2, 4, 5, 7], 7),
        "front_leg": ([0, 2, 4, 5], 5), "back_leg": ([0, 2, 4, 5], 5), "ear": ([0, 1, 3], 4),
        "tail": ([0, 2, 3], 4)},
    2: {"body": ([0, 2, 4, 6, 8], 7), "neck": ([0, 3], 5), "head": ([0, 2, 5, 7], 5),
        "front_leg": ([0, 4, 5], 3), "back_leg": ([0, 4, 5], 3)},
}
GAIT = {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5}

C = {k: fa.hexc(v) for k, v in {
    "wool": "#e8dcc0", "wool_shade": "#cbbb98", "wool_light": "#f2ead6", "face": "#2a2624",
    "face_light": "#3a3431", "hoof": "#1e1b1a", "ear_in": "#5a463e", "eye": "#0a0808", "eye_shine": "#3c2c22",
    "nose": "#1a1615"}.items()}


def paint(P, part):
    n = len(P)
    x, f, z = P[:, 0], P[:, 1], P[:, 2]
    head = part == an.PART["head"]
    legs = np.isin(part, [an.PART[k] for k in ("leg_fl", "leg_fr", "leg_bl", "leg_br")])
    ears = np.isin(part, [an.PART["ear_l"], an.PART["ear_r"]])
    # Wool: soft lumps (normal map) and warm tone variation; the fleece is the body, neck, tail, the
    # leg tops (the thighs) and the hood on the back of the head.
    lump = an.fbm(P, 0.07, 21)
    tone = an.fbm(P, 0.25, 4)
    wool = fa.mix(fa.mix(C["wool"], C["wool_light"], fa.smoothstep(0.55, 0.8, lump)), C["wool_shade"],
                  0.55 * fa.smoothstep(0.55, 0.15, tone))
    # Fleece darkens slightly toward the belly.
    wool = fa.mix(wool, C["wool_shade"], 0.35 * fa.smoothstep(0.36, 0.22, z))
    height = 0.006 * lump + 0.002 * an.fbm(P, 0.025, 9)
    rough = np.full(n, 0.72, np.float32)
    # Face and legs: black, with a soft wool line.
    wl = np.zeros(n)  # wool weight
    wl[~(head | legs | ears)] = 1.0
    hs, _, hx, hn = SPEC["head"].local(P[head])
    sf = hs / SPEC["head"].Lc
    warp = an.fbm(P[head], 0.05, 13) - 0.5
    # Hood: the poll and the back of the head carry wool down to the ears.
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
        eye = SPEC["head"].point(2, np.pi / 2 * s - 0.5 * s)
        d = np.linalg.norm(Ph - eye, axis=1)
        hrgb[d < 0.016] = C["eye_shine"]
        hrgb[d < 0.012] = C["eye"]
        hr[d < 0.016] = 0.15
        hh[d < 0.016] += 0.002 * np.clip(1 - (d[d < 0.016] / 0.016) ** 2, 0, 1)
        nos = SPEC["head"].point(6, 0.6 * s)
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


an.check_buried(SPEC, LODS)
mat = fa.setup(PIECE)
atlas = an.make_atlas(SPEC)
motion = an.Motion(HIP_Y, NECK_PIVOT, NECK_BASE, SPEC["head"].c[-1][1:], SPEC["ear"].c[0])
lods = [an.build_lod(PIECE, i, SPEC, LODS, atlas, motion, mat) for i in range(3)]
texel = an.density(lods[0])
for o in lods[1:]:
    o.hide_render = True
stats = an.paint_maps(lods[0], SPEC, paint, out, PIECE, skip_bake, AO_DIST, atlas.sig)
for o in lods[1:]:
    o.hide_render = False
extras = an.make_extras(SPEC, motion, {"withersM": 0.78, "muzzleToRumpM": 1.185}, GAIT)
an.export_piece(PIECE, lods, out, __file__, extras, texel, atlas.sig, stats, skip_bake, AO_DIST, motion)
