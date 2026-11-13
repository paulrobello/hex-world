"""Byte and pixel comparison of two parity runs' PNGs: per frame, whether the files are byte-identical,
the pixels whose RGB differs at all and by more than 2/255, the largest channel difference and the
bounding box of the differing pixels. usage: python3 diff.py <dir-a> <dir-b> <label>"""
import hashlib
import json
import pathlib
import sys

from PIL import Image, ImageChops

a, b, label = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
rows = []
for pa in sorted(a.glob("flat-*.png")):
    pb = b / pa.name
    ba, bb = pa.read_bytes(), pb.read_bytes()
    ia, ib = Image.open(pa).convert("RGB"), Image.open(pb).convert("RGB")
    d = ImageChops.difference(ia, ib)
    px = list(d.get_flattened_data()) if hasattr(d, "get_flattened_data") else list(d.getdata())
    any_ = sum(1 for p in px if max(p) > 0)
    over2 = sum(1 for p in px if max(p) > 2)
    rows.append({"pair": label, "frame": pa.stem, "bytes_identical": ba == bb,
                 "sha_a": hashlib.sha1(ba).hexdigest()[:12], "sha_b": hashlib.sha1(bb).hexdigest()[:12],
                 "pixels": len(px), "differing": any_, "over_2": over2, "max": max(max(p) for p in px),
                 "bbox": d.getbbox()})
print(json.dumps(rows, indent=1))
