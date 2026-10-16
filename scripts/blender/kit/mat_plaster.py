"""Tileable 'plaster' PBR set (D2: 1024² over 2 m = 512 px/m). Scripted; seamless via lib.materials.torus_coords.
Look: ref-2's hand-trowelled lime render (assets-source/refs/house_cluster): a pale, near-neutral
cream with soft trowel undulation, fine grit, faint weather staining and small pits. It is kept
light and close to neutral on purpose: the house slots tint it with a manifest `color` factor
(cream walls, terracotta and slate roofs, grey stone chimneys and plinths), so one set serves
every isotropic house surface in world space. Roughness 0.8-0.95."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, export, materials, scene  # noqa: E402
from _shared import make_math, make_ramp  # noqa: E402

SET, SIZE, TILE_M = "plaster", 1024, 2.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#ddd6c8"), 0.88)
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


# The torus coordinates run 2*pi*2 = 12.6 units per 2 m tile. Trowel undulation: soft, distorted
# noise at the lowest scale that does not mirror the tile (>= 4), carried into colour and height.
trowel = noise(4.5, detail=3.0, rough=0.55, distortion=0.6)
mottle = noise(4.0, detail=4.0, rough=0.6, distortion=0.3)
grit = noise(60.0, detail=2.0, rough=0.7)
col = ramp(mottle, [(0.3, "#cfc6b4"), (0.55, "#ddd5c5"), (0.75, "#e8e2d5")])
col = mix(math("MULTIPLY", smooth(trowel, 0.55, 0.8), 0.35), col, "#efeae0")
# Faint weather staining in broad soft patches, and fine grit speckle.
stain = smooth(noise(5.5, detail=3.0, rough=0.5, distortion=0.8), 0.6, 0.8)
col = mix(math("MULTIPLY", stain, 0.3), col, "#a89e8c")
col = mix(math("MULTIPLY", smooth(grit, 0.6, 0.75), 0.25), col, "#b8af9e")
# Small pits: sparse Voronoi cells, darker and recessed.
pits = voronoi(10.0)
pit = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(pits.outputs["Distance"], 0.08, 0.2)),
           smooth(pits.outputs["Color"], 0.8, 0.82))
col = mix(math("MULTIPLY", pit, 0.5), col, "#8f8676")
nt.links.new(col, bsdf.inputs["Base Color"])

height = math("ADD", math("MULTIPLY", trowel, 0.7), math("MULTIPLY", grit, 0.15))
height = math("SUBTRACT", height, math("MULTIPLY", pit, 0.4))
rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.95
rough.inputs["To Max"].default_value = 0.8
nt.links.new(trowel, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.35
bump.inputs["Distance"].default_value = 0.01
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
