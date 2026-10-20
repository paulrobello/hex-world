"""Review sheet for the forest-floor variants, fern_clump_b, fern_clump_c and log_fallen_b (PRD §6
Variety, refs approved 2026-10-13): the approved references, textured headless EEVEE stills beside the
originals, and the in-engine shots (`--tilt off`) from forest_floor_variants.json on the forest at
(-3, 1). Run from the repo root after run.sh: python3 docs/checkpoints/forest_floor_variants/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/forest_floor_variants"
REFS = ROOT / "assets-source/refs/forest"
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved): ferns and the fallen log"),
    (REFS / "ref-2.jpg", "ref-2 (approved): ferns tucked at the trunk"),
    (DOCS / "ferns-lineup.jpg", "EEVEE: fern_clump (original, left), fern_clump_b, fern_clump_c; 1.7 m figure behind"),
    (DOCS / "logs-lineup.jpg", "EEVEE: log_fallen (original, back), log_fallen_b (front), 1.7 m figure"),
    (DOCS / "fern_clump_b-eevee-low_side.jpg", "fern_clump_b, young upright fern, 0.48 m (EEVEE)"),
    (DOCS / "fern_clump_c-eevee-three_quarter.jpg", "fern_clump_c, wide low clump, 1.9 m across (EEVEE)"),
    (DOCS / "log_fallen_b-eevee-break.jpg", "log_fallen_b, the splintered solid break (EEVEE)"),
    (DOCS / "log_fallen_b-eevee-moss_flank.jpg", "log_fallen_b, moss streaks down the +y flank (EEVEE)"),
    (DOCS / "var-play.jpg", "in engine, play distance, tilt off: the variants' forest at (-3, 1)"),
    (DOCS / "var-overview-sun.jpg", "in engine: the variants with an original fern and log, sun side"),
    (DOCS / "var-log-sun.jpg", "log_fallen_b close-up, sun side (about 4 m)"),
    (DOCS / "var-log-shade.jpg", "log_fallen_b, shade side (about 4 m); its butt cap gets sky light only and reads "
                                 "dark like log_fallen's cap in var-fern-c-shade"),
    (DOCS / "var-fern-b-sun.jpg", "fern_clump_b close-up, sun side (about 3 m)"),
    (DOCS / "var-fern-b-shade.jpg", "fern_clump_b close-up, shade side, toward the sun (about 3 m)"),
    (DOCS / "var-fern-c-sun.jpg", "fern_clump_c close-up, sun side (about 4 m)"),
    (DOCS / "var-fern-c-shade.jpg", "fern_clump_c close-up, shade side, toward the sun (about 4 m)"),
    (DOCS / "var-eye.jpg", "eye level 1.7 m across the variants' forest"),
    (DOCS / "var-scale.jpg", "scale: the variants and the originals beside a spruce"),
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
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg")
