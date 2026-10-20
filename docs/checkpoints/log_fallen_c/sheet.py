"""Review sheet for log_fallen_c (PRD §6 Variety, forest refs approved 2026-10-13): the approved
reference, textured headless EEVEE stills beside log_fallen and log_fallen_b, and the in-engine shots
(`--tilt off`) from logc-kit.json (the kit forest at (-3, 1), log_fallen_c in the spruce's shade) and
logc-play.json (pine forest tiles in play). Run from the repo root after run.sh:
python3 docs/checkpoints/log_fallen_c/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/log_fallen_c"
REFS = ROOT / "assets-source/refs/forest"
CELLS = [
    (REFS / "ref-1.jpg", "ref-1 (approved): the fallen mossy log and leaf litter"),
    (DOCS / "logs-lineup.jpg", "EEVEE: log_fallen (back), log_fallen_b (middle), log_fallen_c (front); 1.7 m figure"),
    (DOCS / "logs-top.jpg", "EEVEE top view, same order: straight with stubs / short butt / bowed, mossed, open mouth"),
    (DOCS / "log_fallen_c-eevee-mouth.jpg", "log_fallen_c, the open rotten mouth, upper lip fallen away (EEVEE)"),
    (DOCS / "log_fallen_c-eevee-three_quarter.jpg", "log_fallen_c three-quarter (EEVEE)"),
    (DOCS / "log_fallen_c-eevee-low_side.jpg", "log_fallen_c low side: the C-shaped crook (EEVEE)"),
    (DOCS / "logc-kit-play.jpg", "in engine, play distance from -x/-z, tilt off: log_fallen_c is the dark log at "
                                 "the near spruce's foot, half under its crown (the others hide behind the spruce)"),
    (DOCS / "logc-play-sun.jpg", "play scenario (patch applied), a drawn log_fallen_c under the pine canopy, sun side"),
    (DOCS / "logc-play-shade.jpg", "play scenario, the same log_fallen_c from the shade side: the dark open mouth"),
    (DOCS / "logc-kit-sun.jpg", "log_fallen_c from the sun side, under the canopy (about 3.5 m)"),
    (DOCS / "logc-kit-shade.jpg", "log_fallen_c from the shade side, toward the sun (about 4.5 m)"),
    (DOCS / "logc-kit-mouth.jpg", "log_fallen_c, the mouth under the canopy (about 3.5 m)"),
    (DOCS / "logc-kit-eye.jpg", "eye level 1.7 m (fixed camera; a walk-mode eye shot in play stood in the low "
                                "spruce boughs): log_fallen_c under the spruce, log_fallen_b behind it"),
    (DOCS / "logc-play.jpg", "play scenario, six pine forest tiles from above: the floor props sit under the canopy"),
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
