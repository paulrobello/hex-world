import bmesh
import bpy
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from . import hexgeo, scene


def _object(name, verts, faces):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    return scene.link(obj)


def cdt_cap(name, border_pts, interior_pts, height_fn):
    """Terrain cap: constrained Delaunay of the border loop + interior points. Border z = 0 exactly,
    interior z = height_fn(x, y). Every LOD built with the same border_pts shares the border ring."""
    nb = len(border_pts)
    coords = [Vector((x, y)) for x, y in border_pts] + [Vector((x, y)) for x, y in interior_pts]
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt(coords, [], [list(range(nb))], 1, 1e-6)
    border_ids = {i for i, o in enumerate(orig_v) if any(j < nb for j in o)}
    verts = [(v.x, v.y, 0.0 if i in border_ids else height_fn(v.x, v.y)) for i, v in enumerate(out_v)]
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    obj = _object(name, verts, faces)
    for p in obj.data.polygons:
        p.use_smooth = True
    return obj


def skirt(name, border_pts, depth, rows):
    """Side walls from the border ring (z = 0) down to z = -depth, `rows` quads high, triangulated."""
    n = len(border_pts)
    verts = [(x, y, -depth * r / rows) for r in range(rows + 1) for (x, y) in border_pts]
    faces = []
    for r in range(rows):
        for i in range(n):
            a, b = r * n + i, r * n + (i + 1) % n
            c, d = b + n, a + n
            # Winding is normalized by the outward flip pass below (asserted in test_skirt_outward).
            faces += [[a, d, c], [a, c, b]]
    obj = _object(name, verts, faces)
    mid = Vector((0.0, 0.0, 0.0))
    for p in obj.data.polygons:
        if p.normal.dot(p.center - mid) < 0:
            p.flip()
    obj.data.update()
    return obj


def tri_count(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def weld(obj, tol=0.001):
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=tol)
    bm.to_mesh(obj.data)
    bm.free()


def set_border_normals_up(obj, tol=1e-4):
    """Custom normals: +Z on the border ring so neighbouring caps shade identically (PRD §6)."""
    mesh = obj.data
    normals = []
    for v in mesh.vertices:
        on_border = abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < tol and abs(v.co.z) < tol
        normals.append((0.0, 0.0, 1.0) if on_border else tuple(v.normal))
    mesh.normals_split_custom_set_from_vertices(normals)
    mesh.update()


def set_cap_normals(obj, slope_fn, tol=1e-4):
    """Custom normals from an analytic slope: normalize(-sx, -sy, 1) with (sx, sy) = slope_fn(x, y),
    and +Z on the border ring as `set_border_normals_up`. slope_fn may return None to keep the
    vertex's own normal (a channel strip inside a meadow cap). The cap's faces must be smooth."""
    mesh = obj.data
    normals = []
    for v in mesh.vertices:
        if abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < tol and abs(v.co.z) < tol:
            normals.append((0.0, 0.0, 1.0))
            continue
        slope = slope_fn(v.co.x, v.co.y)
        if slope is None:
            normals.append(tuple(v.normal))
            continue
        sx, sy = slope
        normals.append(tuple(Vector((-sx, -sy, 1.0)).normalized()))
    mesh.normals_split_custom_set_from_vertices(normals)
    mesh.update()


def join(objs, name):
    for o in objs:
        scene.apply_transforms(o)
    scene.select_only(objs, objs[0])
    bpy.ops.object.join()
    out = bpy.context.view_layer.objects.active
    out.name = name
    out.data.name = name
    return out
