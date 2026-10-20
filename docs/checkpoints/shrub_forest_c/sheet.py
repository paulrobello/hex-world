"""Review sheet for shrub_forest_c (refs approved 2026-10-13, forest ref-1: "undergrowth of ferns, shrubs and
saplings"): the approved reference, textured headless EEVEE stills (still.py) beside shrub_forest,
shrub_forest_b, sapling_spruce, fern_clump and a 1.7 m figure, kit-scenario shots (`--tilt off`) from
shrubc-kit.json on the undergrowth forest at (-3, 2), and play-scenario shots (`--tilt off`, fixed clock)
from shrubc-play.json taken with docs/checkpoints/shrub_forest_c/integration.patch applied (all three
shrub variants drawn by the pine forest's shrub rule). Writes docs/checkpoints/shrub_forest_c/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/shrub_forest_c/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/shrub_forest_c"
CELLS = [
    (ROOT / "assets-source/refs/forest/ref-1.jpg", "ref-1 (approved): undergrowth of ferns, shrubs and saplings"),
    (DOCS / "shrubc-lineup.jpg", "EEVEE: shrub_forest (1.33 m), shrub_forest_b (1.83 m), shrub_forest_c (1.15 m, "
                                 "reach 1.25 m), sapling_spruce, fern_clump, 1.7 m figure"),
    (DOCS / "shrub_forest_c-eevee-three_quarter.jpg", "shrub_forest_c, three-quarter (EEVEE): lumpy mound, "
                                                      "eight arching leafy canes"),
    (DOCS / "shrub_forest_c-eevee-low_side.jpg", "shrub_forest_c, low side: leaves down to the litter, cane bows"),
    (DOCS / "shrub_forest_c-eevee-close.jpg", "shrub_forest_c, close (EEVEE)"),
    (DOCS / "shrub_forest_c-eevee-top.jpg", "shrub_forest_c, top (EEVEE): broken, lobed outline"),
    (DOCS / "shrubc-kit-play.jpg", "KIT, tilt off: undergrowth forest (-3, 2) at play distance, shrub_forest_c "
                                   "the pale lumpy mound right of the dark holly (bottom right: the hero oak)"),
    (DOCS / "shrubc-kit-scale.jpg", "KIT, scale: shrub_forest_c (front right) beside shrub_forest_b, "
                                    "shrub_forest and both saplings"),
    (DOCS / "shrubc-kit-sun.jpg", "KIT: close up, sun side (about 6 m)"),
    (DOCS / "shrubc-kit-shade.jpg", "KIT: close up, shade side, toward the sun"),
    (DOCS / "shrubc-kit-eye.jpg", "KIT, eye level 1.7 m"),
    (DOCS / "shrubc-play.jpg", "PLAY, patch applied, tilt off: five pine forests at play distance "
                               "(seed 1: 4 shrub_forest, 3 shrub_forest_b, 6 shrub_forest_c)"),
    (DOCS / "shrubc-play-near.jpg", "PLAY: closer, shrub_forest_c at (17.72, 1.00) and (25.77, -1.59) on tile (1, 0)"),
    (DOCS / "shrubc-sun.jpg", "PLAY: shrub_forest_c at (17.72, 1.00) in spruce shade (wind A); luma 1.66x the "
                              "litter, the holly at this spot 1.05x"),
    (ROOT / "docs/checkpoints/shrub_forest_b/shrubb-sun.jpg", "PLAY, same camera spot, before this patch: "
                                                              "shrub_forest_b at (17.72, 1.00) (holly sheet)"),
    (DOCS / "shrubc-wind.jpg", "PLAY: same camera as wind A, 60 frames (1 s, fixed clock) later: 40% of the "
                               "shrub's pixels change, 1.8% of the litter's"),
    (DOCS / "shrubc-shade.jpg", "PLAY: close up, shade side, toward the sun"),
    (DOCS / "shrubc-eye.jpg", "PLAY, walk mode, eye level 1.7 m: shrub_forest_c at (17.72, 1.00) ahead"),
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
