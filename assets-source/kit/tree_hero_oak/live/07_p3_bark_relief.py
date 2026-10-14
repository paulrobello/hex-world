# tree_hero_oak P2 polish round 2, step 7 (HEADLESS, no live Blender): rebuild only the bark high poly
# with shallower ridges and write a new revision to the blend store. The bark's colour, normal and
# fissure AO come from the tileable `bark` set (531 px/m); the high poly feeds only the masks.A AO
# bake (59 px/m). Its 0.07 m ridges are fissure-scale relief that the set already carries, and they
# do not line up with the set's fissures, so in the bake they read as uncorrelated dark streaks.
# 0.025 m keeps a trace of the ridge flow while the macro occlusion (strand grooves, junctions) stays.
# Replayable:
#   "$BLENDER" -b "$(python3 scripts/blender/lib/paths.py blend tree_hero_oak)" --factory-startup \
#     --python-exit-code 1 -P assets-source/kit/tree_hero_oak/live/07_p3_bark_relief.py
# OAK_BLEND_OUT overrides the output (scratch iterations); the default is the store's canonical
# <id>_high.blend, and the revision it replaces is first copied aside in the store as
# <id>_high.<sha12>.blend (never deleted). Only tree_hero_oak_bark_high is replaced.
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
RELIEF = float(os.environ.get("OAK_BARK_RELIEF", "0.025"))  # m; step 06 used 0.07
scn = bpy.data.scenes[P]
assert bpy.context.scene == scn, bpy.context.scene
src = bpy.data.filepath
OUT = os.environ.get("OAK_BLEND_OUT") or str(paths.blend_path(P))
skel = _oak.skeleton()
assert _tree.skeleton_sha(skel) == scn["tho_skeleton_sha"], "skeleton changed since step 06"

old = bpy.data.objects[f"{P}_bark_high"]
bark = bpy.data.materials[f"{P}_bark"]
bpy.data.objects.remove(old, do_unlink=True)
for d in list(bpy.data.meshes):
    if d.name.startswith(f"{P}_bark_high") and d.users == 0:
        bpy.data.meshes.remove(d)
bpy.context.view_layer.update()
hi = _tree.tube_mesh(f"{P}_bark_high", skel, {0: 40, 1: 28, 2: 16, 3: 10},
                     {0: 0.025, 1: 0.04, 2: 0.05, 3: 0.05}, displace=RELIEF)
hi.data.materials.append(bark)
if hi.name not in scn.collection.objects:
    for c in list(hi.users_collection):
        c.objects.unlink(hi)
    scn.collection.objects.link(hi)
scn["tho_p3"] = f"07_p3_bark_relief {RELIEF}"

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
                  "replaced_revision_copy": backup, "relief_m": RELIEF,
                  "bark_high_tris": sum(len(p.vertices) - 2 for p in hi.data.polygons),
                  "objects": sorted(o.name for o in scn.objects)}))
