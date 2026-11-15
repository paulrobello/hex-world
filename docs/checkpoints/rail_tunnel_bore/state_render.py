"""Six-view renders of a staged GLB in one runtime state: closed (faces on `_open` materials removed) or
open (faces on `_back` materials removed). Same cameras and settings as make kit-renders.
Usage: blender -b --factory-startup -P state_render.py -- <repo> <glb> <closed|open> <out_dir>"""
import pathlib
import sys

args = sys.argv[sys.argv.index("--") + 1:]
repo, glb, mode, out = pathlib.Path(args[0]), pathlib.Path(args[1]), args[2], pathlib.Path(args[3])
sys.path.insert(0, str(repo / "scripts" / "blender"))
import bmesh  # noqa: E402
from lib import gate  # noqa: E402

objs = gate.import_glb(glb)
suffix = "_open" if mode == "closed" else "_back"
gone = 0
for o in objs:
    if o.type != "MESH":
        continue
    drop = {i for i, m in enumerate(o.data.materials) if m and m.name.endswith(suffix)}
    if not drop:
        continue
    bm = bmesh.new()
    bm.from_mesh(o.data)
    faces = [f for f in bm.faces if f.material_index in drop]
    gone += len(faces)
    bmesh.ops.delete(bm, geom=faces, context="FACES")
    bm.to_mesh(o.data)
    bm.free()
    o.data.update()
print(f"state {mode}: removed {gone} faces on *{suffix}")
print("\n".join(gate.render_six_views(objs, out / "jpg", png_dir=out / "png")))
