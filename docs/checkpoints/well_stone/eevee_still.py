"""Headless textured EEVEE stills of the staged well_stone or well_stone_b (the six-view render is
untextured), and for LOD0 a scale still of both wells beside the committed house_cottage.

Run after `make kit-build PIECE=<piece>`:
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/well_stone/eevee_still.py -- <out_dir> <piece> [lod]
Writes <out_dir>/<piece>-eevee-{three-quarter,front,side,top}.jpg (LOD1/LOD2 suffixed -lod<n>, first three
only) and, at LOD0, well-eevee-scale.jpg. Tileable slots sample the plaster set box-projected from world
position at tileM 2 m, tinted by the manifest colour (as kit_material.wesl does); colour slots take the
manifest colour. The masks map is bound on UV0: A darkens the base colour by 60% (the runtime applies it
to ambient light only, so this overstates it in direct sun). Blender's view transform is not Bevy's:
evidence only. From docs/checkpoints/house_twostorey_c/eevee_still.py."""
import json
import math
import pathlib
import sys

import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
args = sys.argv[sys.argv.index("--") + 1:]
out = pathlib.Path(args[0])
P = args[1]
lod = int(args[2]) if len(args) > 2 else 0
out.mkdir(parents=True, exist_ok=True)
KIT = REPO / "assets/kit"
MATS = KIT / "p1/materials"
MANIFEST = json.loads((KIT / "manifest.json").read_text())
PIECES = {p["id"]: p for p in MANIFEST["pieces"]}
# The wells' manifest tints (they may not be in the manifest yet when the still runs).
WELL_TILE = {"stone": (0.47, 0.46, 0.43), "stone_buff": (0.60, 0.50, 0.37), "mortar": (0.84, 0.79, 0.67),
             "cap": (0.72, 0.70, 0.66), "roof": (0.50, 0.19, 0.095)}

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 1050
scn.render.image_settings.file_format = "JPEG"
scn.render.image_settings.quality = 85
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
scn.world = world
bg = world.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.5, 0.52, 0.55, 1.0)
bg.inputs[1].default_value = 0.7
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.5
sun.data.angle = math.radians(2.0)
sun.rotation_euler = (math.radians(40), 0.0, math.radians(120))
scn.collection.objects.link(sun)
plaster = bpy.data.images.load(str(MATS / "plaster/plaster.albedo.png"))


def masks_image(path):
    if path.exists():
        img = bpy.data.images.load(str(path))
    else:  # a HEXW_SKIP_BAKE=1 stage: neutral masks (no AO, no night)
        img = bpy.data.images.new("masks", 4, 4, alpha=True)
        img.pixels.foreach_set([1.0, 0.5, 0.0, 1.0] * 16)
    img.colorspace_settings.name = "Non-Color"
    return img


def material(piece, slot, src, masks_img, tile):
    mat = bpy.data.materials.new(f"still_{slot}")
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    sb = src.node_tree.nodes["Principled BSDF"] if src and src.node_tree else None
    rough = sb.inputs["Roughness"].default_value if sb else 0.8
    name = slot[len(piece) + 1:]
    if name in tile:
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        mapping = nt.nodes.new("ShaderNodeMapping")
        mapping.inputs["Scale"].default_value = (0.5, 0.5, 0.5)
        nt.links.new(geo.outputs["Position"], mapping.inputs["Vector"])
        img = nt.nodes.new("ShaderNodeTexImage")
        img.image = plaster
        img.projection = "BOX"
        img.projection_blend = 0.2
        nt.links.new(mapping.outputs["Vector"], img.inputs["Vector"])
        tint = nt.nodes.new("ShaderNodeMix")
        tint.data_type = "RGBA"
        tint.blend_type = "MULTIPLY"
        tint.inputs["Factor"].default_value = 1.0
        nt.links.new(img.outputs["Color"], tint.inputs[6])
        tint.inputs[7].default_value = (*tile[name], 1.0)
        base = tint.outputs[2]
    else:
        rgb = nt.nodes.new("ShaderNodeRGB")
        rgb.outputs[0].default_value = tuple(sb.inputs["Base Color"].default_value) if sb else (0.5, 0.5, 0.5, 1)
        base = rgb.outputs[0]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mk = nt.nodes.new("ShaderNodeTexImage")
    mk.image = masks_img
    nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
    alpha = nt.nodes.new("ShaderNodeMapRange")
    alpha.inputs["To Min"].default_value = 0.4
    nt.links.new(mk.outputs["Alpha"], alpha.inputs["Value"])
    ao = nt.nodes.new("ShaderNodeMix")
    ao.data_type = "RGBA"
    ao.blend_type = "MULTIPLY"
    ao.inputs["Factor"].default_value = 1.0
    nt.links.new(base, ao.inputs[6])
    nt.links.new(alpha.outputs["Result"], ao.inputs[7])
    nt.links.new(ao.outputs[2], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = rough
    return mat


def place(piece, glb, masks_png, tile, at=(0.0, 0.0, 0.0), yaw=0.0, show_lod=0):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(glb))
    new = [o for o in bpy.data.objects if o not in before]
    masks_img = masks_image(masks_png)
    for o in new:
        if o.parent is None:
            o.location = at
            o.rotation_mode = "XYZ"
            o.rotation_euler = (0.0, 0.0, yaw)
        if o.type != "MESH":
            continue
        o.hide_render = show_lod is None or f"_lod{show_lod}" not in o.name
        for s in o.material_slots:
            if s.material:
                s.material = material(piece, s.material.name.split(".")[0], s.material, masks_img, tile)
    return new


def committed_tile(piece):
    return {k[len(piece) + 1:]: tuple(v["color"]) for k, v in PIECES[piece]["slots"].items() if "tileable" in v}


def stage(piece):
    d = REPO / "target/kit-stage" / piece
    if (d / f"{piece}.glb").exists():
        return d / f"{piece}.glb", d / f"{piece}.masks.png"
    if piece not in PIECES:
        return None
    return KIT / PIECES[piece]["glb"], KIT / (PIECES[piece]["maps"]["masks"] + ".png")


glb, masks = stage(P)
own = place(P, glb, masks, WELL_TILE, show_lod=lod)

ground = bpy.data.objects.new("ground", bpy.data.meshes.new("ground"))
ground.data.from_pydata([(-24, -24, -0.02), (24, -24, -0.02), (24, 24, -0.02), (-24, 24, -0.02)], [], [(0, 1, 2, 3)])
gm = bpy.data.materials.new("ground")
gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.09, 0.16, 0.05, 1.0)
ground.data.materials.append(gm)
scn.collection.objects.link(ground)


def camera(name, loc, look_at, lens=40.0, ortho=None):
    cam = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    cam.data.lens = lens
    if ortho:
        cam.data.type = "ORTHO"
        cam.data.ortho_scale = ortho
    cam.location = loc
    d = [look_at[i] - loc[i] for i in range(3)]
    cam.rotation_euler = (math.atan2(math.hypot(d[0], d[1]), -d[2]), 0.0, math.atan2(d[1], d[0]) - math.pi / 2)
    scn.collection.objects.link(cam)
    return cam


def shoot(cam, name):
    scn.camera = cam
    scn.render.filepath = str(out / f"{name}.jpg")
    bpy.ops.render.render(write_still=True)


sfx = "" if lod == 0 else f"-lod{lod}"
shoot(camera("c34", (4.6, 3.2, 3.0), (0.0, 0.0, 1.0), 40.0), f"{P}-eevee-three-quarter{sfx}")
shoot(camera("cf", (5.2, 0.0, 1.6), (0.0, 0.0, 1.05), 40.0), f"{P}-eevee-front{sfx}")
shoot(camera("cs", (1.2, -5.0, 1.8), (0.0, 0.0, 1.1), 40.0), f"{P}-eevee-side{sfx}")
if lod == 0:
    shoot(camera("ct", (0.0, 0.01, 12.0), (0.0, 0.0, 0.0), 40.0, ortho=3.4), f"{P}-eevee-top")
    other = "well_stone_b" if P == "well_stone" else "well_stone"
    for o in own:
        if o.parent is None:
            o.location = (0.0, -1.8, 0.0)
    if stage(other):
        place(other, *stage(other), WELL_TILE, at=(0.0, 1.8, 0.0))
    place("house_cottage", KIT / PIECES["house_cottage"]["glb"],
          KIT / (PIECES["house_cottage"]["maps"]["masks"] + ".png"), committed_tile("house_cottage"),
          at=(-5.5, 0.0, 0.0))
    shoot(camera("cscale", (11.0, 6.0, 4.0), (-2.0, 0.0, 1.6), 32.0), "well-eevee-scale")
