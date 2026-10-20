"""Review sheet for wildflower_single_e (refs: assets-source/refs/meadow_props ref-1, the single wildflowers'
approved meadow reference, which shows no poppy): the reference, textured headless EEVEE stills (still.py)
beside the four sibling single wildflowers and a 1.7 m figure, play-scenario shots (`--tilt off`, fixed
clock) from wfe-play.json taken with integration.patch plus the land-b7 weighting and weights.patch
applied (poppies 1 pick in 10), the same play-distance view with uniform picks (wfe-uniform.json,
integration.patch alone), and the kit meadow placement (wfe-kit.json).
Writes docs/checkpoints/wildflower_single_e/sheet.jpg. Run from the repo root after run.sh:
python3 docs/checkpoints/wildflower_single_e/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/wildflower_single_e"
CELLS = [
    (ROOT / "assets-source/refs/meadow_props/ref-1.jpg", "ref-1 (approved meadow_props): the singles' species and "
                                                        "palette; no poppy in the reference"),
    (DOCS / "wfe-lineup.jpg", "EEVEE: wildflower_single (daisy), _b (buttercup), _c (cornflower), _d (clover), "
                              "_e (poppy, 0.8 m), 1.7 m figure"),
    (DOCS / "wildflower_single_e-eevee-three_quarter.jpg", "poppy, three-quarter (EEVEE): bowed hairy stem, "
                                                           "nodding bud, lobed basal leaves"),
    (DOCS / "wildflower_single_e-eevee-close.jpg", "poppy, close (EEVEE): scarlet cup, black blotch, capsule"),
    (DOCS / "wildflower_single_e-eevee-top.jpg", "poppy, top (EEVEE): the head card, four petals"),
    (DOCS / "wildflower_single_e-eevee-low_side.jpg", "poppy, low side (EEVEE)"),
    (DOCS / "wfe-play.jpg", "PLAY, weighted 1 in 10, tilt off: seven meadows at play distance "
                            "(seed 1: 32 poppies of 280 single flowers)"),
    (DOCS / "wfe-uni-play.jpg", "PLAY, uniform picks 1 in 5 (integration.patch alone), same view: 47 of 280"),
    (DOCS / "wfe-orbit.jpg", "PLAY: orbit over the start meadow, poppies among the singles and clumps"),
    (DOCS / "wfe-sun.jpg", "PLAY: poppy at (-5.11, 3.69), sun side, about 3 m (wind A)"),
    (DOCS / "wfe-wind.jpg", "PLAY: same camera 60 frames (1 s, fixed clock) later (wind B)"),
    (DOCS / "wfe-shade.jpg", "PLAY: same poppy, shade side, toward the sun, about 3 m"),
    (DOCS / "wfe-detail.jpg", "PLAY: same poppy, about 1.4 m: cup and bud above the grass"),
    (DOCS / "wfe-eye.jpg", "PLAY, walk mode, eye level 1.7 m, 2 m from the poppy"),
    (DOCS / "wfe-kit-close.jpg", "KIT, tilt off: the kit meadow (-1, 1) placement at [2.5, 3.0], on the relief"),
]
W, H, PAD, LABEL = 800, 500, 12, 28
cols, rows = 2, (len(CELLS) + 1) // 2
sheet = Image.new("RGB", (cols * (W + PAD) + PAD, rows * (H + LABEL + PAD) + PAD), (30, 30, 30))
draw = ImageDraw.Draw(sheet)
for i, (path, label) in enumerate(CELLS):
    img = Image.open(path).convert("RGB")
    img.thumbnail((W, H))
    x = PAD + (i % cols) * (W + PAD)
    y = PAD + (i // cols) * (H + LABEL + PAD)
    sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
    draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
out = DOCS / "sheet.jpg"
sheet.save(out, quality=85)
print("wrote", out)
