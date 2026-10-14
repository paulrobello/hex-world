import bpy

from . import geom, hexgeo, scene


def triangulate(obj):
    scene.select_only([obj])
    m = obj.modifiers.new("Triangulate", "TRIANGULATE")
    bpy.ops.object.modifier_apply(modifier=m.name)


def decimate_to(obj, target_tris):
    """Collapse-decimate to at most target_tris. Only for pieces that do not touch the hex border."""
    triangulate(obj)
    for _ in range(4):
        cur = geom.tri_count(obj)
        if cur <= target_tris:
            break
        scene.select_only([obj])
        m = obj.modifiers.new("LOD", "DECIMATE")
        m.decimate_type = "COLLAPSE"
        m.use_collapse_triangulate = True
        m.ratio = max(0.005, (target_tris * 0.98) / cur)
        bpy.ops.object.modifier_apply(modifier=m.name)
    return geom.tri_count(obj)


def border_verts(obj, tol=1e-4):
    """World-space border-ring vertices (on the hex border at z = 0), rounded to 0.1 mm."""
    w = obj.matrix_world
    out = set()
    for v in obj.data.vertices:
        p = w @ v.co
        if abs(hexgeo.dist_to_border(p.x, p.y)) < tol and abs(p.z) < tol:
            out.add((round(p.x, 4), round(p.y, 4)))
    return out


def assert_border_identical(objs, tol=1e-4):
    """Raise AssertionError unless every obj has exactly objs[0]'s non-empty border ring."""
    ref = border_verts(objs[0], tol)
    if not ref:
        raise AssertionError(f"{objs[0].name}: no border ring vertices")
    for o in objs[1:]:
        got = border_verts(o, tol)
        if got != ref:
            raise AssertionError(f"{o.name}: border ring differs from {objs[0].name} "
                                 f"(missing {len(ref - got)}, extra {len(got - ref)})")
