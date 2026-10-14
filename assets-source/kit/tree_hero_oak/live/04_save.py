# tree_hero_oak live session: write ONLY our scene (and its dependencies) to the local .blend store
# (owner ruling O2: the main checkout's gitignored assets-source/blend/<id>/, resolved by lib.paths)
# with bpy.data.libraries.write. The owner's session file path, dirty state and scenes are not touched,
# and the owner's "Scene" is checked intact before and after (controller D2).
import importlib
import json
import os
import sys

import bpy

# REPO (the checkout root) is prepended by scripts/blender/mcp_live.py exec.
sys.path.insert(0, f"{REPO}/scripts/blender")  # noqa: F821
from lib import paths  # noqa: E402

importlib.reload(paths)
PATH = str(paths.blend_path("tree_hero_oak"))
os.makedirs(os.path.dirname(PATH), exist_ok=True)


def owner_state():
    s = bpy.data.scenes["Scene"]
    return {"filepath": bpy.data.filepath, "objects": sorted(o.name for o in s.objects),
            "scenes": sorted(sc.name for sc in bpy.data.scenes),
            "owner_materials": len([m for m in bpy.data.materials if not m.name.startswith("tree_hero_oak")])}


before = owner_state()
scn = bpy.data.scenes["tree_hero_oak"]
bpy.data.libraries.write(PATH, {scn}, path_remap="ABSOLUTE", fake_user=False, compress=False)
after = owner_state()
assert before == after, (before, after)
print(json.dumps({"path": PATH, "bytes": os.path.getsize(PATH), "owner_before": before,
                  "owner_unchanged": before == after, "blend1": os.path.exists(PATH + "1"),
                  "objects_written": sorted(o.name for o in scn.objects)}))
