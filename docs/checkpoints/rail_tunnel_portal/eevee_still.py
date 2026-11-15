"""Headless textured EEVEE stills of rail_tunnel_portal on a stand-in of its hillside (evidence only: Blender's
view transform is not Bevy's, and the game draws the real ground, grass, track and trains).

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_tunnel_portal/eevee_still.py -- <out_dir> [views] [lod]

Imports the staged GLB (target/kit-stage/rail_tunnel_portal/, after `make kit-build`) and binds its baked
maps as the road portal's eevee_still.py does: the stone and fern slots take albedo, the tangent normal and
ORM (AO darkens the colour to 40% at 0, which overstates it in sun), the ferns clip at alpha 0.5, the lining
and back take masks.A, the turf is a flat grass tone (the runtime samples the world-space grass set and grows
blades on it). views: comma list from three_quarter, ref2, ref3, front, eye, close, top, night, train
(default all); lod 0, 1 or 2 shows that LOD alone. Writes <out_dir>/portal-<view>[-lod<n>].jpg.

The stand-in: the portal tile's ground from _tunnel_portal.cut_height_at (grass), a ballast bed, sleepers and
two rails (rail top 0.46 m, _rail.py's profile) from the edge-3 neighbour up to the interior back plane, the
cover meadow beyond edge 0 and a raised meadow beyond edge 1 at 4 m, flat meadows beyond edges 3 and 4 at 0.
The `train` view stands the committed train_engine and train_carriage_b LOD0 meshes on the rails, the
engine's front 1 m inside the face."""
import math
import pathlib
import sys

import bmesh
import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))

import _tunnel_portal_rail  # noqa: E402,F401  (sets the rail globals on _tunnel_portal)
import _tunnel_portal as tp  # noqa: E402
from lib import hexgeo, materials  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
out = pathlib.Path(args[0])
ALL = ["three_quarter", "ref2", "ref3", "front", "eye", "close", "top", "night", "train"]
views = args[1].split(",") if len(args) > 1 and args[1] else ALL
show_lod = int(args[2]) if len(args) > 2 else 0
out.mkdir(parents=True, exist_ok=True)
P = "rail_tunnel_portal"
STAGE = REPO / "target/kit-stage" / P

bpy.ops.wm.read_factory_settings(use_empty=True)
scn = bpy.context.scene
scn.render.engine = "BLENDER_EEVEE"
scn.render.resolution_x, scn.render.resolution_y = 1400, 1000
scn.render.image_settings.file_format = "JPEG"
scn.render.image_settings.quality = 85
scn.render.use_stamp_date = False
scn.render.use_stamp_render_time = False
scn.view_settings.view_transform = "AgX"
world = bpy.data.worlds.new("still")
scn.world = world
bg = world.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.52, 0.56, 0.62, 1.0)
bg.inputs[1].default_value = 0.8
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = 3.5
sun.data.angle = math.radians(2.0)
scn.collection.objects.link(sun)


def flat(name, rgb, rough=0.9):
    return materials.pbr_material(name, (*rgb, 1.0), rough)


GRASS, SOIL = flat("grass", (0.10, 0.20, 0.05)), flat("soil", (0.12, 0.08, 0.05))
BALLAST, SLEEPER = flat("ballast", (0.20, 0.20, 0.21)), flat("sleeper", (0.075, 0.056, 0.042), 0.85)
STEEL, HEAD = flat("steel", (0.13, 0.085, 0.06), 0.6), flat("head", (0.56, 0.56, 0.54), 0.28)


def rt(x, z, y):
    """Blender point of runtime (x, y, z)."""
    return (x, -z, y)


def tile(cx, cz, height, n=24):
    """A hex tile centred at runtime (cx, cz), its cap from `height(x, z)` (tile-local) on a
    barycentric grid of n rows per sector, with a soil skirt down to y = -1 (the stand-in column)."""
    bm = bmesh.new()
    for e in range(6):
        a0, a1 = math.radians(30 + 60 * (e - 1)), math.radians(30 + 60 * e)
        ca = (hexgeo.HEX_R * math.cos(a0), hexgeo.HEX_R * math.sin(a0))
        cb = (hexgeo.HEX_R * math.cos(a1), hexgeo.HEX_R * math.sin(a1))
        grid = {}
        for i in range(n + 1):
            for j in range(i + 1):
                x = ca[0] * i / n + (cb[0] - ca[0]) * j / n
                z = ca[1] * i / n + (cb[1] - ca[1]) * j / n
                grid[i, j] = bm.verts.new(rt(cx + x, cz + z, height(x, z)))
        for i in range(n):
            for j in range(i + 1):
                bm.faces.new((grid[i, j], grid[i + 1, j], grid[i + 1, j + 1]))
                if j < i:
                    bm.faces.new((grid[i, j], grid[i + 1, j + 1], grid[i, j + 1]))
        for j in range(n):
            a, b = grid[n, j], grid[n, j + 1]
            la = bm.verts.new((a.co.x, a.co.y, -1.0))
            lb = bm.verts.new((b.co.x, b.co.y, -1.0))
            bm.faces.new((a, la, lb, b))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    for f in bm.faces:
        f.normal_update()
        c = f.calc_center_median()
        if abs(f.normal.z) > 0.05:
            if f.normal.z < 0:
                f.normal_flip()
        elif (c.x - cx) * f.normal.x + (c.y + cz) * f.normal.y < 0:
            f.normal_flip()
    me = bpy.data.meshes.new("tile")
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new("tile", me)
    me.materials.append(GRASS)
    me.materials.append(SOIL)
    for poly in me.polygons:
        poly.material_index = 0 if poly.normal.z > 0.2 else 1
        poly.use_smooth = True
    scn.collection.objects.link(o)
    return o


def boxes(name, mat, items):
    """Axis-aligned boxes, each runtime (x0, x1, y0, y1, z0, z1)."""
    bm = bmesh.new()
    for x0, x1, y0, y1, z0, z1 in items:
        v = [bm.verts.new(rt(x, z, y)) for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]
        for q in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (1, 5, 7, 3), (0, 2, 6, 4)):
            bm.faces.new([v[i] for i in q])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(mat)
    scn.collection.objects.link(bpy.data.objects.new(name, me))


A2 = 2 * hexgeo.APOTHEM
tile(0.0, 0.0, tp.cut_height_at)
for e, h in ((0, 4.0), (1, 4.0), (3, 0.0), (4, 0.0)):
    a = math.radians(60 * e)
    tile(A2 * math.cos(a), A2 * math.sin(a), lambda x, z, h=h: h)
X0 = -A2 - 4.0
boxes("ballast", BALLAST, [(X0, tp.X_B, -0.3, 0.22, -1.75, 1.75), (X0, tp.X_B, -0.3, 0.1, -2.5, -1.75),
                           (X0, tp.X_B, -0.3, 0.1, 1.75, 2.5)])
n_sl = int((tp.X_B - X0) / 0.7)
boxes("sleepers", SLEEPER, [(X0 + 0.7 * i - 0.13, X0 + 0.7 * i + 0.13, 0.18, 0.30, -1.3, 1.3) for i in range(n_sl)])
for s in (-1, 1):
    boxes("rail", STEEL, [(X0, tp.X_B, 0.30, 0.42, s * 0.84 - 0.03, s * 0.84 + 0.03)])
    boxes("head", HEAD, [(X0, tp.X_B, 0.42, 0.46, s * 0.84 - 0.04, s * 0.84 + 0.04)])

# The piece and its maps.
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(STAGE / f"{P}.glb"))
for o in [o for o in bpy.data.objects if o not in before]:
    if o.name.endswith("_collider") or (o.name.startswith(f"{P}_lod") and o.name != f"{P}_lod{show_lod}"):
        o.hide_render = True


def image(channel, colour):
    img = bpy.data.images.load(str(STAGE / f"{P}.{channel}.png"))
    img.colorspace_settings.name = "sRGB" if colour else "Non-Color"
    return img


IMG = {c: image(c, c == "albedo") for c in ("albedo", "normal", "orm", "masks")}


def tex(nt, img, uvn):
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = img
    nt.links.new(uvn.outputs["UV"], t.inputs["Vector"])
    return t


def ao_mul(nt, base, ao_socket):
    rng = nt.nodes.new("ShaderNodeMapRange")
    rng.inputs["To Min"].default_value = 0.4
    nt.links.new(ao_socket, rng.inputs["Value"])
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type, mul.blend_type = "RGBA", "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    if isinstance(base, tuple):
        mul.inputs[6].default_value = base
    else:
        nt.links.new(base, mul.inputs[6])
    nt.links.new(rng.outputs["Result"], mul.inputs[7])
    return mul.outputs[2]


GLASS = []
for m in [m for m in bpy.data.materials if m.name.startswith(P)]:
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    base = tuple(bsdf.inputs["Base Color"].default_value)
    slot = m.name[len(P) + 1:]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    if slot in ("stone", "fern"):
        alb, orm = tex(nt, IMG["albedo"], uvn), tex(nt, IMG["orm"], uvn)
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs[0])
        nt.links.new(ao_mul(nt, alb.outputs["Color"], sep.outputs[0]), bsdf.inputs["Base Color"])
        nt.links.new(sep.outputs[1], bsdf.inputs["Roughness"])
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(tex(nt, IMG["normal"], uvn).outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        if slot == "fern":
            clip = nt.nodes.new("ShaderNodeMath")
            clip.operation = "GREATER_THAN"
            clip.inputs[1].default_value = 0.5
            nt.links.new(alb.outputs["Alpha"], clip.inputs[0])
            nt.links.new(clip.outputs[0], bsdf.inputs["Alpha"])
            m.surface_render_method = "DITHERED"
            m.use_backface_culling = False
    elif slot == "turf":
        bsdf.inputs["Base Color"].default_value = (0.10, 0.20, 0.05, 1.0)
    elif slot in ("lining", "back"):
        mk = tex(nt, IMG["masks"], uvn)
        mk.interpolation = "Closest"
        nt.links.new(ao_mul(nt, base, mk.outputs["Alpha"]), bsdf.inputs["Base Color"])
    elif slot == "glass":
        GLASS.append((bsdf, base))

# The train view's vehicles: the engine's front 1 m inside the face, a carriage behind it.
TRAIN = []
if "train" in views:
    front = tp.X_F + 1.0
    for name in ("train_engine", "train_carriage_b"):
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(REPO / "assets/kit/p1" / name / f"{name}.glb"))
        new = [o for o in bpy.data.objects if o not in before]
        body = next(o for o in new if o.name == f"{name}_lod0")
        xs = [(body.matrix_world @ v.co).x for v in body.data.vertices]
        dx = front - max(xs)
        for o in new:
            if o.parent is None:
                o.location.x += dx
                o.location.z += 0.46
            o.hide_render = o.type == "MESH" and o is not body
            TRAIN.append(o)
        front += dx + min(xs) - 0.4


def cam(name, loc, look, lens=35.0):
    """A camera at runtime point `loc` looking at runtime point `look`."""
    c = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    c.location = rt(loc[0], loc[2], loc[1])
    tgt = rt(look[0], look[2], look[1])
    d = [tgt[i] - c.location[i] for i in range(3)]
    c.rotation_mode = "XYZ"
    c.rotation_euler = (math.atan2(math.hypot(d[0], d[1]), -d[2]), 0.0, math.atan2(d[1], d[0]) - math.pi / 2)
    c.data.lens = lens
    c.data.clip_end = 500.0
    scn.collection.objects.link(c)
    return c


xf = tp.X_F
SHOTS = {  # runtime (x, y, z) camera and target, lens
    "three_quarter": ((xf - 34.0, 24.0, 24.0), (xf, 2.5, 0.0), 35.0),
    "ref2": ((xf - 12.5, 4.0, -7.5), (xf, 3.2, 0.8), 30.0),
    "ref3": ((xf - 36.0, 30.0, 0.0), (xf - 6.0, 1.0, 0.0), 35.0),
    "front": ((xf - 30.0, 7.0, 0.0), (xf, 3.0, 0.0), 40.0),
    "eye": ((xf - 16.0, 1.7, 1.5), (xf, 2.8, 0.0), 30.0),
    "close": ((xf - 5.5, 3.8, -7.0), (xf, 3.4, 4.3), 30.0),
    "top": ((0.0, 75.0, 0.1), (0.0, 0.0, 0.0), 35.0),
    "night": ((xf - 26.0, 10.0, 12.0), (xf, 3.0, 0.0), 35.0),
    "train": ((xf - 16.0, 4.5, -6.0), (xf, 2.8, 0.0), 35.0),
}
suffix = f"-lod{show_lod}" if show_lod else ""
for v in views:
    loc, look, lens = SHOTS[v]
    night = v == "night"
    for o in TRAIN:
        o.hide_render = v != "train" or (o.type == "MESH" and not o.name.endswith("_lod0"))
    for bsdf, base in GLASS:
        bsdf.inputs["Emission Color"].default_value = base
        bsdf.inputs["Emission Strength"].default_value = 4.0 if night else 0.0
    sun.data.energy = 0.05 if night else 3.5
    bg.inputs[1].default_value = 0.03 if night else 0.8
    # The sun from the west-south-west, on the face (the kit sun stands east of a yaw-0 tile).
    sun.rotation_euler = (math.radians(50), 0.0, math.radians(-110))
    scn.camera = cam(v, loc, look, lens)
    scn.render.filepath = str(out / f"portal-{v}{suffix}.jpg")
    bpy.ops.render.render(write_still=True)
    print(f"wrote {scn.render.filepath}")
