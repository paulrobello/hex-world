"""Headless textured EEVEE stills of an open road tunnel built from road_tunnel_portal and road_tunnel_bore (evidence
only: Blender's view transform and its interior light are not Bevy's, and the runtime places the bores).

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/road_tunnel_bore/eevee_still.py -- <out_dir> [views] [lod]

The tunnel: the portal tile at the origin (yaw 0, the arch facing -x), bores on cells 1 and 2 along +x and the far
portal on cell 3 at yaw 180, both portals with their `road_tunnel_portal_back` faces removed (the runtime hides that
slot while the tunnel is complete). Stand-in ground: the portals' cuttings (_tunnel_portal.cut_height_at), the road on
them, the cover meadow at 4 m as caps over the bore cells and their neighbours (no skirts: the runtime's cell-edge
columns are an integration item), level-0 meadows beyond the cuttings. Staged GLBs and PNG maps from target/kit-stage
(after `make kit-build`); the bore's stone binds albedo, normal, ORM (AO darkens it as the portal stills do) and the
emissive map at strength 1 (the runtime's emissive factor), the road slots flat colours.

views: comma list from mouth, mid, car, lamp, seam_bb, seam_bp, alone (default all but alone); lod 0, 1 or 2 shows
that LOD of the bore (and the portals). Writes <out_dir>/bore-<view>[-lod<n>].jpg."""
import math
import pathlib
import sys

import bmesh
import bpy

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))

import _tunnel_portal as tp  # noqa: E402
from lib import hexgeo, materials  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
out = pathlib.Path(args[0])
views = args[1].split(",") if len(args) > 1 and args[1] else ["mouth", "mid", "car", "lamp", "seam_bb", "seam_bp"]
show_lod = int(args[2]) if len(args) > 2 else 0
out.mkdir(parents=True, exist_ok=True)
STAGE = REPO / "target/kit-stage"
AP = hexgeo.APOTHEM
A2 = 2 * AP

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
sun.rotation_euler = (math.radians(50), 0.0, math.radians(-110))
scn.collection.objects.link(sun)


def flat(name, rgb, rough=0.9):
    return materials.pbr_material(name, (*rgb, 1.0), rough)


GRASS, SOIL = flat("grass", (0.10, 0.20, 0.05)), flat("soil", (0.12, 0.08, 0.05))
ASPHALT, KERB = flat("asph", (0.06, 0.06, 0.065)), flat("kerb", (0.42, 0.41, 0.37), 0.85)


def rt(x, z, y):
    """Blender point of runtime (x, y, z)."""
    return (x, -z, y)


def tile(cx, cz, height, n=24, skirt=True):
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
        if skirt:
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


def strip(x0, x1, z0, z1, y, mat, steps=40):
    bm = bmesh.new()
    rows = []
    for i in range(steps + 1):
        x = x0 + (x1 - x0) * i / steps
        rows.append([bm.verts.new(rt(x, z, y)) for z in (z0, z1)])
    for a, b in zip(rows, rows[1:]):
        bm.faces.new((a[0], b[0], b[1], a[1]))
    me = bpy.data.meshes.new("strip")
    bm.to_mesh(me)
    bm.free()
    me.materials.append(mat)
    scn.collection.objects.link(bpy.data.objects.new("strip", me))


def nbrs(cx, cz):
    return [(round(cx + A2 * math.cos(math.radians(60 * e)), 4), round(cz + A2 * math.sin(math.radians(60 * e)), 4))
            for e in range(6)]


alone = "alone" in views
if not alone:
    portals = [(0.0, 0.0), (round(3 * A2, 4), 0.0)]
    tile(0.0, 0.0, tp.cut_height_at)
    tile(3 * A2, 0.0, lambda x, z: tp.cut_height_at(-x, -z))
    cover = set()
    for c in ((A2, 0.0), (2 * A2, 0.0)):
        cover |= {(round(c[0], 4), 0.0)} | set(nbrs(*c))
    for c in sorted(cover - set(portals)):
        tile(c[0], c[1], lambda x, z: 4.0, skirt=False)
    for c in (nbrs(0.0, 0.0)[3], nbrs(0.0, 0.0)[4], nbrs(3 * A2, 0.0)[0], nbrs(3 * A2, 0.0)[1]):
        tile(c[0], c[1], lambda x, z: 0.0)
    for x0, x1 in ((-A2 - 4.0, AP), (5 * AP, 4 * A2 + 4.0)):
        strip(x0, x1, -3.0, 3.0, 0.11, ASPHALT)
        for z0, z1 in ((-3.25, -3.0), (3.0, 3.25)):
            strip(x0, x1, z0, z1, 0.2, KERB)


def image(piece, channel):
    img = bpy.data.images.load(str(STAGE / piece / f"{piece}.{channel}.png"))
    img.colorspace_settings.name = "sRGB" if channel in ("albedo", "emissive") else "Non-Color"
    return img


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


def textured(m, imgs, emissive=None):
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    alb, orm = tex(nt, imgs["albedo"], uvn), tex(nt, imgs["orm"], uvn)
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs[0])
    nt.links.new(ao_mul(nt, alb.outputs["Color"], sep.outputs[0]), bsdf.inputs["Base Color"])
    nt.links.new(sep.outputs[1], bsdf.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(tex(nt, imgs["normal"], uvn).outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    if emissive is not None:
        nt.links.new(tex(nt, emissive, uvn).outputs["Color"], bsdf.inputs["Emission Color"])
        bsdf.inputs["Emission Strength"].default_value = 1.0
    return alb


def masked(m, masks, base):
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mk = tex(nt, masks, uvn)
    mk.interpolation = "Closest"
    nt.links.new(ao_mul(nt, base, mk.outputs["Alpha"]), bsdf.inputs["Base Color"])


def place(piece, loc, yaw=0.0, drop_back=True, root=STAGE):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(root / piece / f"{piece}.glb"))
    new = [o for o in bpy.data.objects if o not in before]
    roots = [o for o in new if o.parent is None]
    for r in roots:
        r.location = rt(loc[0], loc[2], loc[1])
        r.rotation_euler = (0.0, 0.0, math.radians(yaw))
    for o in new:
        if o.name.split(".")[0].endswith("_collider") or (
                "_lod" in o.name and not o.name.split(".")[0].endswith(f"_lod{show_lod}")):
            o.hide_render = True
        if drop_back and o.type == "MESH":
            names = [m.name.split(".")[0] if m else "" for m in o.data.materials]
            if "road_tunnel_portal_back" in names:
                bm = bmesh.new()
                bm.from_mesh(o.data)
                k = names.index("road_tunnel_portal_back")
                bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index == k], context="FACES")
                bm.to_mesh(o.data)
                bm.free()
    return new


P, BORE = "road_tunnel_portal", "road_tunnel_bore"
if alone:
    place(BORE, (0.0, 0.0, 0.0))
else:
    place(P, (0.0, 0.0, 0.0))
    place(P, (3 * A2, 0.0, 0.0), 180.0)
    place(BORE, (A2, 0.0, 0.0))
    place(BORE, (2 * A2, 0.0, 0.0))
    if "car" in views:
        place("car_sedan", (A2 + 2.0, 0.08, -1.5), drop_back=False, root=REPO / "assets/kit/p1")

PI = {c: image(P, c) for c in ("albedo", "normal", "orm", "masks")}
BI = {c: image(BORE, c) for c in ("albedo", "normal", "orm", "masks", "emissive")}
for m in list(bpy.data.materials):
    name = m.name.split(".")[0]
    if not m.node_tree or "Principled BSDF" not in m.node_tree.nodes:
        continue
    base = tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value)
    if name in (f"{P}_stone", f"{P}_fern"):
        alb = textured(m, PI)
        if name.endswith("fern"):
            nt = m.node_tree
            clip = nt.nodes.new("ShaderNodeMath")
            clip.operation = "GREATER_THAN"
            clip.inputs[1].default_value = 0.5
            nt.links.new(alb.outputs["Alpha"], clip.inputs[0])
            nt.links.new(clip.outputs[0], nt.nodes["Principled BSDF"].inputs["Alpha"])
            m.surface_render_method = "DITHERED"
            m.use_backface_culling = False
    elif name == f"{P}_turf":
        m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.10, 0.20, 0.05, 1.0)
    elif name in (f"{P}_lining", f"{P}_back"):
        masked(m, PI["masks"], base)
    elif name == f"{BORE}_stone":
        textured(m, BI, BI["emissive"])
    elif name == "road_asphalt":
        m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.06, 0.06, 0.065, 1.0)
    elif name == "road_verge":
        m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.10, 0.20, 0.05, 1.0)


def cam(name, loc, look, lens=35.0):
    c = bpy.data.objects.new(name, bpy.data.cameras.new(name))
    c.location = rt(loc[0], loc[2], loc[1])
    tgt = rt(look[0], look[2], look[1])
    d = [tgt[i] - c.location[i] for i in range(3)]
    c.rotation_mode = "XYZ"
    c.rotation_euler = (math.atan2(math.hypot(d[0], d[1]), -d[2]), 0.0, math.atan2(d[1], d[0]) - math.pi / 2)
    c.data.lens = lens
    c.data.clip_start = 0.05
    c.data.clip_end = 500.0
    scn.collection.objects.link(c)
    return c


xf = tp.X_F
SHOTS = {  # runtime (x, y, z) camera and target, lens
    "mouth": ((xf - 5.0, 1.7, 1.4), (AP + 16.0, 1.6, 0.0), 24.0),
    "mid": ((A2 - 9.0, 1.7, 1.6), (A2 + 20.0, 1.5, 0.0), 24.0),
    "car": ((A2 - 9.0, 2.3, 0.8), (A2 + 3.0, 1.0, -1.2), 28.0),
    "lamp": ((A2 - 3.6, 1.5, 0.9), (A2, 2.1, -2.8), 35.0),
    "seam_bb": ((3 * AP - 3.5, 1.4, 1.6), (3 * AP, 1.9, -2.9), 28.0),
    "seam_bp": ((AP + 3.5, 1.4, -1.6), (AP, 1.9, 2.9), 28.0),
    "alone": ((-AP - 14.0, 7.0, 16.0), (0.0, 1.2, 0.0), 30.0),
}
suffix = f"-lod{show_lod}" if show_lod else ""
for v in views:
    loc, look, lens = SHOTS[v]
    scn.camera = cam(v, loc, look, lens)
    scn.render.filepath = str(out / f"bore-{v}{suffix}.jpg")
    bpy.ops.render.render(write_still=True)
    print(f"wrote {scn.render.filepath}")
