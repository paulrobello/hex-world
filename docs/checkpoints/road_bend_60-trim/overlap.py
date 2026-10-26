"""road_bend_60 inner corner: do the tile's road pieces overlap, and does the top surface change?

  python3 docs/checkpoints/road_bend_60-trim/overlap.py <before-dir> <after-dir> <out-dir>

Each dir holds staged (uncompressed) GLBs, `<dir>/<piece>/<piece>.glb`, as `make kit-build` writes
them to `target/kit-stage/`:
  before: road_straight_edge and road_bend_60_center as on main 4ea55ca (built from their unchanged
          generators with HEXW_SKIP_BAKE=1 into a scratch dir; `_road_bend.build_bend` unpatched for
          the centre)
  after:  road_bend_60_edge_e0, road_bend_60_edge_e1 and road_bend_60_center (target/kit-stage)
The pieces are placed as the tile places them (stub on lane 0 at turn 0, stub on lane 1 at turn 1,
centre at turn 0), every LOD's up-facing triangles are rasterized at 5 mm over the inner corner, and
the script reports per LOD: the area covered by two or more pieces, the part of it where they are
level (within 1 mm, the coplanar overlap), and the top surface before against after (largest height
change, cells whose top slot changed, cells covered on one side only). It writes overlap.json and
one overlay JPEG per LOD (cell colour = how many pieces cover it, each piece's triangle edges in its
own colour). Needs numpy and Pillow.
"""
import json
import math
import pathlib
import struct
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

CELL = 0.005
X0, X1, Y0, Y1 = 4.8, 9.6, -6.0, -1.6  # Blender XY around C = (6, -3.464)
PX_PER_M = 140
LAYOUTS = {
    "before": [("road_straight_edge", 0), ("road_straight_edge", 1), ("road_bend_60_center", 0)],
    "after": [("road_bend_60_edge_e0", 0), ("road_bend_60_edge_e1", 1), ("road_bend_60_center", 0)],
}
COLOURS = [(64, 160, 255), (255, 200, 40), (230, 70, 200)]
COUNT_RGB = {0: (40, 40, 40), 1: (120, 150, 110), 2: (230, 120, 30), 3: (220, 30, 30)}


def font(size):
    for f in ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def read_glb(path):
    data = path.read_bytes()
    jlen = struct.unpack_from("<I", data, 12)[0]
    doc = json.loads(data[20:20 + jlen])
    blen = struct.unpack_from("<I", data, 20 + jlen)[0]
    binary = data[28 + jlen:28 + jlen + blen]
    return doc, binary


def accessor(doc, binary, i):
    a = doc["accessors"][i]
    v = doc["bufferViews"][a["bufferView"]]
    comps = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
    dt = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[a["componentType"]]
    off = v.get("byteOffset", 0) + a.get("byteOffset", 0)
    stride = v.get("byteStride", 0)
    item = np.dtype(dt).itemsize * comps
    if stride and stride != item:
        raw = np.frombuffer(binary, np.uint8, count=stride * a["count"], offset=off).reshape(a["count"], stride)
        return raw[:, :item].copy().view(dt).reshape(a["count"], comps)
    return np.frombuffer(binary, dt, count=a["count"] * comps, offset=off).reshape(a["count"], comps)


def lod_tris(path, piece, lod, turn):
    """Triangles of `<piece>_lod<lod>` in Blender XY(Z), turned by `turn` edge steps, with slot names."""
    doc, binary = read_glb(path)
    for n in doc["nodes"]:
        if n.get("name") == f"{piece}_lod{lod}":
            assert not any(k in n for k in ("translation", "rotation", "scale", "matrix")), n
    mesh = next(m for m in doc["meshes"] if m["name"] == f"{piece}_lod{lod}")
    a = math.radians(-60.0 * turn)
    c, s = math.cos(a), math.sin(a)
    out = []
    for p in mesh["primitives"]:
        pos = accessor(doc, binary, p["attributes"]["POSITION"]).astype(np.float64)
        idx = accessor(doc, binary, p["indices"]).reshape(-1, 3)
        slot = doc["materials"][p["material"]]["name"]
        bx, by, bz = pos[:, 0], -pos[:, 2], pos[:, 1]
        xy = np.stack([bx * c - by * s, bx * s + by * c, bz], axis=1)
        out.extend((xy[t], slot) for t in idx)
    return out


def raster(tris):
    """Top height and slot per cell of one piece (NaN where uncovered), from up-facing triangles."""
    nx, ny = round((X1 - X0) / CELL), round((Y1 - Y0) / CELL)
    h = np.full((ny, nx), np.nan)
    slot = np.full((ny, nx), -1, dtype=np.int32)
    names = []
    for t, name in tris:
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
        det = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        if abs(det) < 1e-9:
            continue  # vertical (kerb faces) or degenerate: no top area
        if name not in names:
            names.append(name)
        i0 = max(0, int((min(ax, bx, cx) - X0) / CELL))
        i1 = min(nx - 1, int((max(ax, bx, cx) - X0) / CELL) + 1)
        j0 = max(0, int((min(ay, by, cy) - Y0) / CELL))
        j1 = min(ny - 1, int((max(ay, by, cy) - Y0) / CELL) + 1)
        if i0 > i1 or j0 > j1:
            continue
        px, py = np.meshgrid(X0 + (np.arange(i0, i1 + 1) + 0.5) * CELL, Y0 + (np.arange(j0, j1 + 1) + 0.5) * CELL)
        w1 = ((px - ax) * (cy - ay) - (py - ay) * (cx - ax)) / det
        w2 = ((bx - ax) * (py - ay) - (by - ay) * (px - ax)) / det
        w0 = 1 - w1 - w2
        inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        z = w0 * az + w1 * bz + w2 * cz
        sub = h[j0:j1 + 1, i0:i1 + 1]
        ss = slot[j0:j1 + 1, i0:i1 + 1]
        better = inside & (np.isnan(sub) | (z > sub))
        sub[better] = z[better]
        ss[better] = names.index(name)
    return h, slot, names


def evaluate(root, layout, lod):
    pieces = []
    for piece, turn in layout:
        tris = lod_tris(root / piece / f"{piece}.glb", piece, lod, turn)
        h, slot, names = raster(tris)
        pieces.append({"piece": piece, "turn": turn, "tris": tris, "h": h,
                       "slot": np.where(slot >= 0, np.array(names + [""], dtype=object)[slot], "")})
    hs = np.stack([p["h"] for p in pieces])
    covered = ~np.isnan(hs)
    count = covered.sum(axis=0)
    top = np.nanmax(np.where(covered, hs, -np.inf), axis=0)
    top[count == 0] = np.nan
    winner = np.argmax(np.where(covered, hs, -np.inf), axis=0)
    slots = np.stack([p["slot"] for p in pieces])
    top_slot = np.take_along_axis(slots, winner[None], 0)[0]
    top_slot[count == 0] = ""
    level = np.zeros(count.shape, dtype=bool)
    for i in range(len(pieces)):
        for j in range(i + 1, len(pieces)):
            both = covered[i] & covered[j]
            level |= both & (np.abs(np.nan_to_num(hs[i]) - np.nan_to_num(hs[j])) <= 0.001)
    return pieces, count, top, top_slot, level


def overlay(pieces, count, label, path):
    ny, nx = count.shape
    rgb = np.zeros((ny, nx, 3), dtype=np.uint8)
    for k, c in COUNT_RGB.items():
        rgb[np.minimum(count, 3) == k] = c
    img = Image.fromarray(rgb[::-1]).resize((round((X1 - X0) * PX_PER_M), round((Y1 - Y0) * PX_PER_M)), Image.NEAREST)
    d = ImageDraw.Draw(img)

    def px(x, y):
        return ((x - X0) * PX_PER_M, (Y1 - y) * PX_PER_M)

    for p, col in zip(pieces, COLOURS):
        for t, _ in p["tris"]:
            (ax, ay, _), (bx, by, _), (cx_, cy_, _) = t
            if abs((bx - ax) * (cy_ - ay) - (by - ay) * (cx_ - ax)) < 1e-9:
                continue  # vertical face: no top area
            pts = [px(v[0], v[1]) for v in t]
            d.line(pts + [pts[0]], fill=col, width=1)
    cx, cy = px(6.0, -6.0 * math.tan(math.radians(30.0)))
    d.ellipse((cx - 4, cy - 4, cx + 4, cy + 4), outline=(255, 255, 255), width=2)
    f = font(15)
    d.rectangle((0, 0, img.width, 48), fill=(0, 0, 0))
    d.text((6, 4), label, fill=(255, 255, 255), font=f)
    legend = "  ".join(f"{p['piece']} (turn {p['turn']})" for p in pieces)
    d.text((6, 26), legend, fill=(200, 200, 200), font=font(12))
    x = 6
    for p, col in zip(pieces, COLOURS):
        d.rectangle((x, img.height - 18, x + 12, img.height - 6), fill=col)
        x += 18
    d.text((x + 4, img.height - 20), "cells: grey 1 piece, orange 2, red 3; white ring = arc centre C",
           fill=(255, 255, 255), font=font(12))
    img.save(path, quality=90)


def main():
    before, after, out = (pathlib.Path(a) for a in sys.argv[1:4])
    out.mkdir(parents=True, exist_ok=True)
    report = {"cellM": CELL, "regionBlenderXY": [X0, X1, Y0, Y1], "lods": []}
    area = CELL * CELL
    for lod in range(3):
        res = {}
        for name, root in (("before", before), ("after", after)):
            pieces, count, top, top_slot, level = evaluate(root, LAYOUTS[name], lod)
            res[name] = (pieces, count, top, top_slot)
            entry = {
                "overlapM2": round(float((count >= 2).sum() * area), 4),
                "coplanarOverlapM2": round(float(level.sum() * area), 4),
                "coveredM2": round(float((count >= 1).sum() * area), 4),
            }
            report.setdefault(name, {})[f"lod{lod}"] = entry
            overlay(pieces, count, f"{name}, LOD{lod}: overlap {entry['overlapM2']} m2, level overlap "
                    f"{entry['coplanarOverlapM2']} m2", out / f"overlay-{name}-lod{lod}.jpg")
        (_, cb, tb, sb), (_, ca, ta, sa) = res["before"], res["after"]
        both = (cb >= 1) & (ca >= 1)
        dz = np.abs(tb - ta)[both]
        # A slot change counts only where the old top was not a tie between pieces of different slots.
        report["lods"].append({
            "lod": lod,
            "topMaxAbsDeltaM": round(float(dz.max()), 6),
            "topCellsOverOneMm": int((dz > 0.001).sum()),
            "topSlotChangedCells": int((sb[both] != sa[both]).sum()),
            "coveredBeforeOnlyM2": round(float(((cb >= 1) & (ca == 0)).sum() * area), 4),
            "coveredAfterOnlyM2": round(float(((ca >= 1) & (cb == 0)).sum() * area), 4),
        })
    (out / "overlap.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
