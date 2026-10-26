"""Composes render_lods.py's panels into the LOD1 pass review sheet.

  python3 docs/checkpoints/lod1-pass/compose.py target/lod1-pass docs/checkpoints/lod1-pass/sheet.jpg <piece> ...

One row per piece: its name and LOD0/LOD1/LOD2 triangles (staged export-report.json), then per LOD the
whole piece, the detail view of the parts added to LOD1, and the 150 m switch-distance crop magnified
with nearest-neighbour pixels (each square block is one screen pixel at 1920 x 1080)."""
import json
import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
P = 320
LABEL = 230
HEAD = 40


def main():
    src, dst, pieces = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3:]
    w, h = LABEL + 9 * P, HEAD + len(pieces) * P
    sheet = Image.new("RGB", (w, h), (24, 24, 28))
    draw = ImageDraw.Draw(sheet)
    for k in range(3):
        for j, view in enumerate(("whole", "detail", "150 m (pixels)")):
            draw.text((LABEL + (3 * k + j) * P + 8, 12), f"LOD{k} {view}", fill=(230, 230, 230))
    for r, piece in enumerate(pieces):
        y = HEAD + r * P
        rep = json.loads((ROOT / "target" / "kit-stage" / piece / "export-report.json").read_text())
        tris = [rep["lodTris"].get(f"{piece}_lod{k}") for k in range(3)]
        draw.text((8, y + 8), piece, fill=(255, 255, 255))
        draw.text((8, y + 28), f"tris {tris[0]} / {tris[1]} / {tris[2]}", fill=(200, 200, 200))
        draw.text((8, y + 48), f"LOD1 {tris[1] / tris[0]:.1%} of LOD0", fill=(200, 200, 200))
        for k in range(3):
            for j, view in enumerate(("whole", "detail", "far")):
                im = Image.open(src / piece / f"{view}-lod{k}.png").convert("RGB")
                if view == "far":
                    s = max(1, min(P // im.width, P // im.height))
                    im = im.resize((im.width * s, im.height * s), Image.NEAREST)
                    im.thumbnail((P, P), Image.NEAREST)
                x0 = LABEL + (3 * k + j) * P
                sheet.paste(im, (x0 + (P - im.width) // 2, y + (P - im.height) // 2))
        draw.line([(0, y), (w, y)], fill=(70, 70, 80))
    for k in range(1, 3):
        x = LABEL + 3 * k * P
        draw.line([(x, 0), (x, h)], fill=(160, 160, 170), width=2)
    sheet.save(dst, quality=85)
    print(f"wrote {dst} ({w} x {h})")


main()
