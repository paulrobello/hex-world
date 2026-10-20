"""Cycles bakes (ported from par-factory astra_machine_common.py:903-1620, machine specifics dropped)."""
import hashlib
import json
import os
import pathlib

import bpy
import numpy as np

from . import materials, scene


class StaleBakeError(RuntimeError):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def setup_cycles(device=None, samples=16):
    """Cycles with bake defaults. device None -> $HEXW_BAKE_DEVICE or "CPU"; anything else uses Metal."""
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.samples = samples
    device = device or os.environ.get("HEXW_BAKE_DEVICE", "CPU")
    prefs = bpy.context.preferences.addons["cycles"].preferences
    if device != "CPU":
        prefs.compute_device_type = "METAL"
        prefs.kernel_optimization_level = "OFF"  # Metal kernels otherwise recompile per channel
        prefs.get_devices()
        for d in prefs.devices:
            d.use = d.type == "METAL"
    s.cycles.device = "GPU" if device != "CPU" else "CPU"
    b = s.render.bake
    b.use_pass_direct = False  # no directional lighting baked into colour
    b.use_pass_indirect = False
    b.use_pass_color = True
    b.normal_space = "TANGENT"
    return device


def new_image(name, size, non_color):
    img = bpy.data.images.new(name, size, size, alpha=True, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def _target(low, image):
    """Make `image` the active Image Texture node in every material of the low mesh."""
    if not low.data.materials:
        low.data.materials.append(bpy.data.materials.new(f"{low.name}_bake"))
    for m in low.data.materials:
        nt = materials.ensure_nodes(m)
        node = nt.nodes.get("HEXW_BAKE_TARGET") or nt.nodes.new("ShaderNodeTexImage")
        node.name = "HEXW_BAKE_TARGET"
        node.image = image
        nt.nodes.active = node


def bake_to(low, bake_type, image, highs=(), cage_extrusion=0.0, max_ray=0.0, margin=12, clear=True,
            occluders=()):
    """bake_type in {AO, NORMAL, ROUGHNESS, EMIT}. highs empty = bake the low mesh onto itself.
    Every scene mesh other than `low`, `highs` and `occluders` (other LODs, colliders) is
    hide_render'd for the bake and restored afterwards, so it cannot occlude AO or catch rays.
    `occluders` stay visible: a neighbouring piece that stands on or against this one at runtime
    (rail_station_shelter on rail_station_platform) and must shade it."""
    _target(low, image)
    b = bpy.context.scene.render.bake
    b.use_selected_to_active = bool(highs)
    b.cage_extrusion = cage_extrusion
    b.max_ray_distance = max_ray
    b.margin = margin
    b.use_clear = clear
    keep = {low.name, *(h.name for h in highs), *(o.name for o in occluders)}
    hidden = [o for o in bpy.context.scene.objects
              if o.type == "MESH" and o.name not in keep and not o.hide_render]
    for o in hidden:
        o.hide_render = True
    try:
        scene.select_only(list(highs) + [low], low)
        bpy.ops.object.bake(type=bake_type)
    finally:
        for o in hidden:
            o.hide_render = False


def bake_emit_input(highs, low, input_name, image, /, **kw):
    """Bake a Principled input (e.g. 'Base Color', 'Metallic') through EMIT so diffuse shading
    cannot darken it (astra_machine_common.py:1386-1443). Restores the materials afterwards.
    `highs` is positional-only and must not also appear in **kw; kw goes to bake_to."""
    for dup in ("highs", "bake_type"):
        if dup in kw:
            raise TypeError(f"bake_emit_input: {dup} is positional (or fixed); pass it once, "
                            f"not again in **kw (got kw keys {sorted(kw)})")
    sources = list(highs) or [low]
    restored = []
    for mat in {m for o in sources for m in o.data.materials if m}:
        nt = mat.node_tree
        out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
        old = out.inputs["Surface"].links[0].from_socket
        p = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        emit = nt.nodes.new("ShaderNodeEmission")
        src = p.inputs[input_name]
        if src.is_linked:
            nt.links.new(src.links[0].from_socket, emit.inputs["Color"])
        else:
            v = src.default_value
            emit.inputs["Color"].default_value = (v, v, v, 1.0) if isinstance(v, float) else v
        nt.links.new(emit.outputs[0], out.inputs["Surface"])
        restored.append((nt, out, old, emit))
    try:
        bake_to(low, "EMIT", image, highs=highs, **kw)
    finally:
        for nt, out, old, emit in restored:
            nt.links.new(old, out.inputs["Surface"])
            nt.nodes.remove(emit)


def save_png(image, path):
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    return str(path)


def load_png(path):
    """(width, float32 RGBA pixels of shape (w*h, 4)) of a PNG, read as Non-Color (no transfer)."""
    img = bpy.data.images.load(str(path), check_existing=False)
    img.colorspace_settings.name = "Non-Color"
    a = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(a)
    return img.size[0], a.reshape(-1, 4)


def write_png(size, rgba, path, non_color=True):
    """Writes size x size RGBA pixels (Blender order) as a PNG; non_color=False tags it sRGB."""
    img = bpy.data.images.new(pathlib.Path(path).stem, size, size, alpha=True)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    img.pixels.foreach_set(np.ascontiguousarray(rgba, np.float32).ravel())
    return save_png(img, path)


# Old private names, still called by the committed meadow_base.py and tree_hero_oak.py: editing a
# generator re-hashes an owner-approved piece, so they switch to the public names at their next rebuild.
_load = load_png
_write = write_png


def pack_orm(ao_png, rough_png, metal_png, out_png):
    """ORM: R = AO, G = roughness, B = metallic (glTF / Bevy metallic_roughness reads G, B)."""
    s, ao = load_png(ao_png)
    _, r = load_png(rough_png)
    _, m = load_png(metal_png)
    rgba = np.stack([ao[:, 0], r[:, 0], m[:, 0], np.ones_like(ao[:, 0])], axis=1)
    return write_png(s, rgba, out_png)


def make_masks(size, snow, wet, night, ao_png, out_png):
    """masks (linear): R snow capacity, G wetness/porosity, B night emissive, A AO (1.0 if None).
    snow/wet/night are floats or (size*size,) arrays in Blender pixel order."""
    n = size * size
    ch = [np.broadcast_to(np.asarray(c, np.float32), (n,)) for c in (snow, wet, night)]
    if ao_png:
        s, ao = load_png(ao_png)
        if s != size:
            raise ValueError(f"AO size {s} != masks size {size}")
        a = ao[:, 0]
    else:
        a = np.ones(n, np.float32)
    return write_png(size, np.stack([*ch, a], axis=1), out_png)


def receiver_signature(objs):
    """Hash of receiver topology, UVs and corner normals; a changed receiver invalidates cached bakes."""
    h = hashlib.sha256()
    for o in sorted(objs, key=lambda o: o.name):
        m = o.data
        h.update(o.name.encode())
        co = np.empty(len(m.vertices) * 3, np.float64)
        m.vertices.foreach_get("co", co)
        h.update(np.round(co, 6).tobytes())
        for p in m.polygons:
            h.update(np.asarray(p.vertices, np.int64).tobytes())
        for layer in m.uv_layers:
            uv = np.empty(len(m.loops) * 2, np.float64)
            layer.data.foreach_get("uv", uv)
            h.update(np.round(uv, 6).tobytes())
        cn = np.empty(len(m.loops) * 3, np.float64)
        m.corner_normals.foreach_get("vector", cn)
        h.update(np.round(cn, 5).tobytes())
    return h.hexdigest()


def write_bake_report(path, **fields):
    """JSON report with sorted keys plus the Blender version. A report naming its `generator`
    (repo-relative, as tileable sets do) also records `generatorDeps`, which provenance requires."""
    fields.setdefault("blender", bpy.app.version_string)
    if "generator" in fields and "generatorDeps" not in fields:
        from . import export  # export imports this module at load time
        fields["generatorDeps"] = export.generator_deps(export.REPO / fields["generator"])
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(path).write_text(json.dumps(fields, indent=2, sort_keys=True) + "\n")


def check_bake_cache(report_path, signature):
    p = pathlib.Path(report_path)
    if not p.exists():
        raise StaleBakeError(f"no bake report at {p}")
    old = json.loads(p.read_text()).get("receiverSignature")
    if old != signature:
        raise StaleBakeError(f"receiver changed since bake ({old} != {signature}); rebake")


def bake_tileable_set(material, size, out_dir, set_id, metallic=0.0):
    """Bake a seamless tileable PBR set from a material built on lib.materials.torus_coords.
    Writes <set>.albedo.png (sRGB), <set>.normal.png and <set>.orm.png (linear). AO = 1 (flat plane)."""
    out = pathlib.Path(out_dir)
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=(0, 0, 0))
    plane = bpy.context.active_object
    plane.name = f"{set_id}_bakeplane"
    plane.data.materials.append(material)
    albedo = new_image(f"{set_id}.albedo", size, non_color=False)
    bake_emit_input([], plane, "Base Color", albedo, margin=0)
    rough = new_image(f"{set_id}.rough", size, non_color=True)
    bake_to(plane, "ROUGHNESS", rough, margin=0)
    normal = new_image(f"{set_id}.normal", size, non_color=True)
    bake_to(plane, "NORMAL", normal, margin=0)
    files = {"albedo": save_png(albedo, out / f"{set_id}.albedo.png"),
             "normal": save_png(normal, out / f"{set_id}.normal.png")}
    rough_png = save_png(rough, out / f"{set_id}.rough.tmp.png")
    s, r = load_png(rough_png)
    rgba = np.stack([np.ones_like(r[:, 0]), r[:, 0], np.full_like(r[:, 0], metallic), np.ones_like(r[:, 0])], axis=1)
    files["orm"] = write_png(s, rgba, out / f"{set_id}.orm.png")
    os.remove(rough_png)
    return files
