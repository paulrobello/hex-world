"""river_fork review sheet: docs/checkpoints/river_fork/sheet.jpg.

  python3 docs/checkpoints/river_fork/run.py [--sheet-only]     (from the repo root; needs Pillow)

Runs target/release/hex-world on play.json (the fork at (1, 0) rot 0 beside the start meadow, river_straight on arms 0
and 4, river_bend on arm 2, a second straight downstream of arm 0, the oak tile at (1, 1)), shooting
`photo_export`s, which render offscreen (native window captures came back black at host load 300+
on 2026-10-20). Each PNG's mean is checked and a run with a black frame is retried once. Not a timing
measurement, so it does not wait on host load. The sheet puts the approved river references, the
six-view renders of river_fork_base and river_fork_center (`make kit-renders`, untextured), and the
play shots together: play distance, top-down, eye level, a close-up of each arm's seam with its
neighbour, the centre two frames 45 frames (1.5 s of water time) apart with their difference, a
sun-side and a shade-side bank at 4 m, and a scale shot beside the hero oak."""
import pathlib
import shutil
import subprocess
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFont, ImageStat

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/river_fork"
SHOTS = ROOT / "target/shots"
NAMES = ("rf-play", "rf-top", "rf-seam-e0-straight", "rf-seam-e2-bend", "rf-seam-e4-straight", "rf-flow-a",
         "rf-flow-b", "rf-sun", "rf-shade", "rf-scale-oak", "rf-eye")
VIEWS = ("front", "back", "left", "right", "top", "three_quarter")


def mean(p):
    return sum(ImageStat.Stat(Image.open(p).convert("L")).mean) / 255.0


def shoot():
    for attempt in range(2):
        for n in NAMES:
            (SHOTS / f"{n}.png").unlink(missing_ok=True)
        with open(ROOT / "target/river-fork-play.log", "w") as f:
            subprocess.run(["caffeinate", "-u", "-d", "-t", "900", ROOT / "target/release/hex-world", "--scenario",
                            "play", "--script", DOCS / "play.json", "--size", "1280x800", "--seed", "1", "--fps",
                            "off", "--tilt", "off", "--clock", "fixed", "--water_clock", "fixed"],
                           cwd=ROOT, stdout=f, stderr=subprocess.STDOUT, check=False)
        got = [SHOTS / f"{n}.png" for n in NAMES]
        if all(p.exists() and mean(p) > 0.03 for p in got):
            break
        print("black or missing frame, retrying once")
    else:
        sys.exit("still black or missing after a retry (target/river-fork-play.log)")
    for n in NAMES:
        Image.open(SHOTS / f"{n}.png").convert("RGB").save(DOCS / f"{n}.jpg", quality=85)
    (ROOT / "target/shots/rf-state.json").exists() and shutil.copyfile(ROOT / "target/shots/rf-state.json",
                                                                       DOCS / "rf-state.json")


def tile(path, w, h):
    im = Image.open(path).convert("RGB")
    im.thumbnail((w, h), Image.LANCZOS)
    out = Image.new("RGB", (w, h), (24, 24, 24))
    out.paste(im, ((w - im.width) // 2, (h - im.height) // 2))
    return out


def flow_diff():
    a = Image.open(SHOTS / "rf-flow-a.png").convert("RGB")
    b = Image.open(SHOTS / "rf-flow-b.png").convert("RGB")
    d = ImageChops.difference(a, b).convert("L").point(lambda v: min(255, v * 6))
    print(f"flow a/b mean |diff| {sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3:.2f} / 255")
    return d.convert("RGB")


def sheet():
    try:
        f = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 18)
        fb = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 26)
    except OSError:
        f = fb = ImageFont.load_default()
    pad, label = 12, 26
    W = pad + 3 * (600 + pad)
    rows = [
        [("refs", ROOT / "assets-source/refs/river/ref-1.jpg", "approved river ref-1 (owner 2026-10-13)"),
         ("refs", ROOT / "assets-source/refs/river/ref-2.jpg", "approved river ref-2"),
         ("shot", DOCS / "rf-play.jpg", "play distance: fork with straights on arms 0 and 4, bend on arm 2")],
        [("shot", DOCS / "rf-top.jpg", "top-down (edge 0 to the right, arm 0 the outlet)"),
         ("shot", DOCS / "rf-eye.jpg", "eye level (walk mode), meadow in the arm 0 / arm 2 corner"),
         ("shot", DOCS / "rf-seam-e0-straight.jpg", "seam, arm 0 | river_straight, close")],
        [("shot", DOCS / "rf-seam-e2-bend.jpg", "seam, arm 2 | river_bend, close"),
         ("shot", DOCS / "rf-seam-e4-straight.jpg", "seam, arm 4 | river_straight, close"),
         ("shot", DOCS / "rf-flow-a.jpg", "centre, frame A (confluence: arms 2 and 4 in, 0 out)")],
        [("shot", DOCS / "rf-flow-b.jpg", "centre, frame B, 45 frames (1.5 s) later"),
         ("diff", None, "|A - B| x6: the water moves, the ground does not"),
         ("shot", DOCS / "rf-sun.jpg", "sun side, 4 m: arm 0's south bank (facing the sun)")],
        [("shot", DOCS / "rf-shade.jpg", "shade side, 4 m: arm 0's north bank"),
         ("shot", DOCS / "rf-scale-oak.jpg", "scale: the oak tile's hero oak beside arms 0 and 2"),
         ("blank", None, "")],
    ]
    six_h = 220
    H = (90 + len(rows) * (label + 375 + pad) + 2 * (label + six_h + pad) + pad)
    s = Image.new("RGB", (W, H), (24, 24, 24))
    d = ImageDraw.Draw(s)
    d.text((pad, 14), "river_fork: river_fork_base 3432/1260/354 tris, river_fork_center 2368/400/120 (pending owner "
           "review)", fill=(235, 235, 235), font=fb)
    d.text((pad, 50), "Y bed = river_straight profile at the distance to the nearest arm; seams = river_straight_base "
           "ring and joint row on every River edge", fill=(200, 200, 200), font=f)
    y = 90
    for row in rows:
        for c, (kind, path, text) in enumerate(row):
            x = pad + c * (600 + pad)
            if kind == "blank":
                continue
            d.text((x, y + 3), text, fill=(220, 220, 220), font=f)
            im = flow_diff() if kind == "diff" else Image.open(path).convert("RGB")
            s.paste(tile_from(im, 600, 375), (x, y + label))
        y += label + 375 + pad
    for pid in ("river_fork_base", "river_fork_center"):
        d.text((pad, y + 3), f"{pid}: six-view render (make kit-renders, untextured)", fill=(220, 220, 220), font=f)
        w6 = (W - pad) // 6 - pad
        for i, v in enumerate(VIEWS):
            p = ROOT / f"assets-source/kit/{pid}/renders/{v}.jpg"
            s.paste(tile(p, w6, six_h), (pad + i * (w6 + pad), y + label))
        y += label + six_h + pad
    s.save(DOCS / "sheet.jpg", quality=86)
    print(f"wrote {DOCS / 'sheet.jpg'}")


def tile_from(im, w, h):
    im = im.copy()
    im.thumbnail((w, h), Image.LANCZOS)
    out = Image.new("RGB", (w, h), (24, 24, 24))
    out.paste(im, ((w - im.width) // 2, (h - im.height) // 2))
    return out


if __name__ == "__main__":
    if "--sheet-only" not in sys.argv:
        shoot()
    sheet()
