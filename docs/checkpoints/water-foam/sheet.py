"""Before/after sheet of the water-foam checkpoint: docs/checkpoints/water-foam/sheet.jpg.

  python3 docs/checkpoints/water-foam/sheet.py      (after shots.sh before and shots.sh after; Pillow)

One row per still (and the first and 46th frame of each clip), the build before the change on the
left and after it on the right, each at half size with its name."""
import pathlib

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).resolve().parent
W, H, PAD, LABEL = 640, 400, 8, 22

names = sorted(p.stem for p in (HERE / "after").glob("wf-*.jpg"))
sheet = Image.new("RGB", (2 * W + 3 * PAD, len(names) * (H + LABEL + PAD) + PAD), (24, 24, 24))
draw = ImageDraw.Draw(sheet)
for row, name in enumerate(names):
    y = PAD + row * (H + LABEL + PAD)
    for col, side in enumerate(("before", "after")):
        x = PAD + col * (W + PAD)
        src = HERE / side / f"{name}.jpg"
        draw.text((x + 4, y + 4), f"{side}: {name}", fill=(230, 230, 230))
        if src.exists():
            sheet.paste(Image.open(src).convert("RGB").resize((W, H), Image.LANCZOS), (x, y + LABEL))
sheet.save(HERE / "sheet.jpg", quality=85)
print(f"wrote {HERE / 'sheet.jpg'} ({len(names)} rows)")
