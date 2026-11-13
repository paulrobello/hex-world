"""Fits a scatter rock's `grass::ROCK_BASES` 12-gon and reports its collider reach for `scatter::FOOTPRINTS`,
from the y = 0 slice of the GLB's LOD0 and collider meshes (what `grass::rock_bases_match_the_kit`
reads). The 12 sides are supporting lines of the slice's convex hull at 30 degree steps, pushed out
0.5 mm, so the outline holds the slice (1 mm test) and every side touches the stone (2 cm test).
Usage: python3 docs/checkpoints/outcrop_granite_b/rock_base.py <glb> <piece>
Reads an uncompressed GLB (target/kit-stage/<piece>/<piece>.glb); kit-compress keeps FLOAT positions."""
import json
import math
import struct
import sys

import numpy as np

COMP = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
SIZE = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}


def load(path):
    data = open(path, "rb").read()
    n = struct.unpack_from("<I", data, 12)[0]
    gltf = json.loads(data[20:20 + n])
    off = 20 + n
    blen = struct.unpack_from("<I", data, off)[0]
    return gltf, data[off + 8:off + 8 + blen]


def accessor(gltf, binary, i):
    a = gltf["accessors"][i]
    v = gltf["bufferViews"][a["bufferView"]]
    dt = COMP[a["componentType"]]
    k = SIZE[a["type"]]
    start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    stride = v.get("byteStride", 0)
    if stride and stride != k * np.dtype(dt).itemsize:
        raise SystemExit("strided accessor not handled")
    return np.frombuffer(binary, dt, a["count"] * k, start).reshape(a["count"], k)


def slice_points(gltf, binary, names):
    pts = []
    for mesh in gltf["meshes"]:
        if mesh["name"] not in names:
            continue
        for prim in mesh["primitives"]:
            pos = accessor(gltf, binary, prim["attributes"]["POSITION"]).astype(np.float64)
            idx = accessor(gltf, binary, prim["indices"]).ravel()
            for t in idx.reshape(-1, 3):
                for k in range(3):
                    a, b = pos[t[k]], pos[t[(k + 1) % 3]]
                    if a[1] != b[1] and a[1] * b[1] <= 0.0:
                        s = a[1] / (a[1] - b[1])
                        pts.append((a[0] + (b[0] - a[0]) * s, a[2] + (b[2] - a[2]) * s))
    return np.array(pts)


def main():
    path, piece = sys.argv[1], sys.argv[2]
    gltf, binary = load(path)
    pts = slice_points(gltf, binary, {f"{piece}_lod0", f"{piece}_collider"})
    col = [m for m in gltf["meshes"] if m["name"] == f"{piece}_collider"][0]
    cpos = accessor(gltf, binary, col["primitives"][0]["attributes"]["POSITION"])
    reach = float(np.hypot(cpos[:, 0], cpos[:, 2]).max())
    th = np.radians(np.arange(12) * 30.0)
    n = np.stack([np.cos(th), np.sin(th)], 1)
    h = (pts @ n.T).max(0) + 0.0005
    verts = []
    for k in range(12):
        j = (k + 1) % 12
        verts.append(np.linalg.solve(np.stack([n[k], n[j]]), np.array([h[k], h[j]])))
    print(f"slice points {len(pts)}, collider reach {reach:.4f}")
    print(f'    (\n        "{piece}",\n        [')
    for v in verts:
        print(f"            [{v[0]:.3f}, {v[1]:.3f}],")
    print("        ],\n    ),")


main()
