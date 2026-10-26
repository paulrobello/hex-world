"""road_bend_60 inner-corner trim (owner A7) review sheet: docs/checkpoints/road_bend_60-trim/sheet.jpg.

  python3 docs/checkpoints/road_bend_60-trim/sheet.py      (from the repo root; needs Pillow)

Inputs, all committed beside this file:
  overlap/      overlap.py on the staged GLBs: before = road_straight_edge on both lanes and
                road_bend_60_center as on main 4ea55ca, after = road_bend_60_edge_e0/e1 and the
                rebuilt centre; overlay-<side>-lod<n>.jpg and overlap.json.
  before/       main 4ea55ca: `make shots-piece SCRIPT=docs/checkpoints/road_bend_60-trim/trim-kit.json
                OUT=docs/checkpoints/road_bend_60-trim/before ARGS="--clock fixed"` (kit scenario, the
                bend on (2,-1)) and `make shots-piece SCRIPT=docs/checkpoints/road_bend_60-trim/trim-play.json
                OUT=docs/checkpoints/road_bend_60-trim/before ARGS="--scenario play --clock fixed
                --water_clock fixed"` (the tile in play between two road_straight tiles and meadows).
  after/        the same two runs on this branch.
  diff/         shot_diff.py on the lossless frames (target/shots after each run): shot-diff.txt
                (before against after), noise-before-twice.txt (two before kit runs),
                noise-after-twice-play.txt (two after play runs), and before | after |
                difference x8 crops of the frames that differ most; draws.txt (draws.py), masks-e0.txt
                and masks-e1.txt (masks_diff.py), thumbs-diff.txt (thumbs_diff.py).
  thumbs/       thumbs_diff.py: the road_bend_60 thumbnails `make thumbs` changed, before (git) | after |
                difference x8.
Same cameras as docs/checkpoints/road_bend_60-crease (crease-kit.json, crease-play.json).
"""
import json
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


def pairs(stems):
    out = []
    for stem, label in stems:
        out += [(f"before/{stem}", f"before: {label}"), (f"after/{stem}", f"after: {label}")]
    return out


def sections():
    rep = json.loads((DOCS / "overlap" / "overlap.json").read_text())
    nums = ", ".join(f"LOD{i} {rep['before'][f'lod{i}']['overlapM2']} -> {rep['after'][f'lod{i}']['overlapM2']} m2"
                     for i in range(3))
    top = max(l["topMaxAbsDeltaM"] for l in rep["lods"])
    out = [
        (f"1. Overlap at the inner corner, top view of the staged GLBs placed as the tile places them. Overlap {nums}; "
         f"top surface height change {top} m, no slot change", [
            ("overlap/overlay-before-lod0", "before LOD0: 2 stubs + centre"), ("overlap/overlay-after-lod0", "after LOD0: trimmed stubs + centre"),
            ("overlap/overlay-before-lod2", "before LOD2"), ("overlap/overlay-after-lod2", "after LOD2"),
        ]),
        ("2. Inner corner, kit scenario (no play grass), same cameras: before = main 4ea55ca, after = this branch", pairs([
            ("trk-corner-top", "top, 6 m"), ("trk-corner-sun", "close-up, sun side"),
            ("trk-corner-shade", "close-up, shade side"), ("trk-corner-arm-a", "across the bisector"),
            ("trk-far-wedge", "25 m, 7 m up, wedge side"), ("trk-far-shade", "30 m, 5 m up, road side"),
            ("trk-lod1-top", "forced LOD1, top"), ("trk-lod2-top", "forced LOD2, top"),
        ])),
        ("3. In play: road_bend_60 on (1,0) between road_straight (2,0) and (1,1), meadows around", pairs([
            ("trp-play", "play distance"), ("trp-corner-top", "inner corner, top"),
            ("trp-corner-sun", "inner corner, sun side"), ("trp-corner-shade", "inner corner, shade side"),
            ("trp-corner-arm0", "inner corner from arm 0"), ("trp-corner-arm1", "inner corner from arm 1"),
            ("trp-seam-road-e0", "seam with road_straight, edge 0"), ("trp-seam-road-e1", "seam with road_straight, edge 1"),
            ("trp-eye", "eye level, walk mode, 1.7 m"),
        ])),
    ]
    out.append(("4. Where the frames differ (diff/shot-diff.txt; repeat runs differ by up to 6 levels in kit and 4 in play "
                "close-ups, diff/noise-*.txt): before | after | difference x8 around every pixel off by more than 8 levels. "
                "One-pixel lines on kerb and dash edges, and grain on the verge", [
                    ("diff/trk-corner-sun", "kit, close-up, sun side (max 47 levels, 0.023% of pixels over 8)"),
                    ("diff/trk-corner-shade", "kit, close-up, shade side (max 27, 0.021%)"),
                    ("diff/trk-far-wedge", "kit, 25 m, wedge side (max 43, 0.010%)"),
                    ("diff/trp-corner-sun", "play, inner corner, sun side (max 19, 0.013%)"),
                ], True))
    thumbs = sorted((DOCS / "thumbs").glob("*.jpg")) if (DOCS / "thumbs").is_dir() else []
    if thumbs:
        out.append(("5. road_bend_60 thumbnails that make thumbs changed: before (git) | after | difference x8",
                    [(f"thumbs/{p.stem}", p.stem) for p in thumbs], True))
    return out


def layout(tiles, wide):
    """(x, y, w, h) of each tile's image box within its section, and the section height."""
    if not wide:
        boxes = [((i % COLS) * TW, (i // COLS) * (TH + LAB), TW, TH) for i in range(len(tiles))]
        return boxes, ((len(tiles) + COLS - 1) // COLS) * (TH + LAB)
    boxes, y = [], 0
    for stem, _ in tiles:
        with Image.open(DOCS / f"{stem}.jpg") as im:
            h = min(420, round(im.height * COLS * TW / im.width))
        boxes.append((0, y, COLS * TW, h))
        y += h + LAB
    return boxes, y


def main():
    secs = [(s[0], s[1], s[2] if len(s) > 2 else False) for s in sections()]
    rows = sum(HEAD + layout(t, w)[1] for _, t, w in secs)
    sheet = Image.new("RGB", (COLS * TW, rows), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    big, small = font(17), font(15)
    y = 0
    for title, tiles, wide in secs:
        d.text((8, y + 10), title, fill=(235, 235, 235), font=big)
        y += HEAD
        boxes, height = layout(tiles, wide)
        for (stem, label), (x, yy, w, h) in zip(tiles, boxes):
            im = Image.open(DOCS / f"{stem}.jpg").convert("RGB")
            im.thumbnail((w - 4, h - 4))
            sheet.paste(im, (x + 2 + (w - 4 - im.width) // 2, y + yy + 2 + (h - 4 - im.height) // 2))
            d.text((x + 6, y + yy + h + 3), label, fill=(210, 210, 210), font=small)
        y += height
    sheet.save(DOCS / "sheet.jpg", quality=85)
    print(f"wrote {DOCS / 'sheet.jpg'} {sheet.size}")


if __name__ == "__main__":
    main()
