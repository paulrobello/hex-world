import sys, json
from PIL import Image, ImageChops
def diff(a, b):
    A = Image.open(a).convert("RGBA"); B = Image.open(b).convert("RGBA")
    if A.size != B.size: return {"size_mismatch": [A.size, B.size]}
    d = ImageChops.difference(A, B)
    px = d.getdata()
    n = sum(1 for p in px if p != (0, 0, 0, 0))
    big = sum(1 for p in px if max(p[:3]) > 5)
    return {"pixels": A.size[0] * A.size[1], "differing": n, "differing_over_5": big, "max": max(max(p[:3]) for p in px), "bbox": d.getbbox()}
d = sys.argv[1]
out = {}
for cam in ("close", "overview"):
    for x, y in (("mainA", "mainB"), ("mainA", "branch"), ("mainB", "branch")):
        out[f"{cam} {x} vs {y}"] = diff(f"{d}/{x}-{cam}.png", f"{d}/{y}-{cam}.png")
print(json.dumps(out, indent=1))
