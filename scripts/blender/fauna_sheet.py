"""Textured vs-reference sheets for the boid fauna pieces (bird_starling, fish_trout).

Usage: "$BLENDER" -b --factory-startup --python-exit-code 1 -P scripts/blender/fauna_sheet.py -- <piece>

Imports the staged GLB (target/kit-stage/<piece>/), binds the stage PNGs (albedo, normal, ORM) to a
Principled material, renders EEVEE stills (Standard view transform, grey studio world, one sun) and
writes side-by-side sheets beside the approved references as JPEG q85, at most 1024 px wide, into
assets-source/kit/<piece>/renders/vs_*.jpg. Evidence only: the in-engine look is judged in engine.

bird_starling: vs_ref1_glide.jpg (ref-1 | glide pose | flap-up pose) and vs_ref3_perch.jpg (ref-3 |
perched pose, three-quarter and profile), the flap and fold computed from TEXCOORD_1 with the
contract in the generator's docstring (root extras `boid`). fish_trout: vs_ref3_palettes.jpg (ref-3 |
the ocean palette above the river palette, profile, head to the left, as in the reference). Both also
get play_distance.jpg: the piece rendered a few dozen px across and upscaled nearest-neighbour, as a
silhouette and colour check at the size boids are drawn."""
import json
import math
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, gate  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
PIECE = sys.argv[sys.argv.index("--") + 1]
STAGE = REPO / "target" / "kit-stage" / PIECE
OUT = REPO / "assets-source" / "kit" / PIECE / "renders"
TMP = REPO / "target" / "kit-renders" / PIECE / "sheet"
PANEL = 512


def material(name, uv_offset=(0.0, 0.0)):
    m = bpy.data.materials.new(name)
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = N.new("ShaderNodeTexCoord")
    mp = N.new("ShaderNodeMapping")
    mp.inputs["Location"].default_value = (uv_offset[0], uv_offset[1], 0.0)
    L.new(tc.outputs["UV"], mp.inputs["Vector"])

    def tex(ch, non_color):
        t = N.new("ShaderNodeTexImage")
        t.image = bpy.data.images.load(str(STAGE / f"{PIECE}.{ch}.png"), check_existing=True)
        t.image.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
        t.interpolation = "Cubic"
        L.new(mp.outputs["Vector"], t.inputs["Vector"])
        return t

    alb, nrm, orm = tex("albedo", False), tex("normal", True), tex("orm", True)
    sep = N.new("ShaderNodeSeparateColor")
    L.new(orm.outputs["Color"], sep.inputs["Color"])
    mul = N.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    ins = {s.identifier: s for s in mul.inputs}
    ins["Factor_Float"].default_value = 1.0
    L.new(alb.outputs["Color"], ins["A_Color"])
    L.new(sep.outputs["Red"], ins["B_Color"])
    L.new(next(s for s in mul.outputs if s.identifier == "Result_Color"), bsdf.inputs["Base Color"])
    L.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    L.new(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    nm = N.new("ShaderNodeNormalMap")
    L.new(nrm.outputs["Color"], nm.inputs["Color"])
    L.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def setup_scene():
    s = bpy.context.scene
    s.render.engine = "BLENDER_EEVEE"
    s.render.resolution_x = s.render.resolution_y = PANEL
    s.render.film_transparent = False
    s.render.use_stamp_date = False
    s.render.use_stamp_render_time = False
    s.view_settings.view_transform = "Standard"
    s.render.image_settings.file_format = "PNG"
    w = bpy.data.worlds.new("sheet_world")
    w.color = (0.42, 0.42, 0.43)
    bg = w.node_tree.nodes.get("Background")
    if bg:
        bg.inputs["Color"].default_value = (0.42, 0.42, 0.43, 1.0)
        bg.inputs["Strength"].default_value = 0.9
    s.world = w
    sun = bpy.data.objects.new("sheet_sun", bpy.data.lights.new("sheet_sun", "SUN"))
    sun.data.energy = 3.2
    sun.rotation_euler = (math.radians(40), math.radians(-25), math.radians(35))
    s.collection.objects.link(sun)
    cam = bpy.data.objects.new("sheet_cam", bpy.data.cameras.new("sheet_cam"))
    cam.data.type = "ORTHO"
    s.collection.objects.link(cam)
    s.camera = cam
    return cam


def aim(cam, target, direction, scale):
    d = Vector(direction).normalized()
    cam.location = Vector(target) + d * 2.0
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    cam.data.ortho_scale = scale
    cam.data.clip_end = 10.0


def render(path):
    bpy.context.scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)
    return bake.load_png(path)


def motion(obj):
    """Per-vertex (U, V) weights from TEXCOORD_1 (the importer flips V back, so V = 1 - v)."""
    me = obj.data
    lay = me.uv_layers[1].data
    w = np.zeros((len(me.vertices), 2))
    for loop in me.loops:
        u, v = lay[loop.index].uv
        w[loop.vertex_index] = (u, 1.0 - v)
    return w


def pose(obj, base, w, extras, mode, theta=0.0):
    """Apply the boid contract (flap or fold) to the LOD0 mesh, in Blender coordinates
    (creature x = X, forward f = -Y, up = Z)."""
    h = extras["hinge"]
    hx, hz = h["x"], h["y"]
    co = base.copy()
    wing = w[:, 0] > 0.0
    x, y, z = co[wing, 0], co[wing, 1], co[wing, 2]
    s = np.sign(x)
    dx, dz = np.abs(x) - hx, z - hz
    if mode == "flap":
        nx = hx + dx * math.cos(theta) - dz * math.sin(theta)
        nz = hz + dx * math.sin(theta) + dz * math.cos(theta)
        co[wing, 0], co[wing, 2] = s * nx, nz
    elif mode == "fold":
        k, back, r = extras["fold"]["k"], extras["fold"]["back"], math.radians(extras["fold"]["rollDeg"])
        dx, dz = dx * k, dz * k
        co[wing, 0] = s * (hx + dx * math.cos(r) - dz * math.sin(r))
        co[wing, 2] = hz + dx * math.sin(r) + dz * math.cos(r)
        co[wing, 1] = y + back * w[wing, 0]
    obj.data.vertices.foreach_set("co", co.ravel())
    obj.data.update()


def load(path):
    """An image file as float RGB (h, w, 3), top row first."""
    img = bpy.data.images.load(str(path), check_existing=False)
    img.colorspace_settings.name = "Non-Color"
    a = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(img.size[1], img.size[0], 4)[::-1, :, :3]


def resize(a, h):
    """Bilinear resize of (H, W, 3) to height h."""
    H, W = a.shape[:2]
    w = round(W * h / H)
    ys, xs = np.linspace(0, H - 1, h), np.linspace(0, W - 1, w)
    y0, x0 = np.floor(ys).astype(int), np.floor(xs).astype(int)
    y1, x1 = np.minimum(y0 + 1, H - 1), np.minimum(x0 + 1, W - 1)
    fy, fx = (ys - y0)[:, None, None], (xs - x0)[None, :, None]
    top = a[y0][:, x0] * (1 - fx) + a[y0][:, x1] * fx
    bot = a[y1][:, x0] * (1 - fx) + a[y1][:, x1] * fx
    return top * (1 - fy) + bot * fy


def sheet(panels, name):
    """Panels (top-row-first float RGB) side by side with a 6 px gap, scaled to at most 1024 wide."""
    h = PANEL
    ps = [resize(p, h) if p.shape[0] != h else p for p in panels]
    gap = np.full((h, 6, 3), 0.15, np.float32)
    row = ps[0]
    for p in ps[1:]:
        row = np.concatenate([row, gap, p], 1)
    if row.shape[1] > 1024:
        row = resize(row, h * 1024 // row.shape[1])
    hh, ww = row.shape[:2]
    rgba = np.concatenate([row[::-1], np.ones((hh, ww, 1), np.float32)], -1)
    img = bpy.data.images.new(name, ww, hh, alpha=True)
    img.colorspace_settings.name = "Non-Color"
    img.pixels.foreach_set(np.ascontiguousarray(rgba, np.float32).ravel())
    OUT.mkdir(parents=True, exist_ok=True)
    img.file_format = "JPEG"
    path = OUT / f"{name}.jpg"
    img.save(filepath=str(path), quality=85)
    print(f"sheet {path} {ww}x{hh}")


def panel(path):
    a = bake.load_png(path)[1].reshape(PANEL, PANEL, 4)[::-1, :, :3]
    return a


TINY = 64


def tiny(path, target, direction, scale):
    """A play-distance still: rendered at TINY px square (the creature a few dozen px across, as
    boids are drawn) and upscaled nearest-neighbour to PANEL. A silhouette and colour check only:
    Blender's texture filtering approximates, not reproduces, the KTX2 mips."""
    s = bpy.context.scene
    s.render.resolution_x = s.render.resolution_y = TINY
    aim(cam, target, direction, scale)
    a = render(path)[1].reshape(TINY, TINY, 4)[::-1, :, :3]
    s.render.resolution_x = s.render.resolution_y = PANEL
    k = PANEL // TINY
    return np.repeat(np.repeat(a, k, 0), k, 1)


TMP.mkdir(parents=True, exist_ok=True)
objs = gate.import_glb(str(STAGE / f"{PIECE}.glb"))
root = next(o for o in objs if o.name == PIECE)
extras = json.loads(root["boid"])
lod0 = next(o for o in objs if o.name.endswith("_lod0"))
for o in objs:
    if o.type == "MESH" and o is not lod0:
        o.hide_render = True
cam = setup_scene()
refs = REPO / "assets-source" / "refs"
if PIECE == "bird_starling":
    lod0.data.materials.clear()
    lod0.data.materials.append(material("sheet_bird"))
    base = np.empty(len(lod0.data.vertices) * 3)
    lod0.data.vertices.foreach_get("co", base)
    base = base.reshape(-1, 3)
    w = motion(lod0)
    view = (0.75, -0.55, -0.18)
    # Play distance: the 0.40 m span about 40 px across, seen from the tilted strategic camera.
    far = [tiny(TMP / "far_top.png", (0.0, 0.0, 0.0), (0.35, 0.25, 1.0), 0.64),
           tiny(TMP / "far_side.png", (0.0, 0.0, 0.0), (0.8, -0.4, 0.35), 0.64)]
    aim(cam, (0.0, 0.0, 0.01), view, 0.36)
    render(TMP / "glide.png")
    pose(lod0, base, w, extras, "flap", math.radians(40))
    render(TMP / "flap.png")
    pose(lod0, base, w, extras, "fold")
    far.append(tiny(TMP / "far_perch.png", (0.0, 0.01, 0.0), (0.8, -0.4, 0.5), 0.64))
    sheet(far, "play_distance")
    aim(cam, (0.0, 0.01, 0.005), (0.8, -0.45, 0.30), 0.26)
    render(TMP / "perch.png")
    aim(cam, (0.0, 0.01, 0.005), (1.0, 0.0, 0.05), 0.26)
    render(TMP / "perch_side.png")
    sheet([load(refs / "birds" / "ref-1.jpg"), panel(TMP / "glide.png"), panel(TMP / "flap.png")], "vs_ref1_glide")
    sheet([load(refs / "birds" / "ref-3.jpg"), panel(TMP / "perch.png"), panel(TMP / "perch_side.png")],
          "vs_ref3_perch")
else:
    lod0.data.materials.clear()
    lod0.data.materials.append(material("sheet_ocean"))
    river = lod0.copy()
    river.data = lod0.data.copy()
    river.data.materials.clear()
    river.data.materials.append(material("sheet_river", (0.5, 0.0)))
    bpy.context.scene.collection.objects.link(river)
    river.location = (0.0, 0.0, -0.17)
    lod0.location = (0.0, 0.0, 0.0)
    aim(cam, (0.0, 0.0, -0.085), (1.0, 0.0, 0.0), 0.52)
    render(TMP / "palettes.png")
    sheet([load(refs / "fish" / "ref-3.jpg"), panel(TMP / "palettes.png")], "vs_ref3_palettes")
    # Play distance: each 0.40 m fish about 40 px long, from the tilted strategic camera and the side.
    river.location = (0.0, 0.0, -0.17)
    sheet([tiny(TMP / "far_top.png", (0.0, 0.0, -0.085), (0.3, 0.2, 1.0), 0.64),
           tiny(TMP / "far_side.png", (0.0, 0.0, -0.085), (1.0, -0.2, 0.25), 0.64)], "play_distance")
