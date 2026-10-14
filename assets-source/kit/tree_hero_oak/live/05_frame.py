# tree_hero_oak live session: frame the whole tree (12.5 m crown) in the showcase camera and the
# owner's viewport. Live-only: the camera is neither a bake input nor in the export root, so the
# stored .blend (saved by 04_save.py with the earlier, crown-cropping framing) is not re-saved.
import bpy
from mathutils import Vector

scn = bpy.data.scenes["tree_hero_oak"]
cam = bpy.data.objects["tree_hero_oak_cam"]
cam.data.lens = 40
cam.location = (16.5, -21.5, 6.0)
cam.rotation_euler = (Vector((0.3, 0.0, 6.2)) - cam.location).to_track_quat("-Z", "Y").to_euler()
for area in bpy.context.window_manager.windows[0].screen.areas:
    if area.type == "VIEW_3D":
        area.spaces.active.shading.type = "RENDERED"
        area.spaces.active.region_3d.view_perspective = "CAMERA"
print({"lens": cam.data.lens, "location": tuple(round(x, 2) for x in cam.location)})
