"""rail_tunnel_portal owner review sheet (SKILL.md "Owner checkpoints"), Blender evidence only: the approved
references ref-2 (look) and ref-3 (layout) each beside the build seen the same way, textured headless EEVEE
stills of the baked piece on a stand-in of its cutting (front, play distance, the sun-side close-up, walker eye
level, top, night with the lantern glass lit, the steam engine in the mouth), the train clearance section with
the generator's numbers, LOD1 and LOD2 stills, and an untextured six-view render. The in-engine views wait
for the rail portal tile, its runtime ground and a booked GPU window (controller).
Usage: python3 docs/checkpoints/rail_tunnel_portal/sheet.py (from the repo root; needs Pillow), after
eevee_still.py has written the portal-*.jpg stills (all views, then ref2,three_quarter at lod 1 and 2) and
clearance.py has written clearance.json."""
import json
import math
import pathlib
import textwrap

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_tunnel_portal"
RENDERS = ROOT / "assets-source/kit/rail_tunnel_portal/renders"
REFS = ROOT / "assets-source/refs/rail_tunnel_portal"
BAKE = json.loads((ROOT / "assets-source/kit/rail_tunnel_portal/bake-report.json").read_text())
COMP = json.loads((ROOT / "assets-source/kit/rail_tunnel_portal/compress-report.json").read_text())
CLEAR = json.loads((DOCS / "clearance.json").read_text())
W, H = 640, 457
COLOURS = {"train_engine": (244, 67, 54), "train_carriage": (33, 150, 243), "train_carriage_b": (76, 175, 80)}


def section(path):
    """The train clearance section: the opening, the ring and the wall top against each vehicle's LOD0
    upper envelope (mirrored), with the generator's clearance numbers."""
    w, h, k = 1700, 1000, 140.0
    img = Image.new("RGB", (w, h), (30, 30, 30))
    d = ImageDraw.Draw(img)
    ox, oy = 560, h - 90

    def p(y, z):
        return (ox + y * k, oy - z * k)

    a, j, ring, crown = CLEAR["A"], CLEAR["J"], CLEAR["ring"], CLEAR["crown"]
    d.line([p(-3.8, 0), p(3.8, 0)], fill=(120, 100, 80), width=2)
    d.line([p(-1.75, 0.22), p(1.75, 0.22)], fill=(110, 110, 110), width=3)
    d.line([p(-0.88, CLEAR["railTop"]), p(-0.80, CLEAR["railTop"])], fill=(200, 200, 200), width=4)
    d.line([p(0.80, CLEAR["railTop"]), p(0.88, CLEAR["railTop"])], fill=(200, 200, 200), width=4)
    for r, col, wd in ((a, (230, 230, 230), 3), (a + ring, (150, 150, 150), 2)):
        arc = [p(r * math.cos(math.pi * i / 90), j + r * math.sin(math.pi * i / 90)) for i in range(91)]
        d.line([p(r, 0)] + arc + [p(-r, 0)], fill=col, width=wd)
    for z, lab in ((CLEAR["faceTop"], "face top 5.57"), (CLEAR["top"], "wall top 5.85")):
        d.line([p(-3.6, z), p(3.6, z)], fill=(150, 150, 150), width=1)
        d.text(p(-3.6, z + 0.15), lab, fill=(200, 200, 200))
    d.text(p(0.1, crown + 0.08), f"crown {crown:.2f} m", fill=(230, 230, 230))
    d.text(p(-a - 0.1, j - 0.3), f"springing {j:.1f} m", fill=(230, 230, 230), anchor="ra")
    d.text(p(-a, -0.25), f"clear span {2 * a:.1f} m", fill=(230, 230, 230))
    for name, v in CLEAR["vehicles"].items():
        col = COLOURS[name]
        # The generator's conservative envelope: the highest vertex at or outside each |y|.
        env, top = [], 0.0
        for y, z in sorted(v["envelope"], reverse=True):
            top = max(top, z)
            env.append((y, top))
        env = [(env[-1][0] - 0.01, env[-1][1])] + env[::-1] + [(env[0][0], CLEAR["railTop"])]
        pts = [p(-y, z) for y, z in reversed(env)] + [p(y, z) for y, z in env]
        d.line(pts, fill=col, width=2)
        y, z = v["at"]
        r = math.hypot(y, z - j) if z > j else None
        if r:
            ex, ez = j + (z - j) * a / r, y * a / r
            d.line([p(y, z), p(ez, ex)], fill=col, width=2)
        d.ellipse([p(y, z)[0] - 4, p(y, z)[1] - 4, p(y, z)[0] + 4, p(y, z)[1] + 4], outline=col, width=2)
    rep = BAKE["portal"]["clearance"]
    lines = ["Train clearance (generator, LOD0 meshes of the committed GLBs, rail top 0.46 m):"]
    for name, v in rep["vehicles"].items():
        lines.append(f"  {name}: top {v['topM']:.2f} m ({v['topClearanceM']:.2f} m under the crown), half width "
                     f"{v['halfWidthM']:.2f} m, min radial clearance {v['minRadialClearanceM']:.3f} m at |y| {v['at'][0]:.2f}, "
                     f"z {v['at'][1]:.2f}")
    lines.append("Every portal vertex ahead of the back plane over the vehicles' joint envelope:")
    for lod, v in rep["lods"].items():
        lines.append(f"  {lod}: {v['portalSweepMarginM']:.3f} m at {v['portalSweepAt'][:3]} ({v['portalSweepAt'][3]})")
    lines.append(f"Generator floor {rep['minM']:.2f} m (asserted). Intrados at |y| 1.0: {rep['intradosAtHalfWidth']['1.0']} m, "
                 f"at 1.35: {rep['intradosAtHalfWidth']['1.35']} m.")
    wrapped = [w for t in lines for w in textwrap.wrap(t, 66, subsequent_indent="    ")]
    for i, t in enumerate(wrapped):
        d.text((1080, 40 + 22 * i), t, fill=(230, 230, 230))
    for i, (name, col) in enumerate(COLOURS.items()):
        d.text((1080, 420 + 22 * i), f"{name}: LOD0 envelope (mirrored), worst point and its radius", fill=col)
    img.save(path, quality=90)
    return path


SECTION = section(DOCS / "clearance-section.jpg")
CELLS = [
    (REFS / "ref-2.jpg", "approved reference ref-2, the look (owner 2026-11-13)"),
    (DOCS / "portal-ref2.jpg", "the build, ref-2's view: granite, buff stones, lichen, voussoirs, keystone, ferns"),
    (DOCS / "portal-close.jpg", "close-up, sun side: ring stones, jamb stack, lantern"),
    (REFS / "ref-3.jpg", "approved reference ref-3, the layout (owner 2026-11-13)"),
    (DOCS / "portal-ref3.jpg", "the build, ref-3's view: the cutting into the hill, the mound over the wall"),
    (DOCS / "portal-three_quarter.jpg", "play distance: wall top 5.85 m, the mound easing to the 4 m cover"),
    (DOCS / "portal-front.jpg", "front: opening 5.2 m, springing 2.4 m, crown 5.0 m over the tile"),
    (DOCS / "portal-eye.jpg", "walker eye level 1.7 m, 16 m out"),
    (DOCS / "portal-top.jpg", "top: face 6 m inside edge 0, turf mound to the edge"),
    (DOCS / "portal-night.jpg", "night preview: lantern glass masks B = 1 (inert until P5)"),
    (DOCS / "portal-train.jpg", "train_engine and train_carriage_b LOD0 in the mouth"),
    (SECTION, "clearance section: intrados against each vehicle's envelope"),
    (DOCS / "portal-ref2-lod1.jpg", "LOD1, ref-2's view"),
    (DOCS / "portal-ref2-lod2.jpg", "LOD2, ref-2's view"),
    (DOCS / "portal-three_quarter-lod2.jpg", "LOD2 at play distance"),
    (RENDERS / "three_quarter.jpg", "six-view render (untextured): three-quarter"),
]
COLS = 3
tris = BAKE["lodTris"]
dens = BAKE["texelDensityPxPerM"]
kb = COMP["budgets"]["ktxBytes"]
TITLE = (f"rail_tunnel_portal review sheet (Blender EEVEE on a stand-in cutting, evidence only). LOD {tris[0]}/{tris[1]}/{tris[2]} "
         f"tris; stone {dens['stone']['median']:.0f} px/m median, ferns {dens['fern']['median']:.0f}; KTX2 "
         f"{kb['bc7']:,} B BC7 / {kb['astc']:,} B ASTC; pack village, kind prop, class piece.")
NOTE = ("RL-Q1 O2: head wall about 2 m over a one-level hill under a turf mound, face 6 m inside edge 0, the road portal's look. "
        "Pending owner review.")
rows = (len(CELLS) + COLS - 1) // COLS
sheet = Image.new("RGB", (COLS * W, 60 + rows * (H + 22)), (24, 24, 24))
d = ImageDraw.Draw(sheet)
d.text((10, 8), TITLE, fill=(230, 230, 230))
d.text((10, 30), NOTE, fill=(255, 193, 7))
for i, (path, label) in enumerate(CELLS):
    x, y = (i % COLS) * W, 60 + (i // COLS) * (H + 22)
    d.text((x + 6, y + 4), label, fill=(230, 230, 230))
    img = Image.open(path).convert("RGB")
    img.thumbnail((W - 8, H - 8))
    sheet.paste(img, (x + 4, y + 22))
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print(f"wrote {out}")
