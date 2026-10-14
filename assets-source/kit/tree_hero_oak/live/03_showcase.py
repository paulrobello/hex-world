# tree_hero_oak live session: full-size EEVEE showcase of the high poly with the showcase rig.
# Reloads the tileable bark set images first, so the showcase shows the set as last baked.
# REPO (the checkout root) is prepended by scripts/blender/mcp_live.py exec.
OUT = f"{REPO}/target/shots/tree-highpoly-live.png"  # noqa: F821
import bpy  # noqa: E402

for img in bpy.data.images:
    if img.filepath and "materials/bark/" in img.filepath:
        img.reload()
scn = bpy.data.scenes["tree_hero_oak"]
scn.render.resolution_percentage = 100
scn.render.filepath = OUT
scn.render.image_settings.file_format = "PNG"
win = bpy.context.window_manager.windows[0]
with bpy.context.temp_override(window=win, scene=scn):
    bpy.ops.render.render(write_still=True, scene=scn.name)
scn.render.resolution_percentage = 50
print(OUT)
