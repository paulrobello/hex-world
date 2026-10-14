import bmesh
import bpy

from . import scene


def collider_from(src, kind, name):
    """Collider mesh node named `name` (convention: <piece>_collider), custom prop collider=kind.
    Stays render-visible for export (hidden objects can drop out of selected exports); the engine
    hides it. kind: 'cap' (copy of src, e.g. LOD2), 'hull' (convex hull), 'box' (AABB)."""
    bm = bmesh.new()
    bm.from_mesh(src.data)
    bm.transform(src.matrix_world)
    if kind == "hull":
        res = bmesh.ops.convex_hull(bm, input=bm.verts)
        # A vertex can be listed as both unused and interior; delete rejects duplicates.
        drop = dict.fromkeys(g for g in res["geom_unused"] + res["geom_interior"] if isinstance(g, bmesh.types.BMVert))
        bmesh.ops.delete(bm, geom=list(drop), context="VERTS")
    elif kind == "box":
        xs, ys, zs = zip(*[v.co[:] for v in bm.verts])
        bm.clear()
        bmesh.ops.create_cube(bm, size=1.0)
        for v in bm.verts:
            v.co.x = (min(xs) + max(xs)) / 2 + v.co.x * (max(xs) - min(xs))
            v.co.y = (min(ys) + max(ys)) / 2 + v.co.y * (max(ys) - min(ys))
            v.co.z = (min(zs) + max(zs)) / 2 + v.co.z * (max(zs) - min(zs))
    elif kind != "cap":
        bm.free()
        raise ValueError(f"unknown collider kind {kind}")
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    obj["collider"] = kind
    return obj
