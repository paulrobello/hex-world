"""Coast family (ocean): water-bed levels and seabed relief.
Levels follow the water bed contract (docs/kit/water-contract.md, DRAFT W3c), runtime metres:
meadow ring y = 0, water surface y = -1.4, ocean bed y = -4.0 (the tile base).

Blender frame (x, y, z) exports as runtime (x, z, -y), so a runtime height is a Blender z here.
The seabed rocks and pebbles are scatter props since 2026-10-26 (owner: "scatter type items like
rocks and reeds should not be built into the tile"; `seabed_rock`, `seabed_pebbles`, `_stones.py`),
so ocean_base bakes no stone contact shadows and this module serves ocean_base alone."""
import math

from lib import hexgeo

# Water surface at rest (contract §1, may move with the owner's look check).
SURFACE_Y = -1.4
# Water depth over the ocean bed. The one constant to change if the owner deepens the ocean.
OCEAN_DEPTH = 2.6
OCEAN_BED_Y = SURFACE_Y - OCEAN_DEPTH
# The ocean bed is the column floor: the engine's cut face covers the exposed edge from the surface
# down to the bed, so the ocean column has no strata skirt (contract §4). A deeper ocean moves the
# column floor with it, and with it kit_gate's base skirt-bottom (-LEVEL_STEP) and ring-level checks.
assert abs(OCEAN_BED_Y + hexgeo.LEVEL_STEP) < 1e-9, "ocean bed must sit on the tile base today"

# Contract §4 (176f1e7): the ocean floor rises at most 0.10 m above the bed (all upward, never below
# it) and is 0 within 2 m of the border, so the water stays >= 2.5 m deep and waves stay at full
# height. The engine computes depth from the flat profile and never reads this geometry, so the bed
# must sit on the profile: `bed_profile_errors` checks every exported LOD against it.
BED_TOL_M = 0.10
FALLOFF_M = 2.0
RELIEF_MAX = 0.06  # low-poly swell
RIPPLE_AMP = 0.03  # high-poly sand ripples (AO bake source only); relief + ripples <= 0.09
RIPPLE_WAVELENGTH = 0.85


def falloff(x, y):
    t = min(max(hexgeo.dist_to_border(x, y) / FALLOFF_M, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def swell(x, y):
    """Long, low sand swells in [0, 1]: two crossed waves of 14-22 m, no feature near a seam."""
    s = 0.55 * math.sin(0.29 * x + 0.8) * math.cos(0.33 * y - 1.1) + 0.45 * math.sin(0.41 * x + 0.37 * y + 2.3)
    return 0.5 + 0.5 * s


def relief(x, y):
    """Seabed height above the bed (m), in [0, RELIEF_MAX]: 0 with zero slope at the border."""
    return RELIEF_MAX * falloff(x, y) * swell(x, y)


def bed_profile_errors(obj, bed_z=OCEAN_BED_Y, tol=BED_TOL_M):
    """Vertices of a mesh off the ocean bed profile (contract §4): border-ring vertices exactly at
    bed_z (1e-4 m), every other vertex in [bed_z, bed_z + tol]. Returns a list of messages."""
    errs = []
    for v in obj.data.vertices:
        x, y, z = v.co
        if abs(hexgeo.dist_to_border(x, y)) < 1e-4:
            if abs(z - bed_z) > 1e-4:
                errs.append(f"{obj.name}: border vertex ({x:.3f}, {y:.3f}) at {z:.4f}, profile {bed_z}")
        elif not (bed_z - 1e-6 <= z <= bed_z + tol):
            errs.append(f"{obj.name}: vertex ({x:.3f}, {y:.3f}) at {z:.4f}, outside [{bed_z}, {bed_z + tol}]")
    return errs


def ripples(x, y):
    """Relief plus wave-formed sand ripples whose crests wander with the swell (high-poly only)."""
    a = 0.35 * math.sin(0.12 * x - 0.09 * y)  # crest direction drifts across the tile
    u = x * math.cos(0.5 + a) + y * math.sin(0.5 + a)
    phase = 2 * math.pi * u / RIPPLE_WAVELENGTH + 1.4 * math.sin(0.23 * y + 0.4)
    r = RIPPLE_AMP * falloff(x, y) * (0.55 + 0.45 * swell(x, y)) * (0.5 + 0.5 * math.sin(phase))
    return relief(x, y) + r

