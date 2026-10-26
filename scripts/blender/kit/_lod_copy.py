"""Late LOD parts (owner A2 Q4, 2026-10-21: "every piece should have a middle LOD"): a copy of chosen
LOD0 faces, UVs included, joined into a coarser LOD after the masks bake. The copy samples LOD0's
baked texels, so the atlas is packed and baked exactly as before and LOD0's UV0 and the masks map do
not change. Use it for small parts a coarser LOD used to drop whole (a fantail hub, a bread basket),
where LOD0's own geometry is already as cheap as that part can be."""
import bmesh
import bpy

from lib import geom, scene


def copy_faces(src, dst, keep):
    """Joins into `dst` a copy of the faces of `src` for which keep(centre, material_name) holds
    (`centre` the face's median in `src`'s local frame) and returns (joined object, copied tris).
    Both objects must carry the same material list in the same order (the callers' LOD builders
    append every slot to every LOD), so the joined LOD keeps LOD0's primitive order."""
    assert [m.name for m in src.data.materials] == [m.name for m in dst.data.materials], (src.name, dst.name)

    def key(mw, cos):
        return tuple(sorted(tuple(round(c, 4) for c in mw @ co) for co in cos))
    # A face the coarser LOD already has (a post both LODs share) is never copied twice.
    have = {key(dst.matrix_world, [dst.data.vertices[v].co for v in p.vertices]) for p in dst.data.polygons}
    mesh = src.data.copy()
    bm = bmesh.new()
    bm.from_mesh(mesh)
    names = [m.name for m in mesh.materials]
    drop = [f for f in bm.faces if not keep(f.calc_center_median(), names[f.material_index])
            or key(src.matrix_world, [v.co for v in f.verts]) in have]
    bmesh.ops.delete(bm, geom=drop, context="FACES")
    tris = sum(len(f.verts) - 2 for f in bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    part = scene.link(bpy.data.objects.new(f"{dst.name}_late", mesh))
    part.matrix_world = src.matrix_world.copy()
    parent, name = dst.parent, dst.name
    out = geom.join([dst, part], name)
    out.parent = parent
    return out, tris


def copy_with_support(src, dst, slots, support, below, reach):
    """copy_faces for every face in `slots`, plus the faces in slot `support` whose centre lies below
    z = `below` and within `reach` (horizontally) of a `slots` face: the bakery's bread baskets and
    the bench they stand on."""
    names = [m.name for m in src.data.materials]
    near = [p.center.copy() for p in src.data.polygons if names[p.material_index] in slots]

    def keep(c, m):
        if m in slots:
            return True
        return m == support and c.z < below and min((c.xy - b.xy).length for b in near) < reach
    return copy_faces(src, dst, keep)
