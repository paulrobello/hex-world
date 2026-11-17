import ast
import json
import math
import os
import pathlib
import shutil
import struct
import subprocess
import tempfile

import bpy
import numpy as np

from . import bake, geom, hexgeo, scene

REPO = pathlib.Path(__file__).resolve().parents[3]  # scripts/blender/lib -> repo root
# KTX2 budget classes, read by kit-compress.mjs (KTX_BUDGETS in scripts/assets/kit-lib.mjs).
BUDGET_CLASSES = ("piece", "tileable", "hero")


def add_socket(name, loc, parent, yaw_deg=0.0, **extras):
    """Named empty exported as a glTF node with extras (export_extras=True).
    Names: socket-lane-<n> (network lanes), anchor-prop-<n> (prop anchors), anchor-park-<n> (parking spots)."""
    e = scene.empty(name, loc, parent)
    e.rotation_euler = (0.0, 0.0, math.radians(yaw_deg))
    for k, v in extras.items():
        e[k] = v
    return e


def _tree(root):
    out = [root]
    for c in root.children:
        out += _tree(c)
    return out


def export_glb(root: bpy.types.Object, path: str, threads: int = 0) -> None:
    """Export one root, without images, with tangents and Y-up.

    The default preserves in-process export with Blender's current thread count. A positive
    thread count exports a saved scene in a child Blender. Use 1 for deterministic MikkTSpace
    rounding on large meshes. Kit builds select that path through HEXW_EXPORT_THREADS.
    """
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    threads = int(os.environ.get("HEXW_EXPORT_THREADS", threads))
    if threads:
        _export_glb_child(root, path, threads)
        return
    _export_glb_inproc(root, path)


def _export_glb_inproc(root, path):
    objs = _tree(root)
    for o in objs:
        o.hide_render = False
        o.hide_set(False)
    scene.select_only(objs, root)
    bpy.ops.export_scene.gltf(
        filepath=str(path), export_format="GLB", use_selection=True, use_active_scene=True,
        export_yup=True, export_apply=True, export_extras=True, export_tangents=True,
        export_normals=True, export_texcoords=True, export_cameras=False, export_lights=False,
        export_image_format="NONE", export_materials="EXPORT", export_animations=False)
    check_unit_vectors(path)


def _export_glb_child(root: bpy.types.Object, path: str, threads: int) -> None:
    """MikkTSpace's threaded float sums can flip tangent rounding ties on large meshes."""
    tmp = tempfile.mkdtemp(prefix="export-single-thread-")
    glb = pathlib.Path(path).resolve()
    glb.parent.mkdir(parents=True, exist_ok=True)
    glb.unlink(missing_ok=True)
    try:
        blend = os.path.join(tmp, "export.blend")
        bpy.ops.wm.save_as_mainfile(filepath=blend, copy=True)
        code = (f"import sys; sys.path.insert(0, {str(pathlib.Path(__file__).resolve().parents[1])!r}); "
                f"import bpy; from lib import export; "
                f"export.export_glb(bpy.data.objects[{root.name!r}], {str(glb)!r}, threads=0)")
        env = {**os.environ, "HEXW_EXPORT_THREADS": "0"}
        run = subprocess.run([bpy.app.binary_path, "-b", "--factory-startup", "-t", str(threads), blend,
                              "--python-exit-code", "1", "--python-expr", code], capture_output=True, text=True,
                             env=env)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if not glb.is_file():
        raise RuntimeError(f"single-threaded export wrote no GLB (exit {run.returncode}): {run.stderr[-2000:]}")
    check_unit_vectors(str(glb))
    if run.returncode != 0:
        raise RuntimeError(f"single-threaded export failed (exit {run.returncode}): {run.stderr[-2000:]}")


# gltf-validator (run by kit-compress) rejects NORMAL/TANGENT xyz whose length is off 1 by more than
# this; zero-area faces export zero-length tangents, which used to surface only at kit-compress.
UNIT_TOL = 0.00674  # gltf-validator 2.0.0-dev.3.10 ACCESSOR_VECTOR3_NON_UNIT threshold


def check_unit_vectors(path):
    """Raises ValueError naming every mesh primitive whose NORMAL or TANGENT xyz is not unit length."""
    data = pathlib.Path(path).read_bytes()
    n = struct.unpack_from("<I", data, 12)[0]
    j = json.loads(data[20:20 + n])
    bin_at = 20 + n + 8
    bad = []
    for m in j.get("meshes", []):
        for pi, prim in enumerate(m["primitives"]):
            for attr in ("NORMAL", "TANGENT"):
                if attr not in prim["attributes"]:
                    continue
                a = j["accessors"][prim["attributes"][attr]]
                bv = j["bufferViews"][a["bufferView"]]
                width = 4 if a["type"] == "VEC4" else 3
                stride = bv.get("byteStride", 4 * width) // 4
                v = np.frombuffer(data, np.float32, (a["count"] - 1) * stride + width,
                                  bin_at + bv.get("byteOffset", 0) + a.get("byteOffset", 0))
                xyz = np.lib.stride_tricks.as_strided(v, (a["count"], 3), (stride * 4, 4))
                off = np.abs(np.linalg.norm(xyz, axis=1) - 1.0) > UNIT_TOL
                if off.any():
                    bad.append(f"{m['name']} primitive {pi} {attr}: {int(off.sum())} of {a['count']} not unit "
                               f"(first vertex {int(np.argmax(off))})")
    if bad:
        raise ValueError(f"{path}: non-unit vectors (zero-area or degenerate faces?): " + "; ".join(bad))


def generator_deps(generator_path):
    """Local helper modules a generator imports, transitively: every `_*.py` beside the importing
    file named by any absolute `import _x` / `from _x import ...` statement, at any nesting depth
    (ast.walk). Returns sorted
    [{path (repo-relative POSIX), sha256}], so provenance changes when a shared helper such as
    kit/_road.py changes. Names that do not resolve to a sibling file are ignored."""
    seen, todo = {}, [pathlib.Path(generator_path).resolve()]
    while todo:
        src = todo.pop()
        for node in ast.walk(ast.parse(src.read_text(), str(src))):
            if isinstance(node, ast.Import):
                names = [a.name for a in node.names]
            elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
                names = [node.module]
            else:
                continue
            for name in names:
                dep = src.parent / f"{name.split('.')[0]}.py"
                if name.startswith("_") and dep.is_file() and dep not in seen:
                    seen[dep] = bake.sha256_file(str(dep))
                    todo.append(dep)
    return sorted(({"path": p.relative_to(REPO).as_posix(), "sha256": h} for p, h in seen.items()),
                  key=lambda d: d["path"])


def write_export_report(path, root, piece_id, generator_path, budget_class=None):
    """Writes {pieceId, nodes, lodTris, sockets, generator{path, sha256}, blender}; socket positions
    are runtime frame (Y-up). generator.path is repo-relative (POSIX) so provenance never leaks
    an absolute host path; generator_path must lie inside the repo. budget_class (one of
    BUDGET_CLASSES, e.g. "hero") adds "budgetClass", which kit-compress uses as the KTX2 budget."""
    if budget_class is not None and budget_class not in BUDGET_CLASSES:
        raise ValueError(f"budget_class {budget_class!r} not in {BUDGET_CLASSES}")
    bpy.context.view_layer.update()  # matrix_world is stale for objects created since the last update
    nodes, lods, sockets = [], {}, []
    for o in _tree(root):
        nodes.append(o.name)
        if o.type == "MESH" and o.name.startswith(f"{piece_id}_lod"):
            lods[o.name] = geom.tri_count(o)
        if o.name.startswith(("socket-lane-", "anchor-prop-", "anchor-park-")):
            p = o.matrix_world.translation
            sockets.append({"name": o.name, "position": list(hexgeo.blender_to_rt(p.x, p.y, p.z)),
                            "yaw": math.degrees(o.matrix_world.to_euler().z),
                            "extras": {k: o[k] for k in o.keys() if not k.startswith("_")}})
    rep = {"pieceId": piece_id, "nodes": sorted(nodes), "lodTris": lods, "sockets": sockets,
           "generator": {"path": pathlib.Path(generator_path).resolve().relative_to(REPO).as_posix(),
                         "sha256": bake.sha256_file(generator_path)},
           "generatorDeps": generator_deps(generator_path), "blender": bpy.app.version_string}
    if budget_class is not None:
        rep["budgetClass"] = budget_class
    pathlib.Path(path).write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    return rep
