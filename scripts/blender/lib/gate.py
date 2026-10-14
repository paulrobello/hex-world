"""Blender-side gate helpers: six-view evidence renders and a socket round-trip check.
Renders are evidence only; look-dev approval happens in engine (brief §(e) look-dev parity)."""
import json
import math
import pathlib

import bpy
from mathutils import Vector

from . import scene

VIEWS = {"front": (0, -1, 0.35), "back": (0, 1, 0.35), "left": (-1, 0, 0.35), "right": (1, 0, 0.35),
         "top": (0, 0, 1), "three_quarter": (0.8, -0.8, 0.6)}


def import_glb(path):
    scene.reset_scene("gate")
    bpy.ops.import_scene.gltf(filepath=str(path))
    return list(bpy.context.scene.objects)


JPEG_QUALITY = 85
JPEG_MAX_PX = 1024


def _to_jpeg(png, jpg):
    """Committed evidence is JPEG q85 at most 1024 px wide; the lossless PNG stays under target/."""
    img = bpy.data.images.load(str(png), check_existing=False)
    w, h = img.size
    if w > JPEG_MAX_PX:
        img.scale(JPEG_MAX_PX, round(h * JPEG_MAX_PX / w))
    img.file_format = "JPEG"
    img.save(filepath=str(jpg), quality=JPEG_QUALITY)
    bpy.data.images.remove(img)


def render_six_views(objs, out_dir, png_dir, size=1024):
    """Renders six lossless PNGs into png_dir (keep it under target/), then writes the committed
    JPEGs into out_dir. Returns the JPEG paths."""
    s = bpy.context.scene
    s.render.engine = "BLENDER_EEVEE"
    s.render.resolution_x = s.render.resolution_y = size
    # Blender writes the wall-clock date and render time into every PNG/JPEG as metadata; with them
    # off, a re-render of an unchanged GLB is byte-identical and does not churn the committed JPEGs.
    s.render.use_stamp_date = False
    s.render.use_stamp_render_time = False
    s.view_settings.view_transform = "AgX"
    world = s.world or bpy.data.worlds.new("gate")
    s.world = world
    world.color = (0.18, 0.18, 0.2)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.0
    sun.rotation_euler = (math.radians(50), 0, math.radians(30))
    scene.link(sun)
    meshes = [o for o in objs if o.type == "MESH" and o.name.endswith("_lod0")] or [o for o in objs if o.type == "MESH"]
    pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    center, radius = (lo + hi) / 2, (hi - lo).length / 2
    for o in objs:
        if o.type == "MESH":
            o.hide_render = not o.name.endswith("_lod0")
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = radius * 2.2
    cam.data.clip_end = radius * 10
    scene.link(cam)
    s.camera = cam
    out = pathlib.Path(out_dir)
    png = pathlib.Path(png_dir)
    out.mkdir(parents=True, exist_ok=True)
    png.mkdir(parents=True, exist_ok=True)
    s.render.image_settings.file_format = "PNG"
    for name, d in VIEWS.items():
        cam.location = center + Vector(d).normalized() * radius * 4
        cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()
        s.render.filepath = str(png / f"{name}.png")
        bpy.ops.render.render(write_still=True)
        _to_jpeg(png / f"{name}.png", out / f"{name}.jpg")
    return sorted(str(out / f"{name}.jpg") for name in VIEWS)


def reimport_check(glb_path, export_report_path, tol=0.001):
    """Socket names/positions survive export -> import within tol (par-factory verify-modular-machine-bodies.py:25).
    The importer converts back to Z-up, so positions compare in the runtime frame (x, z, -y)."""
    rep = json.loads(pathlib.Path(export_report_path).read_text())
    objs = {o.name: o for o in import_glb(glb_path)}
    bpy.context.view_layer.update()
    errors = [f"node {n} missing after reimport" for n in rep["nodes"] if n not in objs]
    for s in rep["sockets"]:
        o = objs.get(s["name"])
        if o is None:
            continue
        p = o.matrix_world.translation
        rt = (p.x, p.z, -p.y)
        if max(abs(a - b) for a, b in zip(rt, s["position"])) > tol:
            errors.append(f"{s['name']} at {rt}, exported at {s['position']}")
    if errors:
        raise AssertionError("; ".join(errors))
