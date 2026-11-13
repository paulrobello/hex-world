"""Builds docs/checkpoints/outcrop_granite_b/review-sheet.jpg: the approved ref-2, then outcrop_granite (left)
and outcrop_granite_b (right) from the same six views, from the stills `still.py` writes.
Usage: python3 docs/checkpoints/outcrop_granite_b/sheet.py <stills dir> <out.jpg>"""
import sys

from PIL import Image, ImageDraw, ImageFont

stills, out = sys.argv[1], sys.argv[2]
VIEWS = [("three_quarter", "three-quarter, 1.7 m figure at the right"), ("play", "play-like height and pitch"),
         ("top", "straight down"), ("close_sun", "close, sun side (about 6 m)"),
         ("close_shade", "close, shade side (about 6 m)"), ("eye", "eye level 1.7 m")]
W, H, PAD, LABEL = 800, 500, 12, 28
font = ImageFont.load_default(size=18)


def cell(path, label):
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + LABEL), (24, 24, 24))
    ImageDraw.Draw(c).text((6, 4), label, fill=(225, 225, 225), font=font)
    c.paste(im, ((W - im.width) // 2, LABEL))
    return c


rows = [[cell("assets-source/refs/boulders/ref-2.jpg", "ref-2 (approved A25): the whaleback outcrop"),
         None]]
for view, text in VIEWS:
    rows.append([cell(f"{stills}/outcrop_granite-{view}.png", f"outcrop_granite (a): {text}"),
                 cell(f"{stills}/outcrop_granite_b-{view}.png", f"outcrop_granite_b (b, new): {text}")])
note = Image.new("RGB", (W, H + LABEL), (24, 24, 24))
d = ImageDraw.Draw(note)
for i, line in enumerate([
        "outcrop_granite_b, built ahead, pending owner review.",
        "Headless EEVEE stills of LOD0 with the baked maps bound;",
        "evidence only (Blender's view transform is not Bevy's).",
        "In-engine views are taken by the orchestrator.",
        "",
        "a: 7.94 x 4.96 x 1.21 m, centred crest, main joint along",
        "   the long axis with two branches and a hairline.",
        "b: 7.41 x 5.34 x 1.21 m, blunt and broad, high at one end",
        "   with a saddle; at the high end a level slab drops into a",
        "   curved cross joint over a bevelled edge (pass 2: no lip),",
        "   a long joint runs down the low tail with one branch,",
        "   a hairline lies near a rim.",
        "Same granite bake family: albedo luma 0.473 vs 0.472,",
        "AO mean 0.916 vs 0.909, moss 7.2% vs 7.5%."]):
    d.text((16, 16 + i * 30), line, fill=(225, 225, 225), font=font)
rows[0][1] = note
sheet = Image.new("RGB", (2 * W + 3 * PAD, len(rows) * (H + LABEL + PAD) + PAD), (40, 40, 40))
for r, row in enumerate(rows):
    for k, c in enumerate(row):
        sheet.paste(c, (PAD + k * (W + PAD), PAD + r * (H + LABEL + PAD)))
sheet.save(out, quality=85)
print("wrote", out, sheet.size)
