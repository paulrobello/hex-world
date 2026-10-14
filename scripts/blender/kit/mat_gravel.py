"""Tileable 'gravel' PBR set (D2: 1024² over 2 m = 512 px/m). Scripted; seamless via lib.materials.torus_coords.
Look: ref-1's pale limestone pea-gravel yard and driveway (assets-source/refs/house_cluster): rounded
pebbles 1-3 cm across in warm greys and buffs, packed over a darker fines bed. Roughness 0.78-0.92."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export, materials, scene  # noqa: E402

SET, SIZE, TILE_M = "gravel", 1024, 2.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#9a9183"), 0.85)
nt = mat.node_tree
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


# Packed pebbles: every Voronoi cell is a stone, rounded off by its distance to the cell edge, with
# dark fines in the gaps. A sliced 4D F1 distance gives sparse round discs (polka dots), so the
# shape comes from DISTANCE_TO_EDGE and the tone from an F1 node with the same cells. The
# 4D cells sliced by the torus surface come out about 5x larger than 1 / scale suggests: Voronoi 40
# gives stones about 2 cm across (measured on the 2x2 tile), and randomness 1 varies their size.
warp = noise(30.0, detail=2.0)
edge = voronoi(40.0, feature="DISTANCE_TO_EDGE")
cells = voronoi(40.0)
dome = smooth(math("ADD", edge.outputs["Distance"], math("MULTIPLY", warp, 0.03)), 0.02, 0.14)
stones = [(0.1, "#6a6359"), (0.35, "#8f8576"), (0.65, "#ada392"), (0.92, "#cbc2b0")]
stone_col = ramp(cells.outputs["Color"], stones)
fines = ramp(noise(40.0, detail=3.0, rough=0.6), [(0.3, "#3f3a32"), (0.75, "#564f45")])
col = mix(dome, fines, stone_col)
# Broad dusty and damp patches keep a 2 m repeat from reading as a grid (feature scale >= 4).
dust = smooth(noise(5.0, detail=3.0, rough=0.55), 0.45, 0.72)
col = mix(math("MULTIPLY", dust, 0.25), col, "#c9bfab")
damp = smooth(noise(6.5, detail=2.0, rough=0.5, distortion=0.3), 0.6, 0.78)
col = mix(math("MULTIPLY", damp, 0.25), col, "#5b544a")
nt.links.new(col, bsdf.inputs["Base Color"])

height = math("ADD", dome, math("MULTIPLY", warp, 0.15))
rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.92
rough.inputs["To Max"].default_value = 0.78
nt.links.new(height, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.55
bump.inputs["Distance"].default_value = 0.012
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
