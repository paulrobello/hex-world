"""Builds the owner review sheet of the beach/ocean scatter checkpoint: each fixed camera before (the
baked dressing centres) and after (the stone scatter props, integration.patch applied) side by side,
then the five props' six-view three-quarter renders. Writes bos-sheet.jpg here.
Usage: python3 sheet.py (from anywhere)."""

import pathlib

from PIL import Image, ImageDraw

here = pathlib.Path(__file__).resolve().parent
root = here.parents[2]
VIEWS = ["beach-stretch", "beach-close", "pebbles-top", "ocean-stretch", "ocean-close", "far-150", "far-400"]
PROPS = ["beach_pebbles", "beach_pebbles_b", "beach_pebbles_c", "seabed_rock", "seabed_pebbles"]
CW, CH, LABEL = 800, 500, 28


def cell(path, size):
    if not path.exists():
        im = Image.new("RGB", size, (40, 40, 40))
        ImageDraw.Draw(im).text((12, 12), f"(no frame: {path.name})", fill=(220, 220, 220))
        return im
    return Image.open(path).convert("RGB").resize(size, Image.LANCZOS)


rows = []
for v in VIEWS:
    before, after = here / f"bos-before-{v}.jpg", here / f"bos-after-{v}.jpg"
    if not before.exists() and not after.exists():
        continue
    row = Image.new("RGB", (2 * CW, CH + LABEL), (24, 24, 24))
    d = ImageDraw.Draw(row)
    for i, (p, tag) in enumerate(((before, "before: baked centres"), (after, "after: scatter props"))):
        row.paste(cell(p, (CW, CH)), (i * CW, LABEL))
        d.text((i * CW + 10, 8), f"{v}  {tag}", fill=(235, 235, 235))
    rows.append(row)
pw = 2 * CW // len(PROPS)
props = Image.new("RGB", (2 * CW, pw + LABEL), (24, 24, 24))
d = ImageDraw.Draw(props)
for i, pid in enumerate(PROPS):
    props.paste(cell(root / f"assets-source/kit/{pid}/renders/three_quarter.jpg", (pw, pw)), (i * pw, LABEL))
    d.text((i * pw + 10, 8), pid, fill=(235, 235, 235))
rows.append(props)
sheet = Image.new("RGB", (2 * CW, sum(r.height for r in rows)), (24, 24, 24))
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height
sheet.save(here / "bos-sheet.jpg", quality=85)
print(f"wrote {here / 'bos-sheet.jpg'} {sheet.size}")
