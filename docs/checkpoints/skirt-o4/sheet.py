"""skirt-o4 review sheet: python3 docs/checkpoints/skirt-o4/sheet.py (repo root, Pillow) rebuilds sheet.jpg from the eight
skirt-o4 JPEGs that make shots-piece SCRIPT=docs/checkpoints/skirt-o4/skirt-o4.json writes (2 x 4 grid, 800x500 cells)."""
import pathlib

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/skirt-o4"
NAMES = ["01-play-default", "02-overview", "03-close-meadow-road", "04-close-forest", "05-close-house",
         "06-close-beach", "07-close-river", "08-close-river-lip"]
W, H, TOP = 800, 500, 40
TITLE = ("Hex World tile skirts, O4 Cutaway shipped (soil set + water cut walls), play scenario, seed 1, "
         "rich-3 (ACES fitted EV100 12.5, shadow lift)")
try:
    font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 14)
except OSError:
    font = ImageFont.load_default()
sheet = Image.new("RGB", (2 * W, TOP + 4 * H), (16, 18, 17))
d = ImageDraw.Draw(sheet)
d.text((12, 12), TITLE, fill=(230, 230, 230), font=font)
for i, n in enumerate(NAMES):
    im = Image.open(DOCS / f"skirt-o4-{n}.jpg").convert("RGB").resize((W, H), Image.LANCZOS)
    x, y = (i % 2) * W, TOP + (i // 2) * H
    sheet.paste(im, (x, y))
    d.text((x + 10, y + 8), n, fill=(240, 240, 240), font=font)
sheet.save(DOCS / "sheet.jpg", quality=85)
print("wrote", DOCS / "sheet.jpg", sheet.size)
