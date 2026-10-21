"""Dense water-contract check of a staged (uncompressed) river base GLB: every up-facing LOD triangle
sampled at the 15 barycentric points (i, j, k) / 4, against the analytic profile of its `_river`
layout, wet (profile below -1.0 m) and dry split as `_river.profile_errors` does. From the repo root:
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/river_bend_60/dense_profile.py \
    -- target/kit-stage/river_bend_60_base/river_bend_60_base.glb bend60
(`bend` for river_bend_base, `straight` for river_straight_base)."""
import pathlib
import sys

import bpy

WT = str(pathlib.Path(__file__).resolve().parents[3] / "scripts/blender")
sys.path.insert(0, WT)
sys.path.insert(0, WT + "/kit")

import _river  # noqa: E402
import _river_bend_60  # noqa: E402,F401  (registers bend60)
from lib import hexgeo  # noqa: E402

glb, layout = sys.argv[sys.argv.index("--") + 1:][:2]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=glb)
dist = _river.LAYOUTS[layout]["dist"]
pts = [(i, j, 4 - i - j) for i in range(5) for j in range(5 - i)]
for obj in sorted((o for o in bpy.data.objects if o.type == "MESH" and "_lod" in o.name), key=lambda o: o.name):
    m = obj.matrix_world
    mesh = obj.data
    mesh.calc_loop_triangles()
    wet = dry = 0.0
    worst = None
    n = 0
    for t in mesh.loop_triangles:
        a, b, c = (m @ mesh.vertices[v].co for v in t.vertices)
        nz = (b - a).cross(c - a).normalized().z
        if abs(nz) <= 0.2:
            continue
        for i, j, k in pts:
            p = (a * i + b * j + c * k) / 4.0
            if p.z < -_river.LEVEL_EPS_SKIRT or (abs(hexgeo.dist_to_border(p.x, p.y)) < 1e-4 and p.z < -1e-6):
                continue
            d = dist(p.x, p.y)
            z = _river.profile_z(d)
            e = abs(p.z - z)
            n += 1
            if z < _river.WET_Z:
                if e > wet:
                    wet, worst = e, (round(p.x, 2), round(p.y, 2), round(d, 3))
            else:
                dry = max(dry, e)
    print(f"DENSE {obj.name}: samples {n}, wet max {wet:.4f} at {worst}, dry max {dry:.4f}")
