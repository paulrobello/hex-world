# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==12.2.0"]
# ///
"""Turns the raw `hex-world --thumbs` frames into the Legend inventory's tile thumbnails (PRD §5.3).

Reads `target/shots/thumbs/plan.json` and its `<tile>-r<k>.png` frames (rendered at 4x against a
flat key colour), and writes:

- `assets/thumbs/<tile>-r<k>.webp`: 96x96 RGBA, one per distinct rotation. Alpha is the key mask
  box-filtered 4x4 in premultiplied space, so edges are anti-aliased with no key-colour fringe.
  Translucent water (river and ocean cut faces) shows the key through it; those pixels are unmixed
  from the key into partial alpha instead of keeping a magenta tint.
- `assets/thumbs/manifest.json`: every tile's six rotations mapped to their images.
- `docs/checkpoints/thumbs/contact-sheet.png`: every (tile, rotation) on the Legend sheet colour.

Pillow is pinned so the WebP encoder (libwebp) and the label font stay fixed: same frames, same
bytes. Run through `make thumbs`.
"""

from __future__ import annotations

import io
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "target/shots/thumbs"
OUT = ROOT / "assets/thumbs"
SHEET_DIR = ROOT / "docs/checkpoints/thumbs"

SIZE = 96  # The 48 CSS px `.row__glyph` (design/ui/legend/style.css) at 2x for HiDPI.
CSS_SIZE = 48
KEY = (255, 0, 255)  # thumbs.rs clear colour; it reaches the frame untonemapped.
KEY_TOLERANCE = 2  # Max channel distance from the key that still counts as background.
# Magenta cast min(r, b) - g above which a pixel is treated as a blend with the key. No opaque
# kit or placeholder surface exceeds 8; translucent water over the key reaches 255.
CAST_THRESHOLD = 16
WEBP = dict(format="WEBP", quality=85, method=6, alpha_quality=100, exact=False)

SHEET = (18, 39, 58)  # Legend --sheet
INK = (238, 243, 246)  # Legend --ink
INK_3 = (132, 156, 176)  # Legend --ink-3


def fail(msg: str) -> None:
    print(f"thumbs: {msg}", file=sys.stderr)
    sys.exit(1)


def key_out(frame: Image.Image, name: str) -> Image.Image:
    """Premultiplied RGBa frame with the key removed. The corners must all be the key: a piece
    touching the frame edge would be clipped.

    A pixel P that blends foreground F with the key K = (255, 0, 255) at coverage a is
    P = a F + (1 - a) K, so its magenta cast min(r, b) - g is about (1 - a) 255 when F has no cast
    of its own. That gives a; the premultiplied colour is a F = P - (1 - a) K."""
    rgb = frame.convert("RGB")
    w, h = rgb.size
    corners = {rgb.getpixel(p) for p in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]}
    if len(corners) != 1:
        fail(f"{name}: corners {corners} are not one key colour")
    key = corners.pop()
    if max(abs(a - b) for a, b in zip(key, KEY)) > KEY_TOLERANCE:
        fail(f"{name}: background {key} is not the key {KEY}")
    diff = ImageChops.difference(rgb, Image.new("RGB", rgb.size, key))
    dr, dg, db = diff.split()
    dist = ImageChops.lighter(ImageChops.lighter(dr, dg), db)
    r, g, b = rgb.split()
    cast = ImageChops.subtract(ImageChops.darker(r, b), g)
    blended = cast.point(lambda v: 255 if v > CAST_THRESHOLD else 0)
    alpha = Image.composite(ImageChops.invert(cast), Image.new("L", rgb.size, 255), blended)
    foreground = dist.point(lambda v: 255 if v > KEY_TOLERANCE else 0)
    alpha = Image.composite(alpha, Image.new("L", rgb.size, 0), foreground)
    border = [
        alpha.crop((0, 0, w, 1)),
        alpha.crop((0, h - 1, w, h)),
        alpha.crop((0, 0, 1, h)),
        alpha.crop((w - 1, 0, w, h)),
    ]
    if any(edge.getbbox() for edge in border):
        fail(f"{name}: the tile touches the frame edge; widen the thumbs camera (thumbs.rs)")
    key_share = ImageChops.invert(alpha)
    channels = [
        ImageChops.darker(ImageChops.subtract(r, key_share), alpha),
        ImageChops.darker(g, alpha),
        ImageChops.darker(ImageChops.subtract(b, key_share), alpha),
    ]
    return Image.merge("RGBa", [*channels, alpha])


def downsample(rgba: Image.Image, name: str) -> Image.Image:
    w, h = rgba.size
    if w != h or w % SIZE:
        fail(f"{name}: raw frame {w}x{h} is not a square multiple of {SIZE}")
    # Premultiplied, so transparent key pixels add nothing to an edge pixel's colour.
    return rgba.reduce(w // SIZE).convert("RGBA")


def encode(img: Image.Image, **kw) -> bytes:
    buf = io.BytesIO()
    img.save(buf, **kw)
    return buf.getvalue()


def contact_sheet(plan: dict, images: dict[str, Image.Image]) -> Image.Image:
    font = ImageFont.load_default(size=13)
    small = ImageFont.load_default(size=11)
    label_w, cell_w, cell_h, head_h, pad = 150, SIZE + 16, SIZE + 22, 28, 12
    tiles = plan["tiles"]
    w = pad * 2 + label_w + 6 * cell_w
    h = pad * 2 + head_h + len(tiles) * cell_h
    sheet = Image.new("RGB", (w, h), SHEET)
    draw = ImageDraw.Draw(sheet)
    for k in range(6):
        x = pad + label_w + k * cell_w + cell_w // 2
        draw.text((x, pad + head_h // 2), f"rotation {k}", font=font, fill=INK, anchor="mm")
    for row, tile in enumerate(tiles):
        y = pad + head_h + row * cell_h
        tid, canon = tile["id"], tile["rotations"]
        distinct = len(set(canon))
        draw.text((pad, y + SIZE // 2 - 8), tid, font=font, fill=INK, anchor="lm")
        draw.text((pad, y + SIZE // 2 + 9), f"{distinct} distinct", font=small, fill=INK_3, anchor="lm")
        for k in range(6):
            c = canon[k]
            x = pad + label_w + k * cell_w + (cell_w - SIZE) // 2
            img = images[f"{tid}-r{c}"]
            sheet.paste(img, (x, y), img)
            caption = f"r{k}" if c == k else f"r{k} = r{c}"
            colour = INK if c == k else INK_3
            draw.text((x + SIZE // 2, y + SIZE + 9), caption, font=small, fill=colour, anchor="mm")
    return sheet


def main() -> None:
    plan_path = RAW / "plan.json"
    if not plan_path.is_file():
        fail(f"{plan_path} is missing; run `hex-world --thumbs` first (make thumbs)")
    plan = json.loads(plan_path.read_text())

    OUT.mkdir(parents=True, exist_ok=True)
    SHEET_DIR.mkdir(parents=True, exist_ok=True)
    images: dict[str, Image.Image] = {}
    files: dict[str, str] = {}
    webp_total = png_total = 0
    manifest_tiles: dict[str, list[str]] = {}
    for tile in plan["tiles"]:
        tid, canon = tile["id"], tile["rotations"]
        if len(canon) != 6 or any(canon[c] != c for c in canon):
            fail(f"{tid}: bad canonical rotations {canon}")
        for k in sorted(set(canon)):
            stem = f"{tid}-r{k}"
            raw = RAW / f"{stem}.png"
            if not raw.is_file():
                fail(f"{raw} is missing")
            small = downsample(key_out(Image.open(raw), stem), stem)
            webp = encode(small, **WEBP)
            png_total += len(encode(small, format="PNG", optimize=True))
            webp_total += len(webp)
            (OUT / f"{stem}.webp").write_bytes(webp)
            # The contact sheet shows the shipped pixels, decoded back from the WebP.
            images[stem] = Image.open(io.BytesIO(webp)).convert("RGBA")
            files[stem] = f"{stem}.webp"
        manifest_tiles[tid] = [files[f"{tid}-r{c}"] for c in canon]
        print(f"thumbs: {tid}: {len(set(canon))} distinct of 6 rotations {canon}")

    keep = set(files.values()) | {"manifest.json"}
    for stale in OUT.iterdir():
        if stale.name not in keep:
            stale.unlink()
            print(f"thumbs: removed stale {stale.relative_to(ROOT)}")

    manifest = {
        "about": "Per-rotation tile thumbnails for the Legend inventory (PRD §5.3). "
        "tiles[id][k] is the image for rotation k (local edge e faces world edge e + k); "
        "rotations with equal world-edge sockets share one image. Generated by `make thumbs`.",
        "size": [SIZE, SIZE],
        "css_size": [CSS_SIZE, CSS_SIZE],
        "format": "webp",
        "camera": plan["camera"],
        "tiles": manifest_tiles,
    }
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")

    sheet_path = SHEET_DIR / "contact-sheet.png"
    contact_sheet(plan, images).save(sheet_path, format="PNG", optimize=True)

    entries = sum(len(v) for v in manifest_tiles.values())
    print(
        f"thumbs: {len(files)} images for {entries} (tile, rotation) entries, "
        f"{SIZE}x{SIZE} WebP q{WEBP['quality']}: {webp_total} bytes "
        f"(lossless PNG would be {png_total} bytes)"
    )
    print(f"thumbs: wrote {OUT.relative_to(ROOT)}/manifest.json and {sheet_path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
