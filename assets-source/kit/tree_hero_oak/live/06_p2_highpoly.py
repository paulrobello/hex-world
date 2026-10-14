# tree_hero_oak P2 polish, step 6 (HEADLESS, no live Blender): rebuild the high poly from _oak.py
# (heavier roots, visible limbs, lump-free master dome, per-clump lumped fringe) inside the stored
# .blend and write a new revision to the blend store. Replayable:
#   "$BLENDER" -b "$(python3 scripts/blender/lib/paths.py blend tree_hero_oak)" --factory-startup \
#     --python-exit-code 1 -P assets-source/kit/tree_hero_oak/live/06_p2_highpoly.py
# OAK_BLEND_OUT overrides the output (scratch iterations); the default is the store's canonical
# <id>_high.blend, and the revision it replaces is first copied aside in the store as
# <id>_high.<sha12>.blend (never deleted). Only tree_hero_oak-prefixed data is touched.
import hashlib
import json
import os
import pathlib
import shutil
import sys

import bpy

ROOT = globals().get("REPO") or str(pathlib.Path(__file__).resolve().parents[4])
sys.path.insert(0, f"{ROOT}/scripts/blender")
sys.path.insert(0, f"{ROOT}/scripts/blender/kit")
from lib import paths  # noqa: E402
import _oak  # noqa: E402
import _tree  # noqa: E402

P = _tree.PIECE
scn = bpy.data.scenes[P]
assert bpy.context.scene == scn, bpy.context.scene
src = bpy.data.filepath
OUT = os.environ.get("OAK_BLEND_OUT") or str(paths.blend_path(P))

# Replace the authored tree objects (the showcase rig: lights, camera, ground, stays).
for o in list(scn.objects):
    if o.name in (f"{P}_bark_high", f"{P}_leaves_high", f"{P}_canopy_high") or o.name.startswith(f"{P}_canopy_show_"):
        bpy.data.objects.remove(o, do_unlink=True)
for d in list(bpy.data.meshes):
    if d.name.startswith(P) and d.users == 0:
        bpy.data.meshes.remove(d)
bpy.context.view_layer.update()

bark = bpy.data.materials[f"{P}_bark"]
leaves = bpy.data.materials[f"{P}_leaves"]
canopy_mat = bpy.data.materials[f"{P}_canopy_high"]
skel = _oak.skeleton()
crown = _tree.layout()

hi = _tree.tube_mesh(f"{P}_bark_high", skel, {0: 40, 1: 28, 2: 16, 3: 10},
                     {0: 0.025, 1: 0.04, 2: 0.05, 3: 0.05}, displace=0.07)
hi.data.materials.append(bark)
master = _oak.canopy_high(f"{P}_canopy_high")
master.data.materials.append(canopy_mat)
master.hide_render = True
master.hide_set(True)
# Showcase instances: each clump's own lumped shell surface is a low-poly export-stage detail, so the
# showcase shows the master dome at every clump, as in step 1.
for i, cl in enumerate(crown):
    inst = bpy.data.objects.new(f"{P}_canopy_show_{i:02d}", master.data)
    inst.matrix_world = _tree.clump_matrix(cl)
    scn.collection.objects.link(inst)
lv = _oak.leaf_cards(f"{P}_leaves_high", crown)
lv.data.materials.append(leaves)
scn["tho_skeleton_sha"] = _tree.skeleton_sha(skel)
scn["tho_p2"] = "06_p2_highpoly"

os.makedirs(os.path.dirname(OUT), exist_ok=True)
backup = None
if os.path.exists(OUT) and os.path.realpath(OUT) == os.path.realpath(src):
    sha = hashlib.sha256(pathlib.Path(OUT).read_bytes()).hexdigest()
    backup = OUT.replace("_high.blend", f"_high.{sha[:12]}.blend")
    if not os.path.exists(backup):
        shutil.copy2(OUT, backup)
bpy.data.libraries.write(OUT, {scn}, path_remap="ABSOLUTE", fake_user=False, compress=False)
print(json.dumps({"out": OUT, "bytes": os.path.getsize(OUT),
                  "sha256": hashlib.sha256(pathlib.Path(OUT).read_bytes()).hexdigest(),
                  "replaced_revision_copy": backup, "skeleton_sha": scn["tho_skeleton_sha"],
                  "bark_high_tris": sum(len(p.vertices) - 2 for p in hi.data.polygons),
                  "canopy_master_tris": len(master.data.polygons), "fringe_cards": len(lv.data.polygons),
                  "branches": len(skel), "objects": sorted(o.name for o in scn.objects)}))
