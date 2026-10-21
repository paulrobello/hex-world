"""road_bend_60 inner-corner crease review sheet: docs/checkpoints/road_bend_60-crease/sheet.jpg.

  python3 docs/checkpoints/road_bend_60-crease/sheet.py      (from the repo root; needs Pillow)

Inputs, all committed beside this file:
  after/        HEAD. `make shots-piece SCRIPT=docs/checkpoints/road_bend_60-crease/crease-kit.json
                OUT=docs/checkpoints/road_bend_60-crease/after` (kit scenario, no play grass) and
                `make shots-piece SCRIPT=docs/checkpoints/road_bend_60-crease/crease-play.json
                OUT=docs/checkpoints/road_bend_60-crease/after ARGS="--scenario play --clock fixed
                --water_clock fixed"` (the tile in play between two road_straight tiles and meadows).
  before/       crease-kit.json with road_straight_edge.glb and road_bend_60_center.glb from 69f7f6f^,
                the state the crease was filed in (prefix.sh).
  only-stub/    the same with only road_straight_edge.glb from 69f7f6f^ (prefix.sh).
  only-centre/  the same with only road_bend_60_center.glb from 69f7f6f^ (prefix.sh).
"""
import pathlib

from PIL import Image, ImageDraw, ImageFont

DOCS = pathlib.Path(__file__).resolve().parent
TW, TH, LAB, HEAD, COLS = 480, 300, 24, 40, 4


def font(size):
    for f in ("/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


SECTIONS = [
    ("1. Inner corner, kit scenario (no play grass), same cameras. Before: road GLBs from 69f7f6f^ "
     "(the 2026-10-13 state the card was filed in). After: HEAD (no asset change)", [
        ("before/rb60k-corner-top", "before: top, 6 m"), ("after/rb60k-corner-top", "after: top, 6 m"),
        ("before/rb60k-corner-sun", "before: close-up, sun side"), ("after/rb60k-corner-sun", "after: close-up, sun side"),
        ("before/rb60k-corner-shade", "before: close-up, shade side"), ("after/rb60k-corner-shade", "after: close-up, shade side"),
        ("before/rb60k-far-wedge", "before: 25 m, 7 m up, wedge side"), ("after/rb60k-far-wedge", "after: 25 m, 7 m up, wedge side"),
        ("before/rb60k-far-shade", "before: 30 m, 5 m up, road side"), ("after/rb60k-far-shade", "after: 30 m, 5 m up, road side"),
        ("before/rb60k-corner-arm-a", "before: across the bisector"), ("after/rb60k-corner-arm-a", "after: across the bisector"),
    ]),
    ("2. Cause: the stub's inner verge sink slope on its true normal. Only road_straight_edge from 69f7f6f^ "
     "shows the line; only road_bend_60_center from 69f7f6f^ matches HEAD (no pixel off by 16 levels)", [
        ("only-stub/rb60k-corner-top", "stub from 69f7f6f^: top"), ("only-centre/rb60k-corner-top", "centre from 69f7f6f^: top"),
        ("only-stub/rb60k-corner-shade", "stub from 69f7f6f^: shade side"), ("only-centre/rb60k-corner-shade", "centre from 69f7f6f^: shade side"),
    ]),
    ("3. HEAD with the LOD forced (debug force_lod): no z-fight speckle where the two stubs' verge tops overlap", [
        ("after/rb60k-lod1-top", "LOD1: top"), ("after/rb60k-lod2-top", "LOD2: top"),
        ("after/rb60k-lod1-far-wedge", "LOD1: 25 m"), ("after/rb60k-lod2-far-wedge", "LOD2: 25 m"),
    ]),
    ("4. HEAD in play: road_bend_60 on (1,0) between road_straight (2,0) and (1,1), meadows around", [
        ("after/rb60c-play", "play distance"), ("after/rb60c-corner-top", "inner corner, top"),
        ("after/rb60c-corner-sun", "inner corner, sun side"), ("after/rb60c-corner-shade", "inner corner, shade side"),
        ("after/rb60c-seam-road-e0", "seam with road_straight, edge 0"), ("after/rb60c-seam-road-e1", "seam with road_straight, edge 1"),
        ("after/rb60c-seam-meadow-e5", "seam with meadow, edge 5"), ("after/rb60c-seam-meadow-e2", "seam with meadow, edge 2"),
        ("after/rb60c-corner-arm0", "inner corner from arm 0"), ("after/rb60c-corner-arm1", "inner corner from arm 1"),
        ("after/rb60c-eye", "eye level, walk mode, 1.7 m"),
    ]),
]


def main():
    rows = sum(HEAD + ((len(t) + COLS - 1) // COLS) * (TH + LAB) for _, t in SECTIONS)
    sheet = Image.new("RGB", (COLS * TW, rows), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    big, small = font(17), font(15)
    y = 0
    for title, tiles in SECTIONS:
        d.text((8, y + 10), title, fill=(235, 235, 235), font=big)
        y += HEAD
        for i, (stem, label) in enumerate(tiles):
            x, yy = (i % COLS) * TW, y + (i // COLS) * (TH + LAB)
            im = Image.open(DOCS / f"{stem}.jpg").convert("RGB").resize((TW - 4, TH - 4))
            sheet.paste(im, (x + 2, yy + 2))
            d.text((x + 6, yy + TH + 3), label, fill=(210, 210, 210), font=small)
        y += ((len(tiles) + COLS - 1) // COLS) * (TH + LAB)
    sheet.save(DOCS / "sheet.jpg", quality=85)
    print(f"wrote {DOCS / 'sheet.jpg'} {sheet.size}")


if __name__ == "__main__":
    main()
