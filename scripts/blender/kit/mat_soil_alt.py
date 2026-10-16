"""Skirt look options for the owner (scratch, not shipped): tileable sets soil_alt_<n>, n in 1-4, built as
drop-in replacements for the `soil` set (1024² over tileM 4.0, so the swap needs no manifest change).
  1 earth:    one earthy soil, organic mottling and faint horizontal smear, no bands.
  2 plinth:   dark matte neutral plinth with a thin grass lip, a diorama base edge.
  3 strata:   the round-2 icon's warm browns, two thin muted seams of uneven height and width, grass lip.
  4 cutaway:  grass lip over root-dark topsoil that grades softly into a dark base at the foot.
Usage: blender -b --factory-startup --python-exit-code 1 -P mat_soil_alt.py -- <out_dir> <n>
Helpers are copied from mat_soil.py on purpose: that generator is hashed by the approved soil set.
The owner chose option 4 on 2026-10-13: mat_soil.py now builds exactly that look as the shipped `soil` set."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, export, materials, scene  # noqa: E402
from _shared import make_math, make_ramp  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
out, OPTION = pathlib.Path(args[0]), int(args[1])
SET, SIZE, TILE_M = f"soil_alt_{OPTION}", 1024, 4.0
GENERATOR = pathlib.Path(__file__).resolve()
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#3a2f27"), 0.9)
nt = mat.node_tree
math = make_math(nt)
ramp = make_ramp(nt)
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
    return col, height, lip


grain = noise(60.0, detail=6.0, rough=0.6)
mottle = noise(9.0, detail=3.0, rough=0.5)
smear = noise(14.0, detail=3.0, rough=0.5, coords=stretched(0.3, 1.0))
bump_strength = 0.45

if OPTION == 1:
    # One earthy soil: mottling and a faint horizontal smear modulate a single brown, sparse small stones.
    base = ramp(math("ADD", math("MULTIPLY", mottle, 0.6), math("MULTIPLY", smear, 0.4)),
                [(0.3, "#3b2c21"), (0.7, "#57422f")])
    col = mix(math("MULTIPLY", math("SUBTRACT", grain, 0.5), 0.5), base, "#2a1f17")
    stones = voronoi(40.0)
    stone_mask = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(stones.outputs["Distance"], 0.10, 0.17)),
                      smooth(stones.outputs["Color"], 0.80, 0.81))
    col = mix(math("MULTIPLY", stone_mask, 0.7), col, ramp(stones.outputs["Color"], [(0.8, "#5f5348"), (1.0, "#75685a")]))
    height = math("ADD", math("MULTIPLY", grain, 0.6), math("MULTIPLY", stone_mask, 0.4))
elif OPTION == 2:
    # Plinth: near-flat dark warm grey, clearly above the void (#101112), a hint of grain only.
    col = ramp(math("ADD", math("MULTIPLY", grain, 0.5), math("MULTIPLY", mottle, 0.5)),
               [(0.3, "#2c2926"), (0.7, "#35312d")])
    height = math("MULTIPLY", grain, 0.3)
    bump_strength = 0.12
    col, height, lip = grass_lip(col, height)
elif OPTION == 3:
    # Icon round 2 (scripts/brand/author-b.mjs): soilA #8f6645, soilC #9a7752, soilB #7a553a, seams soilD #664834.
    # Toned down for the PBR wall (the icon is flat-shaded), body darkening toward the foot.
    body = ramp(math("ADD", math("MULTIPLY", mottle, 0.5), math("MULTIPLY", smear, 0.5)),
                [(0.3, "#6a4b33"), (0.7, "#7d5b40")])
    lower = ramp(math("ADD", math("MULTIPLY", mottle, 0.5), math("MULTIPLY", smear, 0.5)),
                 [(0.3, "#5a3f2b"), (0.7, "#6a4c35")])
    col = mix(smooth(t, 0.45, 0.75), body, lower)
    col = mix(math("MULTIPLY", math("SUBTRACT", grain, 0.5), 0.45), col, "#3e2c1f")
    wobble = noise(5.0, detail=0.0, coords=stretched(1.0, 0.05))
    for centre, width in ((0.36, 0.022), (0.66, 0.03)):
        c = math("ADD", centre, math("MULTIPLY", math("SUBTRACT", wobble, 0.5), 0.08))
        thick = math("MULTIPLY", width, math("ADD", 0.4, noise(7.0, detail=0.0, coords=stretched(1.0, 0.05))))
        d = math("ABSOLUTE", math("SUBTRACT", t, c))
        seam = math("SUBTRACT", 1.0, smooth(math("DIVIDE", d, thick), 0.2, 1.0))
        col = mix(math("MULTIPLY", seam, 0.6), col, "#4a3423")
    height = math("MULTIPLY", grain, 0.6)
    col, height, lip = grass_lip(col, height)
else:
    # Cutaway: root-dark under the lip, warm brown body, soft darkening into the foot (wobbled, never a line).
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
    col, height, lip = grass_lip(col, height)

nt.links.new(col, bsdf.inputs["Base Color"])

rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.95
rough.inputs["To Max"].default_value = 0.85
nt.links.new(height, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = bump_strength
bump.inputs["Distance"].default_value = 0.02
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, option=OPTION, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
