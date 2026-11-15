"""Draw section.jpg from section-probe.json (run section_probe.py first): python3 section_sheet.py.

Panel A, across the bore: the rail opening (the approved portal's), the cover meadow over a cover cell at
4.0 m (relief dips to 3.75 m), the vehicles' envelopes, and the outline options O1 (turf ridge) and O2
(lower bore). Panel B, along the tunnel axis (s = 0) from the portal face over the extension to edge 0
and one cover cell: the approved mound against the crown."""
import json
import math
import pathlib

from PIL import Image, ImageDraw, ImageFont

HERE = pathlib.Path(__file__).resolve().parent
P = json.loads((HERE / "section-probe.json").read_text())
SEC, COV, MND = P["section"], P["cover"], P["portalMound"]
A, J, CROWN, RING = SEC["halfSpanM"], SEC["springingM"], SEC["crownM"], SEC["ringM"]
COVER, DIP = COV["levelM"], COV["reliefMinM"]

W, H = 1600, 1180
img = Image.new("RGB", (W, H), (30, 30, 30))
d = ImageDraw.Draw(img, "RGBA")
F = ImageFont.load_default(size=22)
FS = ImageFont.load_default(size=17)
FB = ImageFont.load_default(size=28)
TXT, GRID = (230, 230, 230), (70, 70, 70)
STONE, GREEN, RED, BLUE, AMBER = (190, 190, 180), (90, 190, 80), (240, 70, 60), (90, 160, 250), (255, 193, 7)

# Panel A: across the bore, s in [-8, 8], h in [-0.6, 7.0].
ax0, ay0, sc = 60, 520, 62.0  # screen x of s = -8, screen y of h = 0, px per metre


def pa(s, h):
    return (ax0 + (s + 8.0) * sc, ay0 - h * sc)


d.text((40, 18), "A. Across the rail bore (one cover cell): the approved opening against one level of cover", font=FB, fill=TXT)
for h in range(0, 8):
    d.line([pa(-8, h), pa(8, h)], fill=GRID, width=1)
    d.text((pa(-8, h)[0] - 40, pa(-8, h)[1] - 10), f"{h} m", font=FS, fill=TXT)
# The free space above the cover ground (red), inside the intrados.
free = [pa(math.sqrt(A * A - (COVER - J) ** 2), COVER)]
for k in range(0, 181):
    t = math.pi * k / 180
    s, h = A * math.cos(t), J + A * math.sin(t)
    if h >= COVER:
        free.append(pa(s, h))
free.append(pa(-math.sqrt(A * A - (COVER - J) ** 2), COVER))
d.polygon(free, fill=(240, 70, 60, 110))
# Vehicles (mirrored envelopes: highest vertex at or outside each |y|).
for name, col in (("train_engine", (255, 193, 7, 90)), ("train_carriage_b", (120, 200, 255, 70)),
                  ("train_carriage", (200, 120, 255, 70))):
    raw = sorted(P["vehicles"][name]["envelope"], reverse=True)
    env, top = [], -1.0
    for y, z in raw:  # the silhouette: the highest point at or outside each |y|
        top = max(top, z)
        env.append((y, top))
    env.reverse()
    pts = [pa(y, z) for y, z in env] + [pa(env[-1][0], SEC["railTopM"])]
    pts = [pa(-env[-1][0], SEC["railTopM"])] + [pa(-y, z) for y, z in reversed(env)] + pts
    d.polygon(pts, fill=col, outline=col[:3] + (220,))
# The cover meadow (4.0, and the relief's 3.75 dips).
d.line([pa(-8, COVER), pa(8, COVER)], fill=GREEN, width=5)
for s0 in range(-8, 8):
    d.line([pa(s0 + 0.1, DIP), pa(s0 + 0.5, DIP)], fill=GREEN, width=2)
# The opening (intrados) and the ring's extrados.
op = [pa(A, -0.3), pa(A, J)] + [pa(A * math.cos(math.pi * k / 90), J + A * math.sin(math.pi * k / 90)) for k in range(91)] + \
    [pa(-A, J), pa(-A, -0.3)]
d.line(op, fill=STONE, width=5)
ex = [pa((A + RING) * math.cos(math.pi * k / 90), J + (A + RING) * math.sin(math.pi * k / 90)) for k in range(91)]
for a_, b_ in zip(ex[::2], ex[1::2]):
    d.line([a_, b_], fill=STONE, width=2)
# Track: ballast and rail top.
d.line([pa(-2.6, -0.05), pa(-2.5, 0.03), pa(-1.75, 0.22), pa(1.75, 0.22), pa(2.5, 0.03), pa(2.6, -0.05)], fill=(150, 150, 150), width=3)
d.line([pa(-0.84, 0.46), pa(-0.76, 0.46)], fill=TXT, width=4)
d.line([pa(0.76, 0.46), pa(0.84, 0.46)], fill=TXT, width=4)
# O1: a turf ridge over the bore (crest 5.75 m: extrados 5.45 + 0.3 m turf), easing to the cover by |s| = 7.
ridge = []
for k in range(-140, 141):
    s = k / 20
    a_ = abs(s)
    if a_ <= 2.9:
        h = 5.75
    else:
        t = min((a_ - 2.9) / (7.0 - 2.9), 1.0)
        h = 5.75 - 1.75 * t * t * (3 - 2 * t)
    ridge.append(pa(s, h))
for a_, b_ in zip(ridge[::3], ridge[1::3]):
    d.line([a_, b_], fill=AMBER, width=4)
# O2: a lower bore (springing 1.05, crown 3.65), trains faded at the face.
o2 = [pa(A, -0.3), pa(A, 1.05)] + [pa(A * math.cos(math.pi * k / 90), 1.05 + A * math.sin(math.pi * k / 90)) for k in range(91)] + \
    [pa(-A, 1.05), pa(-A, -0.3)]
for a_, b_ in zip(o2[::2], o2[1::2]):
    d.line([a_, b_], fill=BLUE, width=4)
fo = P["freeSpaceOverCover"]
v = P["vehicles"]
lx = 1080
notes = [
    (TXT, f"Opening: 5.2 m wide, springing {J} m, crown {CROWN} m (portal)"),
    (GREEN, f"Cover meadow ring {COVER} m, relief dips to {DIP} m"),
    (RED, f"Free space over the cover: crown +{fo['crownOverCoverM']} m"),
    (RED, f"  (+{fo['crownOverReliefDipM']} m at a dip), |s| < {fo['intradosHalfWidthAtCoverM']} m"),
    ((255, 193, 7), f"Engine top {v['train_engine']['topM']} m: +{v['train_engine']['overCoverM']} m"),
    ((120, 200, 255), f"Carriage B top {v['train_carriage_b']['topM']} m: +{v['train_carriage_b']['overCoverM']} m"),
    ((200, 120, 255), f"Carriage top {v['train_carriage']['topM']} m: +{v['train_carriage']['overCoverM']} m"),
    (AMBER, "O1 ridge: crest 5.75 m, back to 4.0 by |s| 7"),
    (BLUE, "O2 lower bore: crown 3.65 m (no trains)"),
]
for i, (c, t) in enumerate(notes):
    d.text((lx, 90 + 34 * i), t, font=F, fill=c)

# Panel B: along the axis (s = 0), x in [7.0, 21.0], h in [3.0, 7.0].
bx0, by0, bs = 60, 1110, 100.0


def pb(x, h):
    return (bx0 + (x - 7.0) * bs, by0 - (h - 3.0) * bs)


d.text((40, 590), "B. Along the axis (s = 0): the approved portal mound over the lining extension, then a cover cell", font=FB, fill=TXT)
for h in range(3, 8):
    d.line([pb(7.0, h), pb(21.0, h)], fill=GRID, width=1)
    d.text((pb(7.0, h)[0] - 40, pb(7.0, h)[1] - 10), f"{h} m", font=FS, fill=TXT)
xb, xe = MND["xBack"], MND["xEdge0"]
# breach range: the mound under the crown (red) and under crown + lining cover (orange)
x1, x2 = MND["moundBelowCrownPlusLiningCoverFromX"], MND["moundBelowCrownFromX"]
d.rectangle([pb(x1, 7.0), pb(xe, 3.0)], fill=(255, 140, 0, 40))
d.rectangle([pb(x2, 7.0), pb(xe, 3.0)], fill=(240, 70, 60, 70))
d.line([pb(x, h) for x, h in MND["longSectionS0"]], fill=GREEN, width=5)
d.line([pb(xe, COVER), pb(21.0, COVER)], fill=GREEN, width=5)
d.line([pb(MND["xFace"], CROWN), pb(21.0, CROWN)], fill=STONE, width=5)
for x, lab in ((MND["xFace"], "face 7.856"), (xb, "back plane 11.456"), (xe, "edge 0 13.856")):
    d.line([pb(x, 3.0), pb(x, 6.9)], fill=TXT, width=2)
    d.text((pb(x, 6.9)[0] + 6, pb(x, 6.9)[1]), lab, font=FS, fill=TXT)
d.text((pb(14.2, 6.6)[0], pb(14.2, 6.6)[1]), "cover cell (bore)", font=F, fill=TXT)
d.text((pb(14.2, 5.35)[0], pb(14.2, 5.35)[1]), "crown 5.0 m: 1.0 m over the cover", font=F, fill=STONE)
d.text((pb(14.2, 4.35)[0], pb(14.2, 4.35)[1]), "cover 4.0 m", font=F, fill=GREEN)
d.text((pb(7.3, 6.55)[0], pb(7.3, 6.55)[1]), "approved mound (s = 0)", font=F, fill=GREEN)
d.text((pb(11.95, 3.45)[0], pb(11.95, 3.45)[1]), f"mound under the crown from x {x2}", font=FS, fill=RED)
d.text((pb(11.95, 3.2)[0], pb(11.95, 3.2)[1]), f"under crown + 0.3 m from x {x1}", font=FS, fill=(255, 140, 0))
img.save(HERE / "section.jpg", quality=88)
print("wrote", HERE / "section.jpg")
