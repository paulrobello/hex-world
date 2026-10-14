import math

import bpy


def srgb(hex_str):
    """'#rrggbb' -> linear RGBA (the-cube lib.py:46-55)."""
    h = hex_str.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255.0 for i in (0, 2, 4)]
    lin = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return (*lin, 1.0)


def ensure_nodes(mat):
    """Blender 5.x materials come with a node tree; `use_nodes` is deprecated (removed in 6.0)."""
    if mat.node_tree is None:
        mat.use_nodes = True
    return mat.node_tree


def pbr_material(name, base_color, roughness, metallic=0.0, emission=None):
    m = bpy.data.materials.new(name)
    p = next(n for n in ensure_nodes(m).nodes if n.type == "BSDF_PRINCIPLED")
    p.inputs["Base Color"].default_value = base_color
    p.inputs["Roughness"].default_value = roughness
    p.inputs["Metallic"].default_value = metallic
    if emission is not None:
        p.inputs["Emission Color"].default_value = emission
        p.inputs["Emission Strength"].default_value = 1.0
    return m


def torus_coords(nt, scale):
    """Seamless 4D coordinates for a 1x1 UV bake plane: (cos 2πu, sin 2πu, cos 2πv)·s, w = sin 2πv·s.
    Feed into 4D Noise/Voronoi so the baked tile wraps on both axes."""
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["UV"], sep.inputs[0])

    def op(kind, a, b=None):
        n = nt.nodes.new("ShaderNodeMath")
        n.operation = kind
        nt.links.new(a, n.inputs[0])
        if b is not None:
            if isinstance(b, float):
                n.inputs[1].default_value = b
            else:
                nt.links.new(b, n.inputs[1])
        return n.outputs[0]

    r = scale / (2.0 * math.pi)
    au = op("MULTIPLY", sep.outputs["X"], 2.0 * math.pi)
    av = op("MULTIPLY", sep.outputs["Y"], 2.0 * math.pi)
    comb = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(op("MULTIPLY", op("COSINE", au), r), comb.inputs["X"])
    nt.links.new(op("MULTIPLY", op("SINE", au), r), comb.inputs["Y"])
    nt.links.new(op("MULTIPLY", op("COSINE", av), r), comb.inputs["Z"])
    return comb.outputs[0], op("MULTIPLY", op("SINE", av), r)
