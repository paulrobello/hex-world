# tree_hero_oak live session: EEVEE still of our scene through tree_hero_oak_cam, for self-review.
import bpy

# REPO (the checkout root) is prepended by scripts/blender/mcp_live.py exec.
OUT = globals().get("OUT", f"{REPO}/target/live/review.png")  # noqa: F821
scn = bpy.data.scenes["tree_hero_oak"]
scn.render.filepath = OUT
scn.render.image_settings.file_format = "PNG"
scn.render.resolution_percentage = 50
win = bpy.context.window_manager.windows[0]
with bpy.context.temp_override(window=win, scene=scn):
    bpy.ops.render.render(write_still=True, scene=scn.name)
print(OUT)
