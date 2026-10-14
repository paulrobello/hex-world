"""Coast family (ocean, later beach): water-bed levels, seabed relief, pebble and rock scatter.
Levels follow the water bed contract (docs/kit/water-contract.md, DRAFT W3c), runtime metres:
meadow ring y = 0, water surface y = -1.4, ocean bed y = -4.0 (the tile base).

Blender frame (x, y, z) exports as runtime (x, z, -y), so a runtime height is a Blender z here.
ocean_base bakes the pebble and rock contact shadows that ocean_center places, so both pieces
build from this one module and any edit here rebuilds both."""
import math
import random

import bmesh
import bpy

from lib import hexgeo, scene

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

# Stone slot colours (linear), mirrored in assets/kit/manifest.json. The seabed slot tints the shared
# `gravel` set there instead (no new texture binding: kit forward is at 16 sampled textures next to
# refracting water).
PEBBLE_COLOR = (0.105, 0.095, 0.08)
ROCK_COLOR = (0.17, 0.16, 0.14)


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


def scatter():
    """Deterministic seabed detail: [(kind, x, y, width, height, yaw, seed)], kind 'pebble' or 'rock'.
    Pebbles lie in loose clusters with a few strays (refs/ocean ref-1); every item keeps 1.5 m from
    the border so nothing crosses a seam."""
    rng = random.Random("coast-scatter")
    items = []

    def ok(x, y, w):
        return hexgeo.dist_to_border(x, y) >= 1.5 + w and all(
            math.hypot(x - i[1], y - i[2]) > 0.5 * (w + i[3]) + 0.04 for i in items)

    for _ in range(6):  # rocks
        for _try in range(50):
            a, r = rng.uniform(0, 2 * math.pi), rng.uniform(2.0, 10.5)
            x, y, w = r * math.cos(a), r * math.sin(a), rng.uniform(0.55, 1.0)
            if ok(x, y, w):
                items.append(("rock", x, y, w, rng.uniform(0.12, 0.2), rng.uniform(0, 360), rng.random()))
                break
    centres = []
    for _ in range(7):  # pebble clusters
        a, r = rng.uniform(0, 2 * math.pi), rng.uniform(1.0, 10.0)
        centres.append((r * math.cos(a), r * math.sin(a), rng.uniform(0.6, 1.4)))
    for cx, cy, spread in centres:
        for _ in range(rng.randint(4, 7)):
            for _try in range(30):
                x, y = cx + rng.gauss(0, spread * 0.5), cy + rng.gauss(0, spread * 0.5)
                w = rng.uniform(0.15, 0.35)
                if ok(x, y, w):
                    items.append(("pebble", x, y, w, w * rng.uniform(0.4, 0.6), rng.uniform(0, 360), rng.random()))
                    break
    for _ in range(10):  # strays
        for _try in range(30):
            a, r = rng.uniform(0, 2 * math.pi), rng.uniform(1.0, 11.5)
            x, y, w = r * math.cos(a), r * math.sin(a), rng.uniform(0.12, 0.28)
            if ok(x, y, w):
                items.append(("pebble", x, y, w, w * rng.uniform(0.4, 0.6), rng.uniform(0, 360), rng.random()))
                break
    return items


def stone(name, item, subdiv, base_z=OCEAN_BED_Y):
    """One pebble or rock: a jittered, flattened icosphere sunk 30% of its height into the seabed.
    subdiv 1 = 20 tris, 2 = 80 tris. base_z is the bed level the relief sits on."""
    kind, x, y, w, h, yaw, seed = item
    rng = random.Random(f"{kind}-{seed:.12f}")
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=0.5)
    jit = 0.12 if kind == "rock" else 0.06
    for v in bm.verts:
        v.co *= 1.0 + rng.uniform(-jit, jit)
    yr = math.radians(yaw)
    ground = base_z + relief(x, y)
    # The top (0.7 h, plus jitter) stays 1 cm inside the envelope over the local relief.
    h = min(h, (BED_TOL_M - 0.01 - relief(x, y)) / (0.7 * (1 + jit)))
    zc = ground + 0.2 * h
    for v in bm.verts:
        px, py, pz = v.co.x * w, v.co.y * w * 0.8, v.co.z * h
        v.co.x = x + px * math.cos(yr) - py * math.sin(yr)
        v.co.y = y + px * math.sin(yr) + py * math.cos(yr)
        v.co.z = zc + pz
    top = max(v.co.z for v in bm.verts)
    assert top - base_z <= BED_TOL_M, (kind, x, y, top - base_z)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for p in obj.data.polygons:
        p.use_smooth = kind == "pebble"
    return obj
