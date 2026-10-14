"""Debug renders of the tree wind UV (TEXCOORD_1, Blender UV map 2): U = sway weight as a
blue-to-yellow ramp, V = phase as hue. Evidence for the checkpoint only, not a pipeline stage.
Usage: blender -b --factory-startup --python-exit-code 1 -P scripts/blender/tree_wind_debug.py -- <glb> <out_dir>"""
import pathlib
import shutil
import sys

import bpy

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import gate  # noqa: E402

glb, out = sys.argv[sys.argv.index("--") + 1:][:2]
out = pathlib.Path(out)
objs = gate.import_glb(glb)
lod0 = [o for o in objs if o.type == "MESH" and o.name.endswith("_lod0")]
assert lod0, [o.name for o in objs]
wind_uv = lod0[0].data.uv_layers[1].name  # the importer names TEXCOORD_1 after TEXCOORD_0's map


def debug_material(channel: str) -> bpy.types.Material:
    m = bpy.data.materials.new(f"wind_{channel}")
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = wind_uv
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(uvn.outputs["UV"], sep.inputs[0])
    if channel == "u":
        col = nt.nodes.new("ShaderNodeValToRGB")
        col.color_ramp.elements[0].color = (0.02, 0.05, 0.6, 1)
        col.color_ramp.elements[1].color = (1.0, 0.85, 0.05, 1)
        nt.links.new(sep.outputs["X"], col.inputs["Fac"])
        rgb = col.outputs["Color"]
    else:
        hsv = nt.nodes.new("ShaderNodeCombineColor")
        hsv.mode = "HSV"
        hsv.inputs[1].default_value = 0.85
        hsv.inputs[2].default_value = 0.9
        nt.links.new(sep.outputs["Y"], hsv.inputs[0])
        rgb = hsv.outputs["Color"]
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(rgb, em.inputs["Color"])
    o = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(em.outputs[0], o.inputs["Surface"])
    return m


for ch in ("u", "v"):
    mat = debug_material(ch)
    for o in lod0:
        for i in range(len(o.data.materials)):
            o.data.materials[i] = mat
    d = out / f"wind-{ch}"
    gate.render_six_views(objs, str(d), str(d / "png"), size=1024)
    shutil.copy(d / "png" / "three_quarter.png", out / f"tree-wind-{ch}.png")
print(f"wrote {out}/tree-wind-u.png and tree-wind-v.png")
