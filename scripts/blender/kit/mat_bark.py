"""Tileable 'bark' PBR set (owner ruling, Task 10 look pass): 1024² over 2 m = 512 px/m, mapped on the
tree's UV0 (U around the strand, V along it; manifest `tileUvScale`), so the grain runs along every
trunk strand, limb and root. Scripted; seamless via lib.materials.torus_coords, stretched along V.
Look: ref-1's oak bark, long plates split by deep vertical fissures, warm brown with dark grooves."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export, materials, scene  # noqa: E402

SET, SIZE, TILE_M = "bark", 1024, 2.0
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#5e4029"), 0.9)
nt = mat.node_tree
bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
vec0, w0 = materials.torus_coords(nt, scale=TILE_M)


def stretched(along_v):
    """The torus coordinates with the V circle (Z and W) scaled by along_v < 1: features stretch
    along V (the strand direction) and the tile still wraps on both axes."""
    vm = nt.nodes.new("ShaderNodeVectorMath")
    vm.operation = "MULTIPLY"
    vm.inputs[1].default_value = (1.0, 1.0, along_v)
    nt.links.new(vec0, vm.inputs[0])
    wm = nt.nodes.new("ShaderNodeMath")
    wm.operation = "MULTIPLY"
    wm.inputs[1].default_value = along_v
    nt.links.new(w0, wm.inputs[0])
    return vm.outputs[0], wm.outputs[0]


# Every texture is 4D and fed (vec, w) only: 3D textures ignore W and mirror the tile (no wrap).
def noise(coords, scale, detail=4.0, rough=0.5, distortion=0.0):
    n = nt.nodes.new("ShaderNodeTexNoise")
    n.noise_dimensions = "4D"
    n.inputs["Scale"].default_value = scale
    n.inputs["Detail"].default_value = detail
    n.inputs["Roughness"].default_value = rough
    n.inputs["Distortion"].default_value = distortion
    nt.links.new(coords[0], n.inputs["Vector"])
    nt.links.new(coords[1], n.inputs["W"])
    return n.outputs["Fac"]


def voronoi(coords, scale, feature="F1", randomness=1.0):
    n = nt.nodes.new("ShaderNodeTexVoronoi")
    n.voronoi_dimensions = "4D"
    n.feature = feature
    n.inputs["Scale"].default_value = scale
    n.inputs["Randomness"].default_value = randomness
    nt.links.new(coords[0], n.inputs["Vector"])
    nt.links.new(coords[1], n.inputs["W"])
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


# Plates: Voronoi cells stretched about 5.5x along the strand; the fissures are the cell edges, widened and
# wobbled by noise so they read as deep, irregular vertical grooves.
long4 = stretched(0.18)
plates = voronoi(long4, 5.0, feature="DISTANCE_TO_EDGE", randomness=0.9)
wobble = math("MULTIPLY", noise(long4, 9.0, detail=3.0), 0.025)
edge = math("ADD", plates.outputs["Distance"], wobble)
plate_h = smooth(edge, 0.015, 0.16)
# Plate surface: fine streaks along the grain and cross cracks, small enough to read at 512 px/m.
long8 = stretched(0.125)
streak = noise(long8, 60.0, detail=4.0, rough=0.6, distortion=0.8)
cracks = voronoi(stretched(0.6), 14.0, feature="DISTANCE_TO_EDGE")
crack_h = smooth(cracks.outputs["Distance"], 0.0, 0.05)
height = math("MULTIPLY", plate_h, math("ADD", math("MULTIPLY", streak, 0.35), 0.65))
height = math("MULTIPLY", height, math("ADD", math("MULTIPLY", crack_h, 0.25), 0.75))

# Colour: near-black fissures, warm brown plates, lighter worn plate tops, faint grey-green lichen.
col = ramp(height, [(0.05, "#3a2717"), (0.3, "#6a4a2c"), (0.6, "#9c7149"), (0.9, "#c19569")])
lichen = math("MULTIPLY", smooth(noise((vec0, w0), 3.0, detail=3.0), 0.58, 0.68), smooth(height, 0.5, 0.8))
col = mix(math("MULTIPLY", lichen, 0.55), col, "#7d8364")
nt.links.new(col, bsdf.inputs["Base Color"])

rough = nt.nodes.new("ShaderNodeMapRange")
rough.inputs["To Min"].default_value = 0.96
rough.inputs["To Max"].default_value = 0.8
nt.links.new(height, rough.inputs["Value"])
nt.links.new(rough.outputs["Result"], bsdf.inputs["Roughness"])

bump = nt.nodes.new("ShaderNodeBump")
bump.inputs["Strength"].default_value = 0.9
bump.inputs["Distance"].default_value = 0.04
nt.links.new(height, bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])

bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
