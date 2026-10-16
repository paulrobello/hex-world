"""Shared kit material-node factories and UV-island rasterization."""
from collections.abc import Callable

import bpy
import numpy as np

from lib import materials


def make_math(nt: bpy.types.ShaderNodeTree) -> Callable:
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

    return math


def make_ramp(nt: bpy.types.ShaderNodeTree) -> Callable:
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

    return ramp


def island_texels(obj, size):
    """obj's UV0 islands rasterized at texel centres (bool size x size, Blender pixel order)."""
    island = np.zeros((size, size), bool)
    uvd = obj.data.uv_layers["UVMap"].data
    for poly in obj.data.polygons:
        loops = list(poly.loop_indices)
        for k in range(1, len(loops) - 1):
            tri = np.array([uvd[loops[0]].uv[:], uvd[loops[k]].uv[:], uvd[loops[k + 1]].uv[:]]) * size
            lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
            xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = tri
            d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(d) < 1e-12:
                continue
            l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
            l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
            inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
            island[ys[inside].astype(int).clip(0, size - 1), xs[inside].astype(int).clip(0, size - 1)] = True
    return island
