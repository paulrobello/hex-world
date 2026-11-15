"""road_tunnel_bore's cross-section with its numbers, drawn from the generator's own geometry. Two steps:

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/road_tunnel_bore/section.py -- <json>
  python3 docs/checkpoints/road_tunnel_bore/section.py <json> docs/checkpoints/road_tunnel_bore/section.png

The first (inside Blender, where the generator modules import) dumps the LOD0 intrados, extrados, floor profile
and lamp box to <json>; the second (Pillow) draws them at 100 px/m with the dimensions."""
import json
import math
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]


def dump(path):
    sys.path.insert(0, str(REPO / "scripts/blender"))
    sys.path.insert(0, str(REPO / "scripts/blender/kit"))
    import _road
    import _road_tunnel_bore as rb
    import _tunnel_portal as tp

    d = rb.LOD[0]
    arch = [tp.intrados(t) for t in tp.ellipse_divisions(d["arch"])]
    intr = [(rb.A, -rb.BURY)] + arch + [(-rb.A, -rb.BURY)]
    ext = ([(rb.A + rb.EXTRADOS, -rb.BURY)] + [tp.ring_pt(t, rb.EXTRADOS) for t in tp.ellipse_divisions(d["extrados"])]
           + [(-rb.A - rb.EXTRADOS, -rb.BURY)])
    lamps = []
    for side in (1, -1):
        (sw, hw), _ = rb.lamp_wall(side)
        lamps.append({"mount": [sw, hw], "s_in": sw - side * rb.LAMP_OUT, "s_back": sw + side * rb.LAMP_IN,
                      "z0": hw - rb.LAMP_DOWN, "z1": hw + rb.LAMP_UP})
    out = {"A": rb.A, "J": rb.J, "CROWN": rb.CROWN, "BURY": rb.BURY, "EXTRADOS": rb.EXTRADOS,
           "length": 2 * rb.AP, "lampX": list(rb.LAMP_X), "intrados": intr, "extrados": ext,
           "floor": rb.FLOOR_PROFILE, "lamps": lamps, "kerbTop": _road.KERB_TOP, "vergeTop": _road.VERGE_TOP,
           "roadCrown": max(z for t, z in rb.FLOOR_PROFILE if abs(t) < 3.0)}
    pathlib.Path(path).write_text(json.dumps(out, indent=1))
    print("section dumped to", path)


def draw(src, dst):
    from PIL import Image, ImageDraw, ImageFont

    g = json.loads(pathlib.Path(src).read_text())
    k, W, H = 100, 1400, 760
    ox, oy = W // 2, 600

    def p(s, h):
        return (ox - s * k, oy - h * k)  # looking along +x: +s (Blender y) to the left

    img = Image.new("RGB", (W, H), (250, 249, 246))
    dr = ImageDraw.Draw(img)
    try:
        f, fs = ImageFont.load_default(size=18), ImageFont.load_default(size=15)
    except TypeError:
        f = fs = ImageFont.load_default()
    ink, dim, warm = (30, 30, 34), (90, 90, 96), (200, 120, 40)
    for h in range(-1, 5):
        dr.line([p(5.6, h), p(-5.6, h)], fill=(228, 228, 224))
    shell = g["extrados"] + list(reversed(g["intrados"]))
    dr.polygon([p(*q) for q in shell], fill=(205, 200, 190), outline=ink)
    dr.line([p(*q) for q in g["intrados"]], fill=ink, width=3)
    dr.line([p(*q) for q in g["floor"]], fill=(60, 60, 64), width=3)
    for lp in g["lamps"]:
        s0, s1 = sorted((lp["s_in"], lp["s_back"]))
        dr.rectangle([p(s1, lp["z1"]), p(s0, lp["z0"])], fill=warm, outline=ink)

    def hdim(s0, s1, h, text):
        dr.line([p(s0, h), p(s1, h)], fill=dim, width=1)
        for s in (s0, s1):
            x, y = p(s, h)
            dr.line([(x, y - 6), (x, y + 6)], fill=dim)
        x0, y = p(s0, h)
        x1, _ = p(s1, h)
        dr.text(((x0 + x1) / 2, y - 4), text, fill=ink, font=f, anchor="md")

    def vdim(s, h0, h1, text, right=True):
        dr.line([p(s, h0), p(s, h1)], fill=dim)
        for h in (h0, h1):
            x, y = p(s, h)
            dr.line([(x - 6, y), (x + 6, y)], fill=dim)
        x, y0 = p(s, h0)
        _, y1 = p(s, h1)
        dr.text((x + (8 if right else -8), (y0 + y1) / 2), text, fill=ink, font=f, anchor="lm" if right else "rm")

    A, J, C, X = g["A"], g["J"], g["CROWN"], g["EXTRADOS"]
    hdim(A, -A, -0.55, f"inside width {2 * A:.2f} m (corridor half width {A:.2f})")
    vdim(-A - 0.95, 0.0, J, f"springing {J:.2f}")
    vdim(-0.0, 0.0, C, f"crown {C:.2f}")
    vdim(-A - 0.45 - 0.75, 0.0, C + X, f"shell top {C + X:.2f}", right=False)
    lp = g["lamps"][0]
    ms, mh = lp["mount"]
    x, y = p(ms, mh)
    dr.text((x + 70, y + 10), f"lamp mount (s {ms:.3f}, h {mh:.3f})\nbox {lp['z1'] - lp['z0']:.2f} m tall, "
            f"{abs(lp['s_back'] - lp['s_in']):.2f} m deep\nat x 0 and ±{abs(g['lampX'][0]):.4f}",
            fill=ink, font=fs)
    dr.line([(x + 68, y + 20), (x + 8, y + 2)], fill=dim)
    x, y = p(1.6, g["roadCrown"])
    dr.text((x, y - 30), f"road crown {g['roadCrown']:.2f}, kerb top {g['kerbTop']:.2f}, verge {g['vergeTop']:.2f}"
            " (road_straight section)", fill=ink, font=fs, anchor="mm")
    dr.text((20, 18), f"road_tunnel_bore cross-section (LOD0), looking along +x; length {g['length']:.4f} m, "
            f"both ends this section; shell {X:.2f} m thick", fill=ink, font=f)
    dr.text((20, 44), "h = 0 is the cell surface; floor from the road pieces; lining = the portal's lining section",
            fill=dim, font=fs)
    img.save(dst)
    print("wrote", dst)


if __name__ == "__main__":
    if "--" in sys.argv:
        dump(sys.argv[sys.argv.index("--") + 1])
    else:
        draw(sys.argv[1], sys.argv[2])
