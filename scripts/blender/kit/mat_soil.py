"""Tileable 'soil' PBR set: 1024² over 4 m = 256 px/m (owner-approved D2 exception, 2026-10-13: one strata set per 4 m wall). Scripted; seamless via lib.materials.torus_coords.
Look: ref-1's layered side walls, dark topsoil over red clay over gravel (owner ruling B1)."""
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


# Strata (ref-1): topsoil, red clay, gravel banded along V. Walls map texture-up to world +Y and the
# cap rim (world y = 0) sits on the V = 1/0 wrap, so the rim must be topsoil. The band coordinate is
# fract(V + offset + seamless jitter): V is recovered from the torus coordinates as atan2(w, z) / 2π,
# so it stays a function of (vec, w) only, and fract keeps it continuous across the wrap.
sep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(vec, sep.inputs[0])
v = math("DIVIDE", math("ARCTAN2", w, sep.outputs["Z"]), 6.283185307179586)
jitter = math("MULTIPLY", math("SUBTRACT", noise(3.0, detail=3.0), 0.5), 0.03)
band = math("FRACT", math("ADD", math("ADD", v, -0.02), jitter))
# Upward on the wall: gravel (0-0.25), clay (0.3-0.55), topsoil (0.6-1.0). With tileM 4 the tile spans the
# whole 4 m skirt, so the rim and the skirt foot share the wrap row; the topsoil/gravel contact at band 0 = 1
# sits 0.02-0.14 m above the foot, leaving one strata set per wall.
grain = noise(60.0, detail=6.0, rough=0.6)
topsoil = ramp(grain, [(0.35, "#1f1a16"), (0.65, "#3a2f27")])
clay = ramp(noise(25.0, detail=5.0, rough=0.6, distortion=0.6), [(0.35, "#7a3524"), (0.65, "#a04a30")])
pebbles = voronoi(45.0)
pebble_mask = math("SUBTRACT", 1.0, smooth(pebbles.outputs["Distance"], 0.35, 0.5))
gravel = mix(pebble_mask, ramp(grain, [(0.3, "#4a3d30"), (0.7, "#6a5846")]),
             ramp(pebbles.outputs["Color"], [(0.3, "#6d7176"), (0.7, "#9a9ea2")]))
col = mix(smooth(band, 0.25, 0.3), gravel, clay)
col = mix(smooth(band, 0.55, 0.6), col, topsoil)
specks = voronoi(30.0)
speck_mask = math("MULTIPLY", math("SUBTRACT", 1.0, smooth(specks.outputs["Distance"], 0.08, 0.14)),
                  smooth(specks.outputs["Color"], 0.72, 0.73))
col = mix(speck_mask, col, "#8b8f93")
nt.links.new(col, bsdf.inputs["Base Color"])

gravel_h = math("MULTIPLY", pebble_mask, math("SUBTRACT", 1.0, smooth(band, 0.25, 0.3)))
height = math("ADD", math("MULTIPLY", grain, 0.5), math("MULTIPLY", gravel_h, 0.5))
height = math("ADD", height, math("MULTIPLY", speck_mask, 0.3))

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
