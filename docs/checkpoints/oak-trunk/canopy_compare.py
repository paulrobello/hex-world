"""Canopy-unchanged check for the oak trunk/size pass: compares the canopy shell and leaf-card
primitives of two uncompressed (staged) oak GLBs per LOD: positions (the new ones divided by the
scale), normals and the wind UV (TEXCOORD_1). Prints the max differences as JSON.
Usage: python3 canopy_compare.py <old.glb> <new.glb> <scale>"""
import json
import struct
import sys

import numpy as np


def load(path):
    b = open(path, "rb").read()
    jl = struct.unpack_from("<I", b, 12)[0]
    doc = json.loads(b[20:20 + jl])
    bin_ = b[20 + jl + 8:]

    def acc(i):
        a = doc["accessors"][i]
        n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        bv = doc["bufferViews"][a["bufferView"]]
        off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = bv.get("byteStride", 4 * n)
        raw = np.frombuffer(bin_, np.uint8, stride * a["count"], off).reshape(a["count"], stride)
        return raw[:, :4 * n].copy().view(np.float32).reshape(-1, n)

    out = {}
    for m in doc["meshes"]:
        for p in m["primitives"]:
            mat = doc["materials"][p["material"]]["name"] if "material" in p else None
            at = p["attributes"]
            out[(m["name"], mat)] = {k: acc(at[a]) for k, a in (("pos", "POSITION"), ("nrm", "NORMAL"),
                                                                 ("wind", "TEXCOORD_1")) if a in at}
    return out


old, new, scale = load(sys.argv[1]), load(sys.argv[2]), float(sys.argv[3])
report = {}
for key, o in old.items():
    if key[1] is None or not key[1].endswith(("_canopy", "_leaves")):
        continue
    n = new.get(key)
    if n is None:
        report[f"{key[0]}/{key[1]}"] = "missing in the new GLB"
        continue
    # UV seams split vertices differently when the UV layout changes, so match every vertex to the
    # other GLB's vertex at the same (1x) position with the closest normal, both ways.
    p_old, p_new = o["pos"], n["pos"] / scale
    worst = {"posMaxDiffM": 0.0, "normalMaxDiff": 0.0, "windMaxDiff": 0.0}
    for a, b in ((n, o), (o, n)):
        pa = a["pos"] / (scale if a is n else 1.0)
        pb = b["pos"] / (scale if b is n else 1.0)
        for i in range(len(pa)):
            d = np.linalg.norm(pb - pa[i], axis=1)
            cand = np.flatnonzero(d <= d.min() + 1e-5)
            j = cand[np.argmin(np.abs(b["nrm"][cand] - a["nrm"][i]).max(1))]
            worst["posMaxDiffM"] = max(worst["posMaxDiffM"], float(d[j]))
            worst["normalMaxDiff"] = max(worst["normalMaxDiff"], float(np.abs(b["nrm"][j] - a["nrm"][i]).max()))
            worst["windMaxDiff"] = max(worst["windMaxDiff"], float(np.abs(b["wind"][j] - a["wind"][i]).max()))
    report[f"{key[0]}/{key[1]}"] = {"vertices": [len(p_old), len(p_new)], **worst}
print(json.dumps(report, indent=2))
