"""LOD1 pass review renders: each piece's LOD0, LOD1 and LOD2 from its staged export, three views each.

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/lod1-pass/render_lods.py -- \
      target/lod1-pass <piece> [<piece> ...]

Per LOD (the other LODs and the collider hidden), PNGs into <out>/<piece>/:
- whole-lod<k>.png: orthographic three-quarter view of the whole piece (LOD0's bounds), 320 px.
- detail-lod<k>.png: orthographic three-quarter view of the parts this pass added to LOD1 (the slots
  that LOD0 has and the committed LOD1 lacked, passed as --slots per piece in SLOTS below), 320 px.
- far-lod<k>.png: the LOD1 switch distance, 150 m from the piece's centre at 28 degrees elevation,
  through a 45 degree vertical field of view on a 1920 x 1080 frame (the engine camera's default),
  cropped to the piece; compose.py magnifies the crop with nearest-neighbour pixels.
Blender's EEVEE with base colours only (no KTX2 maps, no plaster set): geometry evidence, not look-dev."""
import math
import pathlib
import sys

import bpy
from bpy_extras.object_utils import world_to_camera_view
from mathutils import Vector

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts" / "blender"))
from lib import gate, scene  # noqa: E402

# Slots added to LOD1 per piece (the detail view frames their LOD0 parts).
SLOTS = {
    "river_straight_center": ["river_cattail"],
    "river_bend_center": ["river_cattail"],
    "river_bend_60_center": ["river_cattail"],
    "river_road_bridge_center": ["river_cattail"],
    "river_rail_bridge_center": ["river_cattail"],
    "river_spring_center": ["river_cattail"],
    "river_fork_center": ["river_cattail"],
    "house_pad": ["house_pad_stone"],
    "windmill": ["windmill_iron"],
    "windmill_b": ["windmill_b_iron"],
    "bakery_shop": ["bakery_shop_wicker", "bakery_shop_bread"],
    "bakery_shop_b": ["bakery_shop_b_wicker", "bakery_shop_b_bread"],
    "rail_station_shelter": ["rail_station_coping", "rail_station_iron"],
    "rail_station_platform": ["rail_station_timber", "rail_station_board", "rail_station_flowers"],
    "well_stone": ["well_stone_stone_buff", "well_stone_mortar"],
    "well_stone_b": ["well_stone_b_stone_buff", "well_stone_b_mortar"],
}
DIR = Vector((0.8, -0.8, 0.6)).normalized()  # three-quarter view, about 28 degrees above the horizon
SWITCH_M = 150.0
FRAME = (1920, 1080)
FOV_DEG = 45.0


def lod_of(name):
    for k in range(3):
        if name.endswith(f"_lod{k}"):
            return k
    return None


def bounds(points):
    lo = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
    hi = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
    return lo, hi


def slot_points(objs, slots):
    """World positions of the LOD0 vertices on faces in `slots`, the first cluster only (within 1.5 m
    of the first such vertex), so a piece with many small parts frames one of them."""
    pts = []
    for o in objs:
        if o.type != "MESH" or lod_of(o.name) != 0:
            continue
        names = [m.name if m else "" for m in o.data.materials]
        for p in o.data.polygons:
            if names[p.material_index] in slots:
                pts += [o.matrix_world @ o.data.vertices[v].co for v in p.vertices]
    if not pts:
        return []
    first = pts[0]
    return [p for p in pts if (p - first).length < 1.5]


def render(path, size):
    s = bpy.context.scene
    s.render.resolution_x, s.render.resolution_y = size
    s.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    out, pieces = pathlib.Path(args[0]), args[1:]
    for piece in pieces:
        objs = gate.import_glb(ROOT / "target" / "kit-stage" / piece / f"{piece}.glb")
        s = bpy.context.scene
        s.render.engine = "BLENDER_EEVEE"
        s.render.use_stamp_date = False
        s.render.use_stamp_render_time = False
        s.render.film_transparent = False
        s.view_settings.view_transform = "AgX"
        s.render.image_settings.file_format = "PNG"
        world = s.world or bpy.data.worlds.new("lod1")
        s.world = world
        world.color = (0.55, 0.62, 0.7)
        sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
        sun.data.energy = 4.0
        sun.rotation_euler = (math.radians(50), 0, math.radians(30))
        scene.link(sun)
        meshes = [o for o in objs if o.type == "MESH"]
        lo, hi = bounds([o.matrix_world @ Vector(c) for o in meshes if lod_of(o.name) == 0 for c in o.bound_box])
        centre, radius = (lo + hi) / 2, (hi - lo).length / 2
        detail = slot_points(objs, SLOTS.get(piece, []))
        if detail:
            dlo, dhi = bounds(detail)
            dcentre, dradius = (dlo + dhi) / 2, max((dhi - dlo).length / 2 * 1.3, 0.9)
        else:
            dcentre, dradius = centre, radius
        cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
        scene.link(cam)
        s.camera = cam
        d = out / piece
        d.mkdir(parents=True, exist_ok=True)
        for k in range(3):
            for o in meshes:
                o.hide_render = lod_of(o.name) != k
            for view, c, r in (("whole", centre, radius), ("detail", dcentre, dradius)):
                s.render.use_border = False
                cam.data.type = "ORTHO"
                cam.data.ortho_scale = r * 2.2
                cam.data.clip_start, cam.data.clip_end = 0.01, r * 10
                cam.location = c + DIR * r * 4
                cam.rotation_euler = (c - cam.location).to_track_quat("-Z", "Y").to_euler()
                render(d / f"{view}-lod{k}.png", (320, 320))
            cam.data.type = "PERSP"
            cam.data.sensor_fit = "VERTICAL"
            cam.data.angle_y = math.radians(FOV_DEG)
            cam.data.clip_start, cam.data.clip_end = 1.0, 1000.0
            cam.location = centre + DIR * SWITCH_M
            cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
            s.render.resolution_x, s.render.resolution_y = FRAME
            bpy.context.view_layer.update()
            box = [world_to_camera_view(s, cam, o.matrix_world @ Vector(c))
                   for o in meshes if lod_of(o.name) == 0 for c in o.bound_box]
            pad = 6 / FRAME[1]
            s.render.use_border = True
            s.render.use_crop_to_border = True
            s.render.border_min_x = max(0.0, min(p.x for p in box) - pad)
            s.render.border_max_x = min(1.0, max(p.x for p in box) + pad)
            s.render.border_min_y = max(0.0, min(p.y for p in box) - pad)
            s.render.border_max_y = min(1.0, max(p.y for p in box) + pad)
            render(d / f"far-lod{k}.png", FRAME)
        print(f"lod1-pass: rendered {piece}")


main()
