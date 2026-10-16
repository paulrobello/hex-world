"""Tileable 'grass' PBR set (D2: 1024² over 2 m). Scripted; seamless via lib.materials.torus_coords.
Look: ref-2's dense, clumpy meadow grass (assets-source/refs/meadow_base, owner ruling B1)."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, export, materials, scene  # noqa: E402
from _shared import make_math, make_ramp  # noqa: E402

SET, SIZE, TILE_M = "grass", 1024, 2.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#4f7d32"), 0.85)
nt = mat.node_tree
math = make_math(nt)
ramp = make_ramp(nt)
bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
vec, w = materials.torus_coords(nt, scale=TILE_M)


# Every texture is 4D and fed (vec, w) only: 3D textures ignore W and mirror the tile (no wrap).
def noise(scale, detail=4.0, rough=0.5, distortion=0.0):
    n = nt.nodes.new("ShaderNodeTexNoise")
    n.noise_dimensions = "4D"
    n.inputs["Scale"].default_value = scale
    n.inputs["Detail"].default_value = detail
    n.inputs["Roughness"].default_value = rough
    n.inputs["Distortion"].default_value = distortion
    nt.links.new(vec, n.inputs["Vector"])
    nt.links.new(w, n.inputs["W"])
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


# Clumps (ref-2): low-frequency density, with tufts as Voronoi cells whose centres stand tall.
clump = noise(4.0, detail=3.0, rough=0.55)
tuft = voronoi(22.0)
tuft_h = math("SUBTRACT", 1.0, smooth(tuft.outputs["Distance"], 0.0, 1.0))
# Blades: fine streaky noise (high distortion reads as crossing blades at 512 px/m).
blades = noise(55.0, detail=3.0, rough=0.6, distortion=3.0)
blade_h = math("ADD", math("MULTIPLY", smooth(blades, 0.4, 0.62), 0.7),
               math("MULTIPLY", smooth(noise(140.0, detail=2.0, distortion=2.0), 0.35, 0.65), 0.3))
height = math("ADD", math("MULTIPLY", tuft_h, 0.1), math("MULTIPLY", blade_h, 0.6))
height = math("ADD", height, math("MULTIPLY", clump, 0.3))

# Colour: shadowed base -> lit blade tops, dry-tip patches, clover patches, sparse flowers.
base = ramp(height, [(0.15, "#2b4a1a"), (0.45, "#4b7a2c"), (0.75, "#79a23f"), (0.95, "#9cb956")])
dry_mask = math("MULTIPLY", smooth(noise(2.5, detail=2.0), 0.5, 0.62), smooth(blades, 0.5, 0.65))
col = mix(dry_mask, base, "#a3a45c")
clover_cells = voronoi(14.0)
clover_mask = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(clover_cells.outputs["Distance"], 0.12, 0.3)),
                   smooth(noise(3.0, detail=1.0), 0.52, 0.56))
col = mix(clover_mask, col, ramp(noise(30.0, detail=2.0), [(0.3, "#2f5a2c"), (0.7, "#4c7f3f")]))
flower_cells = voronoi(26.0)
flower_mask = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(flower_cells.outputs["Distance"], 0.035, 0.06)),
                   smooth(flower_cells.outputs["Color"], 0.75, 0.76))
flower_col = ramp(noise(5.0, detail=1.0), [(0.45, "#f2efe0"), (0.55, "#e8c93a")])
col = mix(flower_mask, col, flower_col)
nt.links.new(col, bsdf.inputs["Base Color"])

rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.92
rough.inputs["To Max"].default_value = 0.78
nt.links.new(height, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.5
bump.inputs["Distance"].default_value = 0.02
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
