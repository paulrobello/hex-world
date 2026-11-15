"""Headless textured EEVEE stills of a complete rail tunnel: rail_tunnel_portal at both ends, two
rail_tunnel_bore cells between them, on a stand-in hill (evidence only: Blender's view transform is not
Bevy's, and the game draws the real ground, grass, track and trains).

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_tunnel_bore/tunnel_still.py -- <out_dir> [views] [lod] [state]

Imports the staged GLBs (target/kit-stage/<piece>/, after `make kit-build`) and binds their baked maps as
docs/checkpoints/rail_tunnel_portal/eevee_still.py does (stone slots albedo, tangent normal and ORM with AO
darkening the colour to 40% at 0; the turf a flat grass tone, as the runtime samples the world-space grass
set; the bore's glass lit from its emissive map at every hour). state `complete` (default) removes the
portals' `_back` faces, `closed` their `_open` faces, as the runtime's slot contract does. Every material
culls back faces, as the game's do, so the cover meadow's cap does not close the bore from below.

The stand-in (runtime frame, x along the tunnel): portal A at the origin (mouth toward -x), cover cells at
x = 27.7 and 55.4 at the 4 m cover level with their skirts cut where the bore crosses (|s| <= 2.6 on edges
0 and 3, as the coordinator cuts them in spawn_column), portal B at x = 83.1 turned half round, the hill's
side cells at 4 m and flat meadows at 0 before the mouths. Track boxes run through both portal tiles; the
bores carry their own. The `train` views stand train_engine and train_carriage_b LOD0 in the first bore.
Writes <out_dir>/tunnel-<view>[-lod<n>][-closed].jpg."""
import math
import pathlib
import sys

import bmesh
import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))

import _tunnel_portal_rail as tpr  # noqa: E402  (sets the rail globals on _tunnel_portal)
from lib import hexgeo, materials  # noqa: E402

tp = tpr.tp
args = sys.argv[sys.argv.index("--") + 1:]
out = pathlib.Path(args[0])
ALL = ["orbit", "side", "mouth", "train", "train_side", "seam", "night"]
views = args[1].split(",") if len(args) > 1 and args[1] else ALL
show_lod = int(args[2]) if len(args) > 2 and args[2] else 0
state = args[3] if len(args) > 3 else "complete"
out.mkdir(parents=True, exist_ok=True)
PORTAL, BORE = "rail_tunnel_portal", "rail_tunnel_bore"
A2 = 2 * hexgeo.APOTHEM

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
    m = materials.pbr_material(name, (*rgb, 1.0), rough)
    m.use_backface_culling = True
    return m


GRASS, SOIL = flat("grass", (0.10, 0.20, 0.05)), flat("soil", (0.12, 0.08, 0.05))
BALLAST, SLEEPER = flat("ballast", (0.20, 0.20, 0.21)), flat("sleeper", (0.075, 0.056, 0.042), 0.85)
STEEL, HEAD = flat("steel", (0.13, 0.085, 0.06), 0.6), flat("head", (0.56, 0.56, 0.54), 0.28)


def rt(x, z, y):
    """Blender point of runtime (x, y, z)."""
    return (x, -z, y)


def tile(cx, cz, height, yaw=0.0, cut_edges=(), n=24):
    """A hex tile centred at runtime (cx, cz) turned `yaw` (radians, about +y), its cap from
    `height(x, z)` (tile-local) and a soil skirt down to y = -1 (the stand-in column), the skirt cut
    over the bore (|s| <= A + 0.05 from the tile level - 0.3 up to 5.0) on `cut_edges` (tile-local)."""
    c, s = math.cos(yaw), math.sin(yaw)

    def world(x, z, y):
        return rt(cx + c * x - s * z, cz + s * x + c * z, y)
    bm = bmesh.new()
    for e in range(6):
        a0, a1 = math.radians(30 + 60 * (e - 1)), math.radians(30 + 60 * e)
        ca = (hexgeo.HEX_R * math.cos(a0), hexgeo.HEX_R * math.sin(a0))
        cb = (hexgeo.HEX_R * math.cos(a1), hexgeo.HEX_R * math.sin(a1))
        grid, loc = {}, {}
        for i in range(n + 1):
            for j in range(i + 1):
                x = ca[0] * i / n + (cb[0] - ca[0]) * j / n
                z = ca[1] * i / n + (cb[1] - ca[1]) * j / n
                loc[i, j] = (x, z)
                grid[i, j] = bm.verts.new(world(x, z, height(x, z)))
        for i in range(n):
            for j in range(i + 1):
                bm.faces.new((grid[i, j], grid[i + 1, j], grid[i + 1, j + 1]))
                if j < i:
                    bm.faces.new((grid[i, j], grid[i + 1, j + 1], grid[i, j + 1]))
        for j in range(n):
            (xa, za), (xb, zb) = loc[n, j], loc[n, j + 1]
            if e in cut_edges and max(abs(za), abs(zb)) <= tp.A + 0.05 + hexgeo.HEX_R / n:
                continue
            a, b = grid[n, j], grid[n, j + 1]
            la = bm.verts.new(world(xa, za, -1.0))
            lb = bm.verts.new(world(xb, zb, -1.0))
            bm.faces.new((a, la, lb, b))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    for f in bm.faces:
        f.normal_update()
        ctr = f.calc_center_median()
        if abs(f.normal.z) > 0.05:
            if f.normal.z < 0:
                f.normal_flip()
        elif (ctr.x - cx) * f.normal.x + (ctr.y + cz) * f.normal.y < 0:
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


# The ground: the portals' cuttings, the cover cells, the hill's side cells, the plains before the mouths.
XB = 3 * A2
cells = {}


def put(cx, cz, h):
    cells.setdefault((round(cx, 3), round(cz, 3)), h)


OPEN = state == "complete"
tile(0.0, 0.0, tp.cut_height_at, cut_edges=(0,) if OPEN else ())
tile(XB, 0.0, tp.cut_height_at, yaw=math.pi, cut_edges=(0,) if OPEN else ())
for k in (1, 2):
    tile(k * A2, 0.0, lambda x, z: 4.0, cut_edges=(0, 3) if OPEN else ())
for k, cx in enumerate((0.0, A2, 2 * A2, XB)):
    for e in (1, 2, 4, 5):
        a = math.radians(60 * e)
        put(cx + A2 * math.cos(a), A2 * math.sin(a), 4.0)
put(-A2, 0.0, 0.0)
put(XB + A2, 0.0, 0.0)
for (cx, cz), h in cells.items():
    tile(cx, cz, lambda x, z, h=h: h)
X0 = -A2 - 4.0
for lo, hi in ((X0, tp.X_E), (XB - tp.X_E, XB - X0)):
    boxes("ballast", BALLAST, [(lo, hi, -0.3, 0.22, -1.75, 1.75), (lo, hi, -0.3, 0.1, -2.5, -1.75),
                               (lo, hi, -0.3, 0.1, 1.75, 2.5)])
    boxes("sleepers", SLEEPER, [(lo + 0.7 * i - 0.13, lo + 0.7 * i + 0.13, 0.18, 0.30, -1.3, 1.3)
                                for i in range(1, int((hi - lo) / 0.7))])
    for s in (-1, 1):
        boxes("rail", STEEL, [(lo, hi, 0.30, 0.42, s * 0.84 - 0.03, s * 0.84 + 0.03)])
        boxes("head", HEAD, [(lo, hi, 0.42, 0.46, s * 0.84 - 0.04, s * 0.84 + 0.04)])


def place(piece, x, yaw):
    """Import a staged piece at runtime (x, 0, 0) turned `yaw` about +y; returns its LOD objects."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(REPO / "target/kit-stage" / piece / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        if o.parent is None:
            o.location.x += x
            o.rotation_euler.z += yaw
        n = o.name.split(".")[0]  # a second import of the same GLB gets ".001" names
        if n.endswith("_collider") or ("_lod" in n and not n.endswith(f"_lod{show_lod}")):
            o.hide_render = True
    return [o for o in new if o.type == "MESH"]


def drop_faces(objs, suffix):
    for o in objs:
        drop = {i for i, m in enumerate(o.data.materials) if m and m.name.split(".")[0].endswith(suffix)}
        if not drop:
            continue
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index in drop], context="FACES")
        bm.to_mesh(o.data)
        bm.free()


portals = place(PORTAL, 0.0, 0.0) + place(PORTAL, XB, math.pi)
drop_faces(portals, "_back" if state == "complete" else "_open")
if state == "complete":
    place(BORE, A2, 0.0)
    place(BORE, 2 * A2, math.pi)


def image(piece, channel, colour):
    img = bpy.data.images.load(str(REPO / "target/kit-stage" / piece / f"{piece}.{channel}.png"))
    img.colorspace_settings.name = "sRGB" if colour else "Non-Color"
    return img


IMG = {p: {c: image(p, c, c in ("albedo", "emissive")) for c in ("albedo", "normal", "orm", "masks")} for p in (PORTAL, BORE)}
IMG[BORE]["emissive"] = image(BORE, "emissive", True)


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


GLASS, LAMPS = [], []
for m in bpy.data.materials:
    piece = PORTAL if m.name.startswith(PORTAL) else BORE if m.name.startswith(BORE) else None
    if piece is None or not m.node_tree:
        continue
    m.use_backface_culling = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    base = tuple(bsdf.inputs["Base Color"].default_value)
    slot = m.name.split(".")[0][len(piece) + 1:]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    img = IMG[piece]
    if slot in ("stone", "fern", "barrel_back", "stone_back", "stone_open", "glass") and not (piece == PORTAL and slot == "glass"):
        alb, orm = tex(nt, img["albedo"], uvn), tex(nt, img["orm"], uvn)
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(orm.outputs["Color"], sep.inputs[0])
        nt.links.new(ao_mul(nt, alb.outputs["Color"], sep.outputs[0]), bsdf.inputs["Base Color"])
        nt.links.new(sep.outputs[1], bsdf.inputs["Roughness"])
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nt.links.new(tex(nt, img["normal"], uvn).outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
        if slot == "fern":
            clip = nt.nodes.new("ShaderNodeMath")
            clip.operation = "GREATER_THAN"
            clip.inputs[1].default_value = 0.5
            nt.links.new(alb.outputs["Alpha"], clip.inputs[0])
            nt.links.new(clip.outputs[0], bsdf.inputs["Alpha"])
            m.surface_render_method = "DITHERED"
            m.use_backface_culling = False
        if slot == "glass":
            nt.links.new(tex(nt, img["emissive"], uvn).outputs["Color"], bsdf.inputs["Emission Color"])
            LAMPS.append(bsdf)
    elif slot in ("turf", "turf_open"):
        bsdf.inputs["Base Color"].default_value = (0.10, 0.20, 0.05, 1.0)
    elif slot in ("lining_back", "back"):
        mk = tex(nt, img["masks"], uvn)
        mk.interpolation = "Closest"
        nt.links.new(ao_mul(nt, base, mk.outputs["Alpha"]), bsdf.inputs["Base Color"])
    elif slot == "glass":
        GLASS.append((bsdf, base))

TRAIN = []
if any(v.startswith("train") for v in views):
    front = A2 + 6.0
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
    c.data.clip_start = 0.05
    c.data.clip_end = 600.0
    scn.collection.objects.link(c)
    return c


mid = 1.5 * A2
SHOTS = {  # runtime (x, y, z) camera and target, lens
    "orbit": ((mid - 55.0, 62.0, 70.0), (mid, 2.0, 0.0), 32.0),
    "side": ((mid + 8.0, 20.0, 36.0), (mid - 4.0, 4.5, 0.0), 26.0),
    "mouth": ((tp.X_F - 14.0, 2.2, 1.2), (tp.X_E + 6.0, 2.4, 0.0), 30.0),
    "train": ((A2 + 14.0, 2.6, -0.9), (A2 + 4.0, 2.7, 0.2), 24.0),
    "train_side": ((A2 + 10.5, 2.6, -1.6), (A2 + 3.46, 2.15, -2.0), 30.0),
    "seam": ((tp.X_E - 5.0, 1.6, 1.8), (tp.X_E + 4.0, 3.2, -1.2), 22.0),
    "night": ((tp.X_F - 14.0, 2.2, 1.2), (tp.X_E + 6.0, 2.4, 0.0), 30.0),
}
suffix = (f"-lod{show_lod}" if show_lod else "") + ("-closed" if state != "complete" else "")
for v in views:
    loc, look, lens = SHOTS[v]
    night = v == "night"
    for o in TRAIN:
        o.hide_render = not v.startswith("train") or (o.type == "MESH" and not o.name.endswith("_lod0"))
    for bsdf, base in GLASS:
        bsdf.inputs["Emission Color"].default_value = base
        bsdf.inputs["Emission Strength"].default_value = 4.0 if night else 0.0
    for bsdf in LAMPS:
        bsdf.inputs["Emission Strength"].default_value = 3.0
    sun.data.energy = 0.05 if night else 3.5
    bg.inputs[1].default_value = 0.03 if night else 0.8
    sun.rotation_euler = (math.radians(50), 0.0, math.radians(-110))
    scn.camera = cam(v, loc, look, lens)
    scn.render.filepath = str(out / f"tunnel-{v}{suffix}.jpg")
    bpy.ops.render.render(write_still=True)
    print(f"wrote {scn.render.filepath}")
