"""Compares two staged (uncompressed) animal GLBs mesh by mesh: for every `<piece>_lod<n>` and the
collider, whether POSITION, NORMAL, TANGENT, TEXCOORD_0, TEXCOORD_1 and the indices are identical,
the triangle count, and the collider's bounding box and ground reach (the largest horizontal distance
of a collider vertex from the pivot, as `scatter::footprints_match_the_kit` measures it).

Usage: python3 docs/checkpoints/animal-lod2-head/compare_glb.py <piece> <before.glb> <after.glb>
Needs numpy. The staged GLBs come from `make kit-build` (target/kit-stage/<piece>/<piece>.glb) or a
direct `HEXW_SKIP_BAKE=1` generator run; the compressed runtime GLBs are compared by
compare_compressed.mjs."""
import json
import pathlib
import struct
import sys

import numpy as np

ATTRS = ("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "TEXCOORD_1")
NC = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
DT = {5126: "<f4", 5125: "<u4", 5123: "<u2", 5121: "<u1"}


def load(path: str) -> dict:
    data = pathlib.Path(path).read_bytes()
    off, chunks = 12, {}
    while off < len(data):
        clen, ctype = struct.unpack_from("<II", data, off)
        chunks[ctype] = data[off + 8:off + 8 + clen]
        off += 8 + clen
    js = json.loads(chunks[0x4E4F534A])
    binc = chunks[0x004E4942]

    def acc(i: int) -> np.ndarray:
        a = js["accessors"][i]
        bv = js["bufferViews"][a["bufferView"]]
        nc = NC[a["type"]]
        start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        return np.frombuffer(binc, DT[a["componentType"]], count=a["count"] * nc, offset=start).reshape(-1, nc)

    meshes = {}
    for m in js["meshes"]:
        prims = []
        for p in m["primitives"]:
            d = {k: acc(v) for k, v in p["attributes"].items() if k in ATTRS}
            d["indices"] = acc(p["indices"]).astype(np.int64)
            prims.append(d)
        meshes[m["name"]] = prims
    return meshes


def tris(prims: list) -> int:
    return sum(len(p["indices"]) // 3 for p in prims)


piece, a_path, b_path = sys.argv[1:4]
A, B = load(a_path), load(b_path)
for lv in range(3):
    name = f"{piece}_lod{lv}"
    pa, pb = A[name], B[name]
    same = len(pa) == len(pb) and all(
        set(x) == set(y) and all(np.array_equal(x[k], y[k]) for k in x) for x, y in zip(pa, pb))
    print(f"{name}: tris {tris(pa)} -> {tris(pb)}, vertices {sum(len(p['POSITION']) for p in pa)} -> "
          f"{sum(len(p['POSITION']) for p in pb)}, identical {same}")
col = f"{piece}_collider"
for label, M in (("before", A), ("after", B)):
    P = np.concatenate([p["POSITION"] for p in M[col]])
    lo, hi = P.min(0), P.max(0)
    reach = float(np.hypot(P[:, 0], P[:, 2]).max())
    print(f"{col} {label}: min {np.round(lo, 4).tolist()} max {np.round(hi, 4).tolist()} reach {reach:.4f}")
