"""Measure rail_buffer_stop at every kind of open rail end against the real track meshes: the stop's
LOD0 posed by the runtime rule (runtime-notes.md section 1) on rail_straight_center, rail_bend_60_center
and rail_junction_center (toe on the main line, branch end, straight end), each track's LOD0 from its
generator's uncompressed stage GLB. Per end:
- feetOffHeadM: the worst plan distance of a foot centreline point (a stop vertex resting on a rail
  head, moved onto its leg's plane |y| = RAIL_T) from the nearest rail-head top face of the track
  (0 = on a head);
- clashTris: stop triangles that intersect track triangles with the stop lifted 1 mm (so resting
  contact does not count), with the track slots they hit and how high those hits reach.
Run (Blender 5.2, headless):
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_buffer_stop/end_fit.py -- \
    <stop stage dir> <straight stage dir> <bend stage dir> <junction stage dir>
Stage dirs come from `make kit-build PIECE=<id>` (target/kit-stage/<id>) or the generator run into
any directory."""
import json
import math
import sys

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

A = 13.856406
R, C = 8.0, Vector((A, -8.0))
STOP_U, CHORD = 3.5, 2.24
RAIL_TOP, RAIL_Y = 0.46, 0.84


def arc(theta0, sign):
    def p(u):
        t = math.radians(theta0) + sign * u / R
        return Vector((C.x + R * math.cos(t), C.y + R * math.sin(t)))
    return p


PATHS = {
    "edge0_straight": lambda u: Vector((A - u, 0.0)),
    "edge3_straight": lambda u: Vector((-A + u, 0.0)),
    "edge0_bend": arc(90.0, 1),
    "edge1_bend": arc(210.0, -1),
}
ENDS = [("straight", "rail_straight_center", "edge0_straight"),
        ("bend edge 0", "rail_bend_60_center", "edge0_bend"),
        ("bend edge 1", "rail_bend_60_center", "edge1_bend"),
        ("junction toe (main line)", "rail_junction_center", "edge0_straight"),
        ("junction branch end", "rail_junction_center", "edge1_bend"),
        ("junction straight end", "rail_junction_center", "edge3_straight")]


def load(path, name):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    o = next(o for o in new if o.name == name)
    bpy.context.view_layer.update()
    return o


def world_tris(o, mat=Matrix.Identity(4)):
    me = o.data
    me.calc_loop_triangles()
    m = mat @ o.matrix_world
    verts = [m @ v.co for v in me.vertices]
    return verts, me.loop_triangles, me


def plan_dist(p, tri):
    """Plan (x, y) distance from p to triangle tri (three Vectors); 0 inside."""
    a, b, c = (Vector((q.x, q.y)) for q in tri)
    q = Vector((p.x, p.y))

    def cross(o, u, v):
        return (u.x - o.x) * (v.y - o.y) - (u.y - o.y) * (v.x - o.x)
    s = [cross(a, b, q), cross(b, c, q), cross(c, a, q)]
    if all(x >= 0 for x in s) or all(x <= 0 for x in s):
        return 0.0

    def seg(u, v):
        d = v - u
        t = max(0.0, min(1.0, (q - u).dot(d) / max(d.length_squared, 1e-12)))
        return (u + d * t - q).length
    return min(seg(a, b), seg(b, c), seg(c, a))


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    stop_dir, dirs = argv[0], {"rail_straight_center": argv[1], "rail_bend_60_center": argv[2],
                               "rail_junction_center": argv[3]}
    bpy.ops.wm.read_factory_settings(use_empty=True)
    stop = load(f"{stop_dir}/rail_buffer_stop.glb", "rail_buffer_stop_lod0")
    tracks = {k: load(f"{d}/{k}.glb", f"{k}_lod0") for k, d in dirs.items()}
    out = {}
    for label, track_id, path_key in ENDS:
        path = PATHS[path_key]
        o, f = path(STOP_U), path(STOP_U - CHORD)
        d = (f - o).normalized()
        yaw = math.atan2(d.y, d.x)
        pose = Matrix.Translation((o.x, o.y, 0.0)) @ Matrix.Rotation(yaw, 4, "Z")
        lift = Matrix.Translation((0.0, 0.0, float(argv[4]) if len(argv) > 4 else 0.001)) @ pose
        tv, ttris, tme = world_tris(tracks[track_id])
        heads = [[tv[i] for i in t.vertices] for t in ttris
                 if tme.materials[t.material_index].name.split(".")[0] == "rail_head"
                 and all(abs(tv[i].z - RAIL_TOP) < 0.012 for i in t.vertices)]
        feet = [pose @ Vector((v.co.x, math.copysign(RAIL_Y, v.co.y), v.co.z))
                for v in stop.data.vertices if abs(v.co.z - RAIL_TOP) < 1e-4]
        off = max(min(plan_dist(p, h) for h in heads) for p in feet)
        sv, stris, sme = world_tris(stop, lift)
        bs = BVHTree.FromPolygons(sv, [t.vertices for t in stris])
        bt = BVHTree.FromPolygons(tv, [t.vertices for t in ttris])
        pairs = bs.overlap(bt)
        hit_slots, top = {}, None
        for i, j in pairs:
            slot = tme.materials[ttris[j].material_index].name.split(".")[0]
            hit_slots[slot] = hit_slots.get(slot, 0) + 1
            z = max(tv[k].z for k in ttris[j].vertices)
            top = z if top is None else max(top, z)
        out[label] = {"track": track_id, "yawDeg": round(math.degrees(yaw), 2),
                      "feetOffHeadM": round(off, 4), "footPoints": len(feet),
                      "clashTris": len({i for i, _ in pairs}), "clashTrackSlots": hit_slots,
                      "clashTopZ": None if top is None else round(top, 3)}
        print(f"END_FIT {label}: {json.dumps(out[label])}")
    print("END_FIT_JSON " + json.dumps(out))


main()
