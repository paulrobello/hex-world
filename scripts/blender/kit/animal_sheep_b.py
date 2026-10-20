"""animal_sheep_b: a lamb, animal_sheep at about 60% of its size with the same black face, legs and
creamy fleece, a static farm prop (PRD §6 Variety; owner-approved references
assets-source/refs/farm_animals ref-2 sheep and ref-3 overhead flock; owner decisions A17: black face
and legs, creamy fleece, ref-2's smooth form). Scripted route; the scale and rest-paint maps are in
_animals_var.py, the builder in _animals.py (both reused unchanged).

animal_sheep's stations, palette and paint are copied here unchanged (animal_sheep.py is frozen with
the approved piece) and each part goes through its own scale map, in the proportions of a lamb: the
body and neck at BODY_S (0.58), the head and ears larger at HEAD_S (0.72), the legs relatively longer
at LEG_S (0.66 high, 0.60 apart, thinner than the body), the muzzle shortened by SNOUT. Every sample
paints at its rest position on animal_sheep, so the hood, the wool line on the legs, the eyes and
nose are the sheep's own, and the fleece lumps shrink with the lamb (the paint's relief is scaled by
BODY_S).

Real scale: about 0.51 m at the withers (the fleece top), 0.70 m from muzzle tip to rump. Faces runtime
+z, +y up, +x the lamb's left; pivot on the ground under the body centre. Kind `prop`, one slot
`animal_sheep_b_body` (unique 1024 px albedo, 512 px normal and ORM, 128 px masks).

Motion contract (TEXCOORD_1, Blender UV map `motion`, FLOAT VEC2 on every primitive of every LOD;
`_animals.check_motion` re-reads it from the GLB), as animal_sheep's at the lamb's size:
  U = leg weight: 0 off the legs; on a leg (HIP_Y - y) / HIP_Y with HIP_Y = 0.28 m.
  V = where U > 0, the leg code (0.125 FL, 0.375 FR, 0.625 BL, 0.875 BR; left = runtime +x).
      Where U = 0, the head weight: 0 on the body and tail, a position field along the axis from
      NECK_PIVOT to the muzzle tip, clamp((t - t_base) / (t_tip - t_base), 0, 1), t_base the largest
      t of any neck surface point inside the body. Ears take their root's value. A graze pitches
      V > 0 about the axis parallel to x through NECK_PIVOT, nose down, by theta x V; extras
      `grazeDegToGround` is the theta that brings the muzzle tip to 0.08 m.
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

PIECE = "animal_sheep_b"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
BODY_S, HEAD_S, LEG_S = 0.58, 0.72, (0.60, 0.60, 0.66)
SNOUT = 0.88  # the head's forward reach relative to HEAD_S: a lamb's shorter muzzle
LEG_W = 0.8  # leg half sizes: thinner than the body's scale would draw a sheep's leg
BODY_Z = 0.344  # the lamb's body centre height (animal_sheep's 0.505): belly about 0.17 m up
AO_DIST = 0.2 * BODY_S
HIP_Y = 0.28
SHEEP_NECK_PIVOT = (0.20, 0.50)

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

# Body, neck and tail scale about animal_sheep's body centre; the head and ears about its neck end,
# which lands where the body map puts it; the legs about their hooves on the ground.
BODY = av.scale(BODY_S, (0.0, 0.0, 0.505), (0.0, 0.0, BODY_Z))
HEAD = av.scale((HEAD_S, HEAD_S * SNOUT, HEAD_S), REST["neck"].c[-1], BODY(REST["neck"].c[-1]))
LEG = av.scale(LEG_S)
SPEC = {
    "body": av.moved(REST["body"], BODY, "fixed", BODY_S, BODY_S),
    "neck": av.moved(REST["neck"], BODY, "path", BODY_S, BODY_S),
    "head": av.moved(REST["head"], HEAD, "path", HEAD_S, HEAD_S),
    "front_leg": av.moved(REST["front_leg"], LEG, "fixed", LEG_W * LEG_S[0], LEG_W * LEG_S[2]),
    "back_leg": av.moved(REST["back_leg"], LEG, "fixed", LEG_W * LEG_S[0], LEG_W * LEG_S[2]),
    "ear": av.moved(REST["ear"], HEAD, "fixed", HEAD_S, HEAD_S),
    "tail": av.moved(REST["tail"], BODY, "path", BODY_S, BODY_S),
}
NECK_PIVOT = tuple(BODY((0.0, *SHEEP_NECK_PIVOT))[1:])

# Per LOD: animal_sheep's, with LOD2's head on four stations (three drew a beak, animal_cow friction
# note 6) paid for by three-sided legs beyond 400 m.
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


paint = av.RestPaint(paint_rest, {
    "body": BODY.inv, "neck": BODY.inv, "tail": BODY.inv, "head": HEAD.inv, "ear": av.mirror_x(HEAD.inv),
    "front_leg": av.mirror_x(LEG.inv), "back_leg": av.mirror_x(LEG.inv)}, height_scale=BODY_S)

NECK_BASE = av.neck_base(SPEC, NECK_PIVOT)
real = av.measures(SPEC)
print(f"{PIECE}: neck pivot {np.round(NECK_PIVOT, 3)}, neck base {np.round(NECK_BASE, 3)}, "
      f"muzzle tip {np.round(SPEC['head'].c[-1], 3)}, leg top z {SPEC['front_leg'].c[0][2]:.3f}")
motion = an.Motion(HIP_Y, NECK_PIVOT, NECK_BASE, SPEC["head"].c[-1][1:], SPEC["ear"].c[0])
av.build(PIECE, SPEC, LODS, motion, paint, out, skip_bake, AO_DIST, real, GAIT, __file__,
         extra={"pose": f"lamb: animal_sheep with body {BODY_S:g}, head {HEAD_S:g}, legs {LEG_S[2]:g} high"})
