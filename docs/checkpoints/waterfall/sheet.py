"""Builds docs/checkpoints/waterfall/refs-sheet.jpg: the waterfall reference candidates for the owner.

Run from the repo root: python3 docs/checkpoints/waterfall/sheet.py
Top row: the approved references the fall face must match (granite, river) and the in-engine cliff
column it covers. Then one row per drop, each image labelled with its ref number.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "docs/checkpoints/waterfall/refs-sheet.jpg"
W, H, LABEL, GAP = 800, 597, 36, 8
REFS = "assets-source/refs"

ROWS: list[tuple[str, list[tuple[str, str]]]] = [
    (
        "approved family (match these)",
        [
            (f"{REFS}/massif_granite/ref-2.jpg", "massif_granite ref-2 (approved)"),
            (f"{REFS}/peak_granite/ref-1.jpg", "peak_granite ref-1 (approved)"),
            (f"{REFS}/boulders/ref-1.jpg", "boulders ref-1 (approved)"),
            (f"{REFS}/river/ref-1.jpg", "river ref-1 (approved)"),
            (
                "docs/checkpoints/massif_granite/massif-cliff-raised.jpg",
                "in engine: a raised tile's soil cliff column",
            ),
        ],
    ),
    (
        "4 m drop, waterfall_l1 (RQ2 O1: straight lip)",
        [
            (f"{REFS}/waterfall/ref-{i}.jpg", f"waterfall / ref-{i} (4 m)")
            for i in (1, 2, 3)
        ],
    ),
    (
        "8 m drop, waterfall_l2 (RQ2 O2: two tiers)",
        [
            (f"{REFS}/waterfall/ref-{i}.jpg", f"waterfall / ref-{i} (8 m)")
            for i in (4, 5, 6, 10)
        ],
    ),
    (
        "12 m drop, waterfall_l3 (RQ2 O2: three tiers)",
        [
            (f"{REFS}/waterfall/ref-{i}.jpg", f"waterfall / ref-{i} (12 m)")
            for i in (7, 8, 9)
        ],
    ),
]


def font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    try:
        return ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", size)
    except OSError:
        return ImageFont.load_default(size=size)


def fit(path: Path) -> Image.Image:
    img = Image.open(path).convert("RGB")
    img.thumbnail((W, H), Image.Resampling.LANCZOS)
    cell = Image.new("RGB", (W, H), "#2a2a2a")
    cell.paste(img, ((W - img.width) // 2, (H - img.height) // 2))
    return cell


def main() -> None:
    cols = max(len(items) for _, items in ROWS)
    row_h = LABEL + LABEL + H + GAP
    sheet = Image.new(
        "RGB", (GAP + cols * (W + GAP), GAP + len(ROWS) * row_h), "#121212"
    )
    draw = ImageDraw.Draw(sheet)
    head, small = font(26), font(20)
    for r, (title, items) in enumerate(ROWS):
        top = GAP + r * row_h
        draw.text((GAP + 4, top + 4), title, fill="#e6e6e6", font=head)
        for c, (rel, label) in enumerate(items):
            left = GAP + c * (W + GAP)
            draw.rectangle(
                (left, top + LABEL, left + W - 1, top + 2 * LABEL - 1), fill="#1e1e1e"
            )
            draw.text((left + 12, top + LABEL + 7), label, fill="#e6e6e6", font=small)
            sheet.paste(fit(ROOT / rel), (left, top + 2 * LABEL))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(OUT, quality=88)
    print("wrote", OUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
