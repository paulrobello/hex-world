"""Review sheet for shrub_forest_b (refs approved 2026-10-13, forest ref-1: "undergrowth of ferns, shrubs and
saplings"): the approved reference, textured headless EEVEE stills (still.py) beside shrub_forest,
bush_verge, bush_verge_b and a 1.7 m figure, kit-scenario shots (`--tilt off`) from shrubb-kit.json on the
undergrowth forest at (-3, 2), and play-scenario shots (`--tilt off`) from shrubb-play.json taken with
docs/checkpoints/shrub_forest_b/integration.patch applied (both shrub variants drawn by the pine forest's
shrub rule). Writes docs/checkpoints/shrub_forest_b/sheet.jpg.
Run from the repo root after run.sh: python3 docs/checkpoints/shrub_forest_b/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/shrub_forest_b"
CELLS = [
    (ROOT / "assets-source/refs/forest/ref-1.jpg", "ref-1 (approved): undergrowth of ferns, shrubs and saplings"),
    (DOCS / "shrubb-lineup.jpg", "EEVEE: shrub_forest (2.06 m, 1.33 m), shrub_forest_b (1.47 x 1.43 m, 1.83 m), "
                                 "bush_verge, bush_verge_b, 1.7 m figure"),
    (DOCS / "shrub_forest_b-eevee-three_quarter.jpg", "shrub_forest_b, three-quarter (EEVEE)"),
    (DOCS / "shrub_forest_b-eevee-low_side.jpg", "shrub_forest_b, low side: cone broadest low, leaves to the litter"),
    (DOCS / "shrub_forest_b-eevee-close.jpg", "shrub_forest_b, close: glossy leaves, berry clusters (EEVEE)"),
    (DOCS / "shrub_forest_b-eevee-top.jpg", "shrub_forest_b, top (EEVEE)"),
    (DOCS / "shrubb-kit-play.jpg", "KIT, tilt off: undergrowth forest (-3, 2), shrub_forest_b at the right, "
                                   "three shrub_forest, saplings, spruces"),
    (DOCS / "shrubb-kit-eye.jpg", "KIT, eye level 1.7 m: shrub_forest_b (right) beside shrub_forest and the saplings"),
    (DOCS / "shrubb-kit-sun.jpg", "KIT: close up, sun side (about 6 m)"),
    (DOCS / "shrubb-kit-shade.jpg", "KIT: close up, shade side, toward the sun"),
    (DOCS / "shrubb-play.jpg", "PLAY, patch applied, tilt off: five pine forests at play distance "
                               "(seed 1: 6 shrub_forest, 7 shrub_forest_b)"),
    (DOCS / "shrubb-play-near.jpg", "PLAY: closer, shrub_forest_b at (17.72, 1.00) and shrub_forest at (25.78, -1.59)"),
    (DOCS / "shrubb-sun.jpg", "PLAY: close up from the sun side: shrub_forest_b at (17.72, 1.00), tile (1, 0), "
                              "standing in spruce shade (open sun: the KIT close-ups)"),
    (DOCS / "shrubb-shade.jpg", "PLAY: close up, shade side, toward the sun"),
    (DOCS / "shrubb-eye.jpg", "PLAY, walk mode, eye level 1.7 m: shrub_forest_b at (17.72, 1.00) ahead"),
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
