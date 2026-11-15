"""Textured headless EEVEE stills of a kit piece (evidence only: Blender's AgX view transform is not
Bevy's tonemapper; approval happens in engine).

Each piece's LOD0 is imported from its GLB with its slots bound the way kit_material.wesl draws
them: a tileable slot samples the world-space tileable set (box-projected on object coordinates at the
set's tileM, an approximation of `tile_frame`) times its manifest colour, a colour slot takes its
manifest colour and roughness, and every slot's diffuse is multiplied by the piece's masks.A (baked
AO, on UV0) when the masks PNG exists.

New owner checkpoints call this instead of copying a `still.py` into `docs/checkpoints/<piece>/`:

    blender -b --factory-startup --python-exit-code 1 -P scripts/blender/lib/still.py -- <piece> <out dir>

or `from lib import still; still.main(piece, out_dir)` from a script. It writes `<out>/<piece>-hero.png`
(three-quarter front, sun side) and `<piece>-back.png` (three-quarter back, shade side), framed from
the piece's own bounds. Pieces whose slots use another binding (foliage with its own albedo, ORM and
normal images, an animated paint, a scale shot beside other pieces) keep a script of their own; the
committed `docs/checkpoints/*/still.py` files stay as the record of how each frame was made.
"""
from __future__ import annotations

import json
import math
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
KIT = ROOT / "assets/kit"
MANIFEST = KIT / "manifest.json"
RESOLUTION = (1600, 1000)
# The kit sun (lighting.rs KIT_SUN_DIR, runtime (0.553, 0.742, 0.379)) in Blender's frame.
SUN_DIR = (0.553, -0.379, 0.742)
SUN_ENERGY = 4.5
SUN_ANGLE_DEG = 2.0
GROUND_SIZE = 120.0
GROUND_DROP = 0.02
GROUND_COLOR = (0.13, 0.22, 0.06, 1.0)
SKY_COLOR = (0.45, 0.55, 0.7)
CAMERA_LENS = 40.0
# Camera offset from the piece's centre, per view: (x, y) in multiples of its largest horizontal
# extent, z in multiples of its height, and the height the camera looks at as a fraction of it. Tuned
# on barn_red against its hand-placed still (camera (19, -13, 9) looking at (0, 0, 3), a 10.7 m wide,
# 7.8 m tall barn).
VIEWS = {
    "hero": (1.8, -1.2, 1.15, 0.4),
    "back": (-1.7, 1.3, 1.05, 0.4),
}
BOX_BLEND = 0.15


def image(path: pathlib.Path, non_color: bool):
    """The PNG at `path`, tagged sRGB or Non-Color."""
    import bpy

    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def _mix(nt, blend: str):
    """A colour Mix node of `blend` type at full factor, with its sockets by identifier."""
    node = nt.nodes.new("ShaderNodeMix")
    node.data_type = "RGBA"
    node.blend_type = blend
    sockets = {x.identifier: x for x in node.inputs}
    sockets["Factor_Float"].default_value = 1.0
    return node, sockets


def bind(mat, slot: dict, sets: dict, masks_png) -> None:
    """Binds one manifest `slot` on `mat` the way kit_material.wesl draws it (see the module doc)."""
    mat.use_backface_culling = True  # kit slots without alphaCutoff are culled single-sided at runtime
    nt = mat.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    col = None
    if "tileable" in slot:
        st = sets[slot["tileable"]]
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        k = 1.0 / st["tileM"]
        mp.inputs["Scale"].default_value = (k, k, k)
        nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
        alb = nt.nodes.new("ShaderNodeTexImage")
        alb.image = image(KIT / f"{st['maps']['albedo']}.png", False)
        alb.projection = "BOX"
        alb.projection_blend = BOX_BLEND
        nt.links.new(mp.outputs["Vector"], alb.inputs["Vector"])
        tint, ins = _mix(nt, "MULTIPLY")
        nt.links.new(alb.outputs["Color"], ins["A_Color"])
        ins["B_Color"].default_value = (*slot.get("color", (1, 1, 1)), 1.0)
        col = next(x for x in tint.outputs if x.identifier == "Result_Color")
        orm = nt.nodes.new("ShaderNodeTexImage")
        orm.image = image(KIT / f"{st['maps']['orm']}.png", True)
        orm.projection = "BOX"
        orm.projection_blend = BOX_BLEND
        nt.links.new(mp.outputs["Vector"], orm.inputs["Vector"])
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
        nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    else:
        bsdf.inputs["Roughness"].default_value = slot.get("roughness", 0.8)
    if masks_png:
        uvn = nt.nodes.new("ShaderNodeUVMap")
        uvn.uv_map = "UVMap"
        mk = nt.nodes.new("ShaderNodeTexImage")
        mk.image = image(masks_png, True)
        nt.links.new(uvn.outputs["UV"], mk.inputs["Vector"])
        occl, ins = _mix(nt, "MULTIPLY")
        if col is None:
            ins["A_Color"].default_value = (*slot["color"], 1.0)
        else:
            nt.links.new(col, ins["A_Color"])
        nt.links.new(mk.outputs["Alpha"], ins["B_Color"])
        col = next(x for x in occl.outputs if x.identifier == "Result_Color")
    if col is None:
        bsdf.inputs["Base Color"].default_value = (*slot["color"], 1.0)
    else:
        nt.links.new(col, bsdf.inputs["Base Color"])


def load(manifest_path: pathlib.Path = MANIFEST) -> tuple[dict, dict]:
    """(pieces by id, tileable sets by name) from the kit manifest."""
    man = json.loads(pathlib.Path(manifest_path).read_text())
    return {p["id"]: p for p in man["pieces"]}, man["materials"]


def masks_of(piece: dict) -> pathlib.Path | None:
    """The piece's masks PNG, or None when it has none on disk."""
    p = KIT / f"{piece['maps']['masks']}.png"
    return p if p.exists() else None


def place(piece_id: str, pieces: dict, sets: dict, at=(0.0, 0.0, 0.0)):
    """Imports the piece's GLB at `at`, keeps its LOD0 only, binds its slots, returns the LOD0 object."""
    import bpy
    from mathutils import Vector

    piece = pieces[piece_id]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(KIT / piece["glb"]))
    for o in [o for o in bpy.data.objects if o not in before]:
        if o.type == "MESH" and o.name != f"{piece_id}_lod0":
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    root = next(o for o in bpy.data.objects if o not in before and o.parent is None)
    root.location = Vector(at)
    bpy.context.view_layer.update()
    lod0 = bpy.data.objects[f"{piece_id}_lod0"]
    masks = masks_of(piece)
    for s in lod0.material_slots:
        name = s.material.name.split(".")[0]
        if name in piece["slots"]:
            bind(s.material, piece["slots"][name], sets, masks)
    return lod0


def scene_setup():
    """An empty EEVEE scene with the kit sun, a ground plane and a camera; returns (scene, camera)."""
    import bpy
    from mathutils import Vector

    bpy.ops.wm.read_factory_settings(use_empty=True)
    scn = bpy.context.scene
    scn.render.engine = "BLENDER_EEVEE"
    scn.render.resolution_x, scn.render.resolution_y = RESOLUTION
    scn.render.use_stamp_date = False
    scn.render.use_stamp_render_time = False
    scn.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("still")
    world.color = SKY_COLOR
    scn.world = world
    bpy.ops.mesh.primitive_plane_add(size=GROUND_SIZE, location=(0, 0, -GROUND_DROP))
    ground = bpy.context.object
    gm = bpy.data.materials.new("ground")
    next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED").inputs["Base Color"].default_value = GROUND_COLOR
    ground.data.materials.append(gm)
    sun_dir = Vector(SUN_DIR).normalized()
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = SUN_ENERGY
    sun.data.angle = math.radians(SUN_ANGLE_DEG)
    sun.rotation_euler = (-sun_dir).to_track_quat("-Z", "Y").to_euler()
    scn.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = CAMERA_LENS
    scn.collection.objects.link(cam)
    scn.camera = cam
    return scn, cam


def camera_pose(view: str, size, height: float):
    """(location, look-at) for `view` of a piece `size` wide (its largest horizontal extent) and `height` tall."""
    from mathutils import Vector

    x, y, z, look = VIEWS[view]
    return Vector((x * size, y * size, z * height)), Vector((0.0, 0.0, look * height))


def main(piece_id: str, out_dir: str | pathlib.Path, views=tuple(VIEWS)) -> list[pathlib.Path]:
    """Renders `piece_id` from `views` to `<out_dir>/<piece>-<view>.png` and returns the paths."""
    import bpy
    from mathutils import Vector

    pieces, sets = load()
    if piece_id not in pieces:
        raise SystemExit(f"still: {piece_id!r} is not in {MANIFEST}")
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    scn, cam = scene_setup()
    obj = place(piece_id, pieces, sets)
    corners = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    size = max(max(c.x for c in corners) - min(c.x for c in corners), max(c.y for c in corners) - min(c.y for c in corners))
    height = max(c.z for c in corners)
    written = []
    for view in views:
        loc, target = camera_pose(view, size, height)
        cam.location = loc
        cam.rotation_euler = (target - loc).to_track_quat("-Z", "Y").to_euler()
        path = out / f"{piece_id}-{view}.png"
        scn.render.filepath = str(path)
        bpy.ops.render.render(write_still=True)
        written.append(path)
        print("still written", path)
    return written


if __name__ == "__main__" and "--" in sys.argv:
    args = sys.argv[sys.argv.index("--") + 1:]
    if len(args) != 2:
        raise SystemExit("usage: blender -b --factory-startup --python-exit-code 1 -P scripts/blender/lib/still.py -- <piece> <out dir>")
    main(args[0], args[1])
