"""Beach driftwood (refs: assets-source/refs/driftwood, approved 2026-10-20, BD-A1): barkless,
sea-bleached wood lying on the beach's dry sand, a scatter prop of the beach tiles. ref-1 gives the
look: silver-grey wood with beige streaks, a twisted spiral grain, cracks along the grain, a broken
branch stub and a small root flare; ref-2 two logs and a forked branch half sunk in the sand.

The shape is log_fallen's parametric log (`_log.Log` and `_log.build`: body with root flare, a
solid snapped end, stubs, the metric atlas that every LOD samples exactly, the Cycles bake, the
512² normal and ORM and the body-box collider). Only the high-poly materials differ: `_log.py`
backs three approved logs, so it is not edited, and this module sets its two material factories
(`bark_high_material`, `wood_high_material`) to the bleached ones at import time. The bark map's
moss channel bakes to 0 here (no emission), so masks.G is `_log`'s plain-wood porosity 0.4.

Bleached surface (on the high poly's metric `bark` UV: U = metres around / 2, V = metres along / 2):
a fine grain in bands along a helix that turns TWIST metres around per metre along, beige streaks
stretched along the same helix, and sparse dark cracks on it, broken along their length. The bump
from the grain and the crack grooves bakes into the tangent normal. The end grain is pale grey
rings with radial checks."""
from __future__ import annotations

import math

import bpy

from lib import materials

import _log

TWIST = 0.45  # metres around per metre along: a spiral grain about 24 degrees off the axis
SILVER, GREY, BEIGE, CRACK = "#aca99f", "#86827a", "#b9a687", "#5a5248"


def _coords(N, L, math_):
    """(w, x): metres across the spiral grain and along the log, from the high poly's `bark` UV."""
    uvn = N.new("ShaderNodeUVMap")
    uvn.uv_map = "bark"
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(uvn.outputs["UV"], sep.inputs[0])
    a = math_("MULTIPLY", sep.outputs["X"], 2.0)
    x = math_("MULTIPLY", sep.outputs["Y"], 2.0)
    w = math_("SUBTRACT", a, math_("MULTIPLY", x, TWIST))
    return w, x


def _vec(N, L, xs, ys, zs=0.0):
    c = N.new("ShaderNodeCombineXYZ")
    for i, v in enumerate((xs, ys, zs)):
        if isinstance(v, float):
            c.inputs[i].default_value = v
        else:
            L.new(v, c.inputs[i])
    return c.outputs["Vector"]


def bleached_high_material(piece: str, name: str, _moss=None):
    """The driftwood surface (replaces `_log.bark_high_material`; `_moss` is ignored)."""
    m = bpy.data.materials.new(name)
    nt, N, L, mix, rng_, math_ = _log._nodes(m)
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    w, x = _coords(N, L, math_)
    # Fine grain: bands across w (period about 1.6 cm), wavering by the wave's own distortion.
    grain = N.new("ShaderNodeTexWave")
    grain.wave_type, grain.bands_direction = "BANDS", "X"
    grain.inputs["Scale"].default_value = 20.0
    grain.inputs["Distortion"].default_value = 3.0
    grain.inputs["Detail"].default_value = 3.0
    L.new(_vec(N, L, w, math_("MULTIPLY", x, 0.15)), grain.inputs["Vector"])
    # Cracks: every ~13 cm across the grain, only the crest of the band, broken along the log.
    crack_w = N.new("ShaderNodeTexWave")
    crack_w.wave_type, crack_w.bands_direction = "BANDS", "X"
    crack_w.inputs["Scale"].default_value = 2.4
    crack_w.inputs["Distortion"].default_value = 1.2
    crack_w.inputs["Detail"].default_value = 2.0
    L.new(_vec(N, L, w, math_("MULTIPLY", x, 0.05), 3.7), crack_w.inputs["Vector"])
    breaks = N.new("ShaderNodeTexNoise")
    breaks.inputs["Scale"].default_value, breaks.inputs["Detail"].default_value = 2.2, 2.0
    L.new(_vec(N, L, math_("MULTIPLY", w, 4.0), x, 1.3), breaks.inputs["Vector"])
    crack = math_("MULTIPLY", rng_(crack_w.outputs["Fac"], 0.975, 0.997), rng_(breaks.outputs["Fac"], 0.52, 0.62))
    # Beige streaks along the helix and a low tone variation.
    streak = N.new("ShaderNodeTexNoise")
    streak.inputs["Scale"].default_value, streak.inputs["Detail"].default_value = 3.0, 3.0
    L.new(_vec(N, L, math_("MULTIPLY", w, 7.0), math_("MULTIPLY", x, 0.5), 7.1), streak.inputs["Vector"])
    tone = N.new("ShaderNodeTexNoise")
    tone.inputs["Scale"].default_value, tone.inputs["Detail"].default_value = 1.4, 2.0
    L.new(_vec(N, L, w, x, -4.2), tone.inputs["Vector"])
    col = mix(rng_(tone.outputs["Fac"], 0.35, 0.65), (*materials.srgb(GREY)[:3], 1.0), (*materials.srgb(SILVER)[:3], 1.0))
    col = mix(math_("MULTIPLY", rng_(grain.outputs["Fac"], 0.2, 0.9), 0.32), col, (*materials.srgb(GREY)[:3], 1.0))
    col = mix(math_("MULTIPLY", rng_(streak.outputs["Fac"], 0.55, 0.72), 0.8), col, (*materials.srgb(BEIGE)[:3], 1.0))
    col = mix(crack, col, (*materials.srgb(CRACK)[:3], 1.0))
    L.new(col, bsdf.inputs["Base Color"])
    L.new(math_("ADD", 0.84, math_("MULTIPLY", crack, 0.12)), bsdf.inputs["Roughness"])
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.55
    bump.inputs["Distance"].default_value = 0.008
    L.new(math_("SUBTRACT", math_("MULTIPLY", grain.outputs["Fac"], 0.35), crack), bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Emission Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


def bleached_end_material(name: str, _spec=None):
    """The snapped ends and stub tips (replaces `_log.wood_high_material`): pale grey growth rings
    with a few radial checks."""
    m = bpy.data.materials.new(name)
    nt, N, L, mix, rng_, math_ = _log._nodes(m)
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = N.new("ShaderNodeTexCoord")
    rings = N.new("ShaderNodeTexWave")
    rings.wave_type = "RINGS"
    rings.inputs["Scale"].default_value = 9.0
    rings.inputs["Distortion"].default_value = 4.0
    L.new(tc.outputs["Object"], rings.inputs["Vector"])
    checks = N.new("ShaderNodeTexVoronoi")
    checks.feature = "DISTANCE_TO_EDGE"
    checks.inputs["Scale"].default_value = 14.0
    L.new(tc.outputs["Object"], checks.inputs["Vector"])
    check = rng_(checks.outputs["Distance"], 0.03, 0.0)
    col = mix(rng_(rings.outputs["Fac"], 0.2, 0.8), (*materials.srgb("#857c70")[:3], 1.0),
              (*materials.srgb("#ada393")[:3], 1.0))
    col = mix(math_("MULTIPLY", check, 0.7), col, (*materials.srgb(CRACK)[:3], 1.0))
    L.new(col, bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    bump = N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.35
    bump.inputs["Distance"].default_value = 0.005
    L.new(math_("SUBTRACT", rings.outputs["Fac"], check), bump.inputs["Height"])
    L.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Emission Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 0.0
    return m


_log.bark_high_material = bleached_high_material
_log.wood_high_material = bleached_end_material

Spec, build = _log.Spec, _log.build
D = math.radians
