"""Review sheet for sapling_spruce_c (refs approved 2026-10-13, forest ref-1: "undergrowth of ferns, shrubs
and saplings"): the approved reference, textured headless EEVEE stills (still.py) beside sapling_spruce,
sapling_spruce_b, tree_forest_c and a 1.7 m figure, kit-scenario shots (`--tilt off --clock fixed`) from
saplingc-kit.json on the undergrowth forest (-3, 2) including a wind pair 60 fixed frames (1 s) apart,
and play-scenario shots (`--tilt off`) from saplingc-play.json taken with
docs/checkpoints/sapling_spruce_c/integration.patch applied (all three sapling variants drawn by the
pine forest's sapling rule). Writes docs/checkpoints/sapling_spruce_c/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/sapling_spruce_c/sheet.py"""
import json
import pathlib

from PIL import Image, ImageChops, ImageDraw, ImageStat

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/sapling_spruce_c"
SHOTS = ROOT / "target/shots"
state = json.loads((SHOTS / "saplingc-play-state.json").read_text())["scatter"]["drawn_by_piece"]
drawn = ", ".join(f"{state.get(p, 0)} {p}" for p in ("sapling_spruce", "sapling_spruce_b", "sapling_spruce_c"))
# Wind pair: share of pixels that changed by more than 8 levels between the two frames, on the
# sapling's crown against bare litter (TAA and grain only) and the spruces behind it.
a, b = (Image.open(SHOTS / f"saplingc-kit-{n}.png").convert("L") for n in ("sun", "wind"))
w, h = a.size


def moved(x0, y0, x1, y1):
    box = (int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h))
    d = ImageChops.difference(a.crop(box), b.crop(box)).point(lambda p: 255 if p > 8 else 0)
    return ImageStat.Stat(d).mean[0] / 255


crown, litter, spruces = moved(0.44, 0.30, 0.58, 0.62), moved(0.05, 0.85, 0.35, 0.97), moved(0.0, 0.0, 0.3, 0.35)
CELLS = [
    (ROOT / "assets-source/refs/forest/ref-1.jpg", "ref-1 (approved): undergrowth of ferns, shrubs and saplings"),
    (DOCS / "saplingc-lineup.jpg", "EEVEE: sapling_spruce (2.45 m), sapling_spruce_b (1.95 m), sapling_spruce_c "
                                   "(2.25 m), tree_forest_c behind, 1.7 m figure"),
    (DOCS / "sapling_spruce_c-eevee-three_quarter.jpg", "sapling_spruce_c, three-quarter: crooked bare stem, "
                                                        "five sparse level tiers (EEVEE)"),
    (DOCS / "sapling_spruce_c-eevee-low_side.jpg", "sapling_spruce_c, low side (EEVEE)"),
    (DOCS / "saplingc-kit-play.jpg", "KIT, tilt off: undergrowth forest (-3, 2), sapling_spruce_c at [5, -4], "
                                     "sapling_spruce_b at [0, 2], sapling_spruce at [-2.5, 1.5]"),
    (DOCS / "saplingc-kit-eye.jpg", "KIT, eye level 1.7 m: the undergrowth, sapling_spruce_c at the right"),
    (DOCS / "saplingc-kit-sun.jpg", "KIT: sapling_spruce_c close up, sun side (+x+z), wind frame A"),
    (DOCS / "saplingc-kit-wind.jpg", f"KIT: same camera 60 fixed frames (1 s) later, wind frame B: pixels changed "
                                     f">8 levels: crown {crown:.1%}, bare litter {litter:.1%}, spruces behind {spruces:.1%}"),
    (DOCS / "saplingc-kit-shade.jpg", "KIT: sapling_spruce_c close up, shade side (-x-z, toward the sun)"),
    (DOCS / "saplingc-play.jpg", f"PLAY, patch applied, tilt off: six pine forests (drawn: {drawn})"),
    (DOCS / "saplingc-sun.jpg", "PLAY: about 4.4 m from +x, sun side: sapling_spruce_c at (79.25, -27.52) "
                                "near the open east edge of tile (3, -1)"),
    (DOCS / "saplingc-eye.jpg", "PLAY, walk mode, eye level 1.7 m, 3.2 m east of the same sapling"),
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
print("wrote", out, "wind moved share", round(crown, 3), round(litter, 3), round(spruces, 3))
