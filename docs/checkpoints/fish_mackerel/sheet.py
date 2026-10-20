"""Builds docs/checkpoints/fish_mackerel/sheet.jpg, the owner review sheet for fish_mackerel, and copies the
in-engine stills it uses next to it as JPEG.

Inputs: the approved fish references (assets-source/refs/fish/ref-3.jpg, ref-2.jpg); the textured headless
EEVEE stills from scripts/blender/fauna_sheet.py (target/kit-renders/fish_mackerel/sheet/); the in-engine
frames from mackerel.json (target/shots/mk-*.png; run it as make_script.py's docstring says).
Usage: python3 docs/checkpoints/fish_mackerel/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

REPO = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
SHOTS = REPO / "target" / "shots"
EEVEE = REPO / "target" / "kit-renders" / "fish_mackerel" / "sheet"
REFS = REPO / "assets-source" / "refs" / "fish"
W, GAP, LABEL = 1800, 8, 22
SEA, RIVER = "mk-sea-play", "mk-river-play"
SEA_PAIR, RIVER_PAIR = ("mk-sea-pair-a", "mk-sea-pair-b"), ("mk-river-pair-a", "mk-river-pair-b")
SEA_CROP, RIVER_CROP = (700, 420, 1050, 620), (680, 110, 980, 300)
SCHOOL_CROP = (380, 330, 1000, 560)


def load(p):
    return Image.open(p).convert("RGB")


def fit_h(im, h):
    return im.resize((round(im.width * h / im.height), h), Image.LANCZOS)


def zoom(im, box, k):
    c = im.crop(box)
    return c.resize((c.width * k, c.height * k), Image.NEAREST)


def row(images, labels, width=W):
    """Images side by side, scaled to one height so the row is `width` wide, each with a label."""
    h0 = 400
    scaled = [fit_h(i, h0) for i in images]
    total = sum(i.width for i in scaled) + GAP * (len(scaled) - 1)
    h = round(h0 * (width - GAP * (len(scaled) - 1)) / (total - GAP * (len(scaled) - 1)))
    scaled = [fit_h(i, h) for i in images]
    out = Image.new("RGB", (width, h + LABEL), (24, 24, 26))
    d = ImageDraw.Draw(out)
    x = 0
    for im, text in zip(scaled, labels):
        out.paste(im, (x, LABEL))
        d.text((x + 6, 4), text, fill=(235, 235, 235))
        x += im.width + GAP
    return out


shots = {n: load(SHOTS / f"{n}.png") for n in (SEA, RIVER, *SEA_PAIR, *RIVER_PAIR)}
for n, im in shots.items():
    im.save(OUT / f"{n}.jpg", quality=88)
rows = [
    row([load(REFS / "ref-3.jpg"), load(REFS / "ref-2.jpg"), load(EEVEE / "profile.png"), load(EEVEE / "above.png")],
        ["approved ref-3 (form, finish; no ref shows a mackerel)", "approved ref-2 (school layout)",
         "EEVEE: fish_mackerel, profile", "EEVEE: three-quarter from above"]),
    row([load(EEVEE / "pair_side.png"), load(EEVEE / "pair_top.png"), load(EEVEE / "pair_far.png")],
        ["at scale, real size (both drawn x2): mackerel (sea) above trout (river palette)",
         "side by side from the strategic camera", "both a few dozen px across (play size)"]),
    row([shots[SEA], zoom(shots[SEA], SCHOOL_CROP, 2)],
        ["in engine, play distance (~38 m, looking down 40 deg): the coast region's one mackerel school",
         "the same frame, 2x: mackerel, dark backs and pale bellies"]),
    row([shots[RIVER], zoom(shots[RIVER], (600, 150, 1000, 500), 2)],
        ["in engine, play distance (~34 m): trout in the river (unchanged: river palette, lifted, grown)",
         "the same frame, 2x: trout near the surface, alone and in small groups"]),
    row([zoom(shots[SEA_PAIR[0]], SEA_CROP, 2), zoom(shots[SEA_PAIR[1]], SEA_CROP, 2)],
        ["swim pair (long lens, 2x): mackerel, t", "t + 0.1 s (6 fixed frames)"]),
    row([zoom(shots[RIVER_PAIR[0]], RIVER_CROP, 2), zoom(shots[RIVER_PAIR[1]], RIVER_CROP, 2)],
        ["swim pair (long lens, 2x): trout, t", "t + 0.1 s (6 fixed frames)"]),
]
sheet = Image.new("RGB", (W, sum(r.height for r in rows) + GAP * (len(rows) - 1)), (24, 24, 26))
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height + GAP
sheet.save(OUT / "sheet.jpg", quality=86)
print(f"wrote {OUT / 'sheet.jpg'} {sheet.size}")
