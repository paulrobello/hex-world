"""Tileable 'asphalt' PBR set (D2: 1024² over 2 m = 512 px/m). Scripted; seamless via lib.materials.torus_coords.
Look: ref-1's worn two-lane road (assets-source/refs/road_straight): dark binder with grey aggregate,
lighter worn patches, sparse fine cracks. Roughness 0.6-0.85, mean sRGB value 0.12-0.25."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, export, materials, scene  # noqa: E402
from _shared import make_math, make_ramp  # noqa: E402

SET, SIZE, TILE_M = "asphalt", 1024, 2.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#2c2c2e"), 0.75)
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


# Aggregate: Voronoi cells about 1 cm across (5 px at 512 px/m) as stones in a dark binder. Stones
# stand proud (height) and vary from dark basalt to pale grey by cell colour.
stones = voronoi(110.0)
stone_mask = math("SUBTRACT", 1.0, smooth(stones.outputs["Distance"], 0.25, 0.55))
stone_col = ramp(stones.outputs["Color"], [(0.2, "#2a2a2b"), (0.6, "#4a4846"), (0.95, "#6e6a64")])
binder = ramp(noise(160.0, detail=3.0, rough=0.6), [(0.3, "#1c1c1e"), (0.7, "#2b2b2d")])
col = mix(math("MULTIPLY", stone_mask, 0.75), binder, stone_col)
# Wear: low-frequency lighter, dustier patches (ref-1's faded asphalt), and darker oily blotches.
wear = smooth(noise(4.5, detail=3.0, rough=0.55), 0.45, 0.7)
col = mix(math("MULTIPLY", wear, 0.35), col, "#6b655c")
oil = smooth(noise(5.5, detail=2.0, rough=0.5, distortion=0.4), 0.62, 0.75)
col = mix(math("MULTIPLY", oil, 0.4), col, "#161618")
# Fine cracks: Voronoi cell edges, kept only where a broad mask allows, so cracks are sparse runs.
cells = voronoi(7.0, feature="DISTANCE_TO_EDGE")
crack_line = math("SUBTRACT", 1.0, smooth(cells.outputs["Distance"], 0.0, 0.012))
crack_mask = smooth(noise(4.0, detail=2.0), 0.55, 0.62)
crack = math("MULTIPLY", crack_line, crack_mask)
col = mix(math("MULTIPLY", crack, 0.85), col, "#0e0e0f")
nt.links.new(col, bsdf.inputs["Base Color"])

height = math("ADD", math("MULTIPLY", stone_mask, 0.6), math("MULTIPLY", noise(40.0, detail=4.0), 0.25))
height = math("SUBTRACT", height, math("MULTIPLY", crack, 0.6))

# Roughness: worn, polished stone tops 0.62; binder 0.8; cracks and oil 0.85.
rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.82
rough.inputs["To Max"].default_value = 0.66
nt.links.new(math("ADD", math("MULTIPLY", stone_mask, 0.7), math("MULTIPLY", wear, 0.3)), rough.inputs["Value"])
rough_v = math("ADD", rough.outputs["Result"], math("MULTIPLY", math("ADD", crack, oil), 0.04))
nt.links.new(math("MINIMUM", rough_v, 0.85), bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.4
bump.inputs["Distance"].default_value = 0.01
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
