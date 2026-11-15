"""Owner review sheet for rail_tunnel_bore and the open rail_tunnel_portal: tunnel_still.py's stills in a
grid with captions, numbers from the committed bake reports.

  python3 docs/checkpoints/rail_tunnel_bore/sheet.py

Reads docs/checkpoints/rail_tunnel_bore/tunnel-*.jpg and writes sheet.jpg beside them."""
import json
import pathlib

from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
bore = json.loads((REPO / "assets-source/kit/rail_tunnel_bore/bake-report.json").read_text())
portal = json.loads((REPO / "assets-source/kit/rail_tunnel_portal/bake-report.json").read_text())
b, tr = bore["bore"], bore["bore"]["trains"]
CELLS = [
    ("tunnel-orbit", "Complete tunnel, orbit camera: portal, two bores (the second turned half round), portal. "
                     f"Ridge crest {b['ridge']['crestM']} m, cover {min(b['ridge']['coverOverLiningM']):.2f} m over the crown."),
    ("tunnel-side", "Closer: the ridge rises out of portal A's mound and runs level over both cover cells; "
                    "its foot dips 6 cm under the 4.0 m meadow by |s| = 7.4 m."),
    ("tunnel-orbit-closed", "Lone portals (closed state, `_open` hidden): the approved look. The six kit renders "
                            "match the approved ones byte for byte."),
    ("tunnel-mouth", "Into the mouth: the stone lining from the soffit to edge 0 and on through the bore; "
                     "the barrel, dark lining and back plane are hidden."),
    ("tunnel-train", f"Engine in the first bore. Radial clearance: engine {tr['train_engine']['minRadialClearanceM']:.3f} m, "
                     f"carriage B {tr['train_carriage_b']['minRadialClearanceM']:.3f} m; "
                     f"lining over the envelope {b['liningSweepMarginM'][0]:.2f} m (LOD0)."),
    ("tunnel-train_side", f"Lamp and carriage: lamps {b['lampLateralMarginM']:.3f} m clear of the widest vehicle "
                          f"(half width {tr['train_carriage_b']['halfWidthM']} m), glass 2.0 to 2.28 m."),
    ("tunnel-seam", "Portal to bore at edge 0 (x 13.856): same ring, same panel pixels, the track's section. "
                    "The bright floor wedge is sunlight through the mouth."),
    ("tunnel-night", "Night: the bore's lamps are always lit (emissive map); the portal lanterns glow with masks B."),
    ("tunnel-orbit-lod1", f"LOD1: bore {bore['lodTris'][1]} tris, portal {portal['lodTris'][1]}."),
    ("tunnel-orbit-lod2", f"LOD2: bore {bore['lodTris'][2]} tris, portal {portal['lodTris'][2]}."),
]
W, H, PAD, CAP = 700, 500, 12, 64
cells = [(n, c) for n, c in CELLS if (HERE / f"{n}.jpg").is_file()]
cols = 2
rows = (len(cells) + cols - 1) // cols
TITLE = 70
sheet = Image.new("RGB", (cols * (W + PAD) + PAD, TITLE + rows * (H + CAP + PAD) + PAD), (24, 24, 26))
d = ImageDraw.Draw(sheet)
try:
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 17)
    big = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 22)
except OSError:
    font = big = ImageFont.load_default()
d.text((PAD, 14), f"rail_tunnel_bore (new, {bore['lodTris'][0]} / {bore['lodTris'][1]} / {bore['lodTris'][2]} tris) and "
       f"rail_tunnel_portal open slots ({portal['lodTris'][0]} / {portal['lodTris'][1]} / {portal['lodTris'][2]}): "
       "section O1, Blender EEVEE stand-in", fill=(230, 230, 230), font=big)


def wrap(text, width=92):
    out, line = [], ""
    for w in text.split():
        if len(line) + len(w) + 1 > width:
            out.append(line)
            line = w
        else:
            line = f"{line} {w}".strip()
    return out + [line]


for k, (name, cap) in enumerate(cells):
    x, y = PAD + (k % cols) * (W + PAD), TITLE + (k // cols) * (H + CAP + PAD)
    img = Image.open(HERE / f"{name}.jpg").convert("RGB").resize((W, H), Image.LANCZOS)
    sheet.paste(img, (x, y))
    for i, line in enumerate(wrap(cap)[:3]):
        d.text((x + 4, y + H + 4 + 19 * i), line, fill=(220, 220, 220), font=font)
sheet.save(HERE / "sheet.jpg", quality=85)
print(f"wrote {HERE / 'sheet.jpg'} ({len(cells)} stills)")
