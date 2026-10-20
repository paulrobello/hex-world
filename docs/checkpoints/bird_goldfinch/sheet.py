"""Builds docs/checkpoints/bird_goldfinch/sheet.jpg, the owner review sheet for bird_goldfinch, and
copies the in-engine stills it uses next to it as JPEG.

Inputs: the approved reference (assets-source/refs/birds/ref-1.jpg, ref-3.jpg); the textured headless
EEVEE stills from scripts/blender/fauna_sheet.py (target/kit-renders/bird_goldfinch/sheet/); the
in-engine frames from goldfinch.json (target/shots/gf-*.png; run it with
`bash scripts/shots/run.sh ./target/release/hex-world --scenario play --script
docs/checkpoints/bird_goldfinch/goldfinch.json --size 1600x1000 --seed 1 --fps off --tilt off
--clock fixed --water_clock fixed`). Usage: python3 docs/checkpoints/bird_goldfinch/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

REPO = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent
SHOTS = REPO / "target" / "shots"
EEVEE = REPO / "target" / "kit-renders" / "bird_goldfinch" / "sheet"
REFS = REPO / "assets-source" / "refs" / "birds"
W, GAP, LABEL = 1800, 8, 22
# In-engine frames: the launch frame shows both species' flocks over the meadow, the flap pair is
# 6 fixed frames (0.1 s) apart.
PLAY, ROOST_A, ROOST_B, CLOSE_A = "gf-05-launch-22", "gf-03-roost-a-top", "gf-03-roost-b-top", "gf-03-roost-a-close"
FLAP = ("gf-04-flap-2a", "gf-04-flap-2b")
FLOCKS_CROP = (340, 100, 840, 380)
FLAP_CROP = (620, 230, 1020, 460)
PERCH_CROP = (590, 280, 830, 385)


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


shots = {n: load(SHOTS / f"{n}.png") for n in (PLAY, ROOST_A, ROOST_B, CLOSE_A, *FLAP)}
for n, im in shots.items():
    im.save(OUT / f"{n}.jpg", quality=88)
rows = [
    row([load(REFS / "ref-1.jpg"), load(EEVEE / "glide.png"), load(EEVEE / "flap.png"),
         load(REFS / "ref-3.jpg"), load(EEVEE / "perch.png")],
        ["approved ref-1 (form)", "EEVEE: glide", "EEVEE: flap up", "approved ref-3 (roost)",
         "EEVEE: perched (fold)"]),
    row([load(EEVEE / "pair_glide.png"), load(EEVEE / "pair_perch.png")],
        ["at scale, real size (both drawn x4): goldfinch | starling, gliding",
         "perched, feet on one line: goldfinch (front) | starling"]),
    row([shots[PLAY], zoom(shots[PLAY], FLOCKS_CROP, 3)],
        ["in engine, play distance (camera ~70 m over the oaks): a goldfinch and a starling flock launch",
         "the same frame, 3x: starlings (blue) top right, goldfinches (tan, yellow) bottom left"]),
    row([shots[ROOST_A], shots[ROOST_B]],
        ["roost: goldfinch flock on one oak's 12 perches", "roost: starling flock on the other oak"]),
    row([zoom(shots[ROOST_A], PERCH_CROP, 3), shots[CLOSE_A]],
        ["roost close-up, 3x: goldfinches folded on the canopy", "roost close-up, low camera"]),
    row([zoom(shots[FLAP[0]], FLAP_CROP, 2), zoom(shots[FLAP[1]], FLAP_CROP, 2)],
        ["flap pair, 2x: goldfinches settling, t", "flap pair: t + 0.1 s (6 fixed frames)"]),
]
sheet = Image.new("RGB", (W, sum(r.height for r in rows) + GAP * (len(rows) - 1)), (24, 24, 26))
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height + GAP
sheet.save(OUT / "sheet.jpg", quality=86)
print(f"wrote {OUT / 'sheet.jpg'} {sheet.size}")
