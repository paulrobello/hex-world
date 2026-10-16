"""Tileable 'soil' PBR set: 1024² over 4 m = 256 px/m (owner-approved D2 exception, 2026-10-13: one profile per 4 m wall). Scripted; seamless via lib.materials.torus_coords.
Look: the owner's O4 "cutaway" (2026-10-13, replacing ruling B1's dark topsoil over red clay over gravel): a grass
lip over root-dark topsoil, a warm brown body that darkens softly into a near-black foot, no bright strata bands.
Option 4 of mat_soil_alt.py (the comparison record, docs/checkpoints/skirt-options/), inlined here so this file is
the whole generator: it imports no `_*.py` helper, so provenance hashes exactly what it ran."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export, materials, scene  # noqa: E402

SET, SIZE, TILE_M = "soil", 1024, 4.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#3a2f27"), 0.9)
nt = mat.node_tree
bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
vec, w = materials.torus_coords(nt, scale=TILE_M)


def stretched(su, sv):
    """Torus coordinates scaled per axis: su < sv stretches features along the wall (U), su > sv along V."""
    vm = nt.nodes.new("ShaderNodeVectorMath")
    vm.operation = "MULTIPLY"
    vm.inputs[1].default_value = (su, su, sv)
    nt.links.new(vec, vm.inputs[0])
    return vm.outputs[0], math("MULTIPLY", w, sv)


# Every texture is 4D and fed torus coordinates only: 3D textures ignore W and mirror the tile (no wrap).
def noise(scale, detail=4.0, rough=0.5, distortion=0.0, coords=None):
    v4, w4 = coords or (vec, w)
    n = nt.nodes.new("ShaderNodeTexNoise")
    n.noise_dimensions = "4D"
    n.inputs["Scale"].default_value = scale
    n.inputs["Detail"].default_value = detail
    n.inputs["Roughness"].default_value = rough
    n.inputs["Distortion"].default_value = distortion
    nt.links.new(v4, n.inputs["Vector"])
    nt.links.new(w4, n.inputs["W"])
    return n.outputs["Fac"]


def voronoi(scale, feature="F1", randomness=1.0):
    n = nt.nodes.new("ShaderNodeTexVoronoi")
    n.voronoi_dimensions = "4D"
    n.feature = feature
    n.inputs["Scale"].default_value = scale
    n.inputs["Randomness"].default_value = randomness
    nt.links.new(vec, n.inputs["Vector"])
    nt.links.new(w, n.inputs["W"])
    return n


def math(op, a, b=None, clamp=False):
    n = nt.nodes.new("ShaderNodeMath")
    n.operation = op
    n.use_clamp = clamp
    for i, v in enumerate((a, b)):
        if v is None:
            continue
        if isinstance(v, float):
            n.inputs[i].default_value = v
        else:
            nt.links.new(v, n.inputs[i])
    return n.outputs[0]


def ramp(fac, stops):
    n = nt.nodes.new("ShaderNodeValToRGB")
    els = n.color_ramp.elements
    while len(els) > len(stops):
        els.remove(els[-1])
    while len(els) < len(stops):
        els.new(0.5)
    for el, (pos, col) in zip(els, stops):
        el.position = pos
        el.color = col if isinstance(col, tuple) else materials.srgb(col)
    nt.links.new(fac, n.inputs["Fac"])
    return n.outputs["Color"]


def mix(fac, a, b):
    n = nt.nodes.new("ShaderNodeMix")
    n.data_type = "RGBA"
    n.blend_type = "MIX"
    nt.links.new(fac, n.inputs["Factor"])
    for sock, v in ((n.inputs[6], a), (n.inputs[7], b)):
        if isinstance(v, str):
            sock.default_value = materials.srgb(v)
        else:
            nt.links.new(v, sock)
    return n.outputs[2]


def smooth(x, lo, hi):
    n = nt.nodes.new("ShaderNodeMapRange")
    n.interpolation_type = "SMOOTHSTEP"
    n.inputs["From Min"].default_value = lo
    n.inputs["From Max"].default_value = hi
    nt.links.new(x, n.inputs["Value"])
    return n.outputs["Result"]


# Depth below the rim, t in [0, 1): 0 at the cap rim, 1 at the skirt foot. Walls map V = -y / tileM, so
# with tileM 4 one tile spans the 4 m skirt and the rim and the foot share the wrap row (top PNG row = rim).
# V is recovered from the torus coordinates as atan2(w, z) / 2π, so t stays a function of (vec, w) only.
sep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(vec, sep.inputs[0])
t = math("FRACT", math("MULTIPLY", math("DIVIDE", math("ARCTAN2", w, sep.outputs["Z"]), 6.283185307179586), -1.0))

LIP_T = 0.03  # 12 cm grass lip under the rim


def grass_lip(col, height):
    """Grass hanging over the rim: blade-like lower edge (noise stretched along V), a root-dark line under it.
    Only the lower edge is jittered, so the rim row itself is always lip and the foot never is."""
    blades = noise(70.0, detail=3.0, rough=0.6, coords=stretched(1.0, 0.12))
    edge = math("ADD", LIP_T, math("MULTIPLY", math("SUBTRACT", blades, 0.5), 0.03))
    lip = math("SUBTRACT", 1.0, smooth(math("SUBTRACT", t, edge), -0.002, 0.002))
    roots = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(math("SUBTRACT", t, edge), 0.0, 0.025)), 0.7)
    col = mix(roots, col, "#1c1510")
    turf = ramp(noise(90.0, detail=4.0, rough=0.6), [(0.3, "#2f4a1b"), (0.7, "#4e722b")])
    col = mix(lip, col, turf)
    height = math("ADD", math("MULTIPLY", height, math("SUBTRACT", 1.0, lip)), math("MULTIPLY", lip, blades))
    return col, height


grain = noise(60.0, detail=6.0, rough=0.6)
mottle = noise(9.0, detail=3.0, rough=0.5)
smear = noise(14.0, detail=3.0, rough=0.5, coords=stretched(0.3, 1.0))

# Root-dark under the lip, warm brown body, soft darkening into the foot (wobbled, never a line).
body = ramp(math("ADD", math("MULTIPLY", mottle, 0.6), math("MULTIPLY", smear, 0.4)),
            [(0.3, "#47352a"), (0.7, "#5a4433")])
col = mix(math("MULTIPLY", math("SUBTRACT", grain, 0.5), 0.5), body, "#2a1f17")
top = math("SUBTRACT", 1.0, smooth(t, 0.04, 0.16))
col = mix(math("MULTIPLY", top, 0.55), col, "#2a1f18")
fall = smooth(math("ADD", t, math("MULTIPLY", math("SUBTRACT", noise(6.0, detail=2.0, coords=stretched(1.0, 0.05)), 0.5), 0.15)), 0.3, 1.0)
col = mix(math("MULTIPLY", fall, 0.85), col, "#181411")
stones = voronoi(40.0)
stone_mask = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(stones.outputs["Distance"], 0.10, 0.17)),
                  smooth(stones.outputs["Color"], 0.82, 0.83))
stone_mask = math("MULTIPLY", stone_mask, math("SUBTRACT", 1.0, fall))
col = mix(math("MULTIPLY", stone_mask, 0.6), col, "#6a5d50")
height = math("ADD", math("MULTIPLY", grain, 0.6), math("MULTIPLY", stone_mask, 0.4))
col, height = grass_lip(col, height)
nt.links.new(col, bsdf.inputs["Base Color"])

rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.95
rough.inputs["To Max"].default_value = 0.85
nt.links.new(height, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.45
bump.inputs["Distance"].default_value = 0.02
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
