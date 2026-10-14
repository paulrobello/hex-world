import statistics

import bpy

from . import hexgeo, scene


def planar_cap_uv(obj, layer="UVMap"):
    """Unique UV0 for a cap: top-down planar map of the 2R x 2R square (32 m -> 0..1).
    U = (x + R) / 2R, V = (y + R) / 2R in Blender's frame."""
    mesh = obj.data
    uvl = mesh.uv_layers.get(layer) or mesh.uv_layers.new(name=layer)
    s = 2.0 * hexgeo.HEX_R
    for loop in mesh.loops:
        co = mesh.vertices[loop.vertex_index].co
        uvl.data[loop.index].uv = ((co.x + hexgeo.HEX_R) / s, (co.y + hexgeo.HEX_R) / s)


def atlas_unwrap(objs, margin=0.008):
    """One atlas across objs (astra_machine_common.py:1162-1173)."""
    scene.select_only(objs)
    for o in objs:
        if not o.data.uv_layers:
            o.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.152, island_margin=margin, area_weight=0.2)
    bpy.ops.uv.pack_islands(rotate=True, margin=margin)
    bpy.ops.object.mode_set(mode="OBJECT")


def measure_texel_density(obj, atlas_px, layer="UVMap"):
    """px/m per face = sqrt(uv_area * atlas_px² / world_area); returns min/median/max over faces."""
    mesh = obj.data
    uvl = mesh.uv_layers[layer]
    w = obj.matrix_world
    vals = []
    for p in mesh.polygons:
        if p.area <= 1e-9:
            continue
        uvs = [uvl.data[li].uv for li in p.loop_indices]
        a = 0.0
        for i in range(1, len(uvs) - 1):
            e1, e2 = uvs[i] - uvs[0], uvs[i + 1] - uvs[0]
            a += abs(e1.x * e2.y - e1.y * e2.x) / 2.0
        if a <= 1e-12:
            continue  # collapsed UVs (e.g. skirts parked at one texel) are intentionally unmapped
        world_area = p.area * abs(w.determinant()) ** (2.0 / 3.0)
        vals.append((a * atlas_px * atlas_px / world_area) ** 0.5)
    return {"min": min(vals), "median": statistics.median(vals), "max": max(vals)}
