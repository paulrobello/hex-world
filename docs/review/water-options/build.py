"""Builds the water-options review images from shots.sh's captures (target/wo/opt-<n>/).

  python3 docs/review/water-options/build.py

Writes into this folder: per option a drying-trail strip (five frames of the beach clip across
one dry-back) and four churn stills (fork top and oblique, bend top and oblique) as JPEG, one
comparison MP4 per family (every option in a labelled grid, played in step), and metrics.json:
the mean luminance of a fixed waterline crop over the strip frames and of each churn still, which
must move monotonically with the knob."""
import json
import pathlib
import shutil
import subprocess

from PIL import Image, ImageDraw, ImageFont, ImageStat

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SRC = ROOT / "target/wo"
TRAIL = [0.95, 0.9, 0.8, 0.7, 0.6]
CHURN = [0.25, 0.5, 1.0, 2.0, 3.5]
CURRENT = 3
# The beach clip's retreat: the wash runs back down the sand from frame 80 to about 120 (1/30 s
# per frame), so these five frames span 1.07 s of dry-back.
STRIP = [84, 92, 100, 108, 116]
STILLS = ["wo-fork-top", "wo-fork-oblique", "wo-bend-top", "wo-bend-oblique", "wo-bend60-top", "wo-bend60-oblique"]
# The waterline the retreating wash leaves (1280x800 frame): the strip crop and the luminance
# metric. The churn metric is each still's whole-frame mean luminance: everything but the foam is
# the same in every option.
SAND_BOX = (160, 200, 1120, 680)
FONT = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 26)


def frame(n, name):
    return Image.open(SRC / f"opt-{n}" / f"{name}.png").convert("RGB")


def jpeg(im, name, size=(960, 600)):
    im.resize(size, Image.LANCZOS).save(HERE / f"{name}.jpg", quality=88)


def luma(im, box):
    return ImageStat.Stat(im.crop(box).convert("L")).mean[0]


def grid(frames_by_opt, labels, out_png):
    w, h = 640, 400
    sheet = Image.new("RGB", (w * 3, h * 2), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    for k, im in enumerate(frames_by_opt):
        x, y = (k % 3) * w, (k // 3) * h
        sheet.paste(im.resize((w, h), Image.LANCZOS), (x, y))
        d.rectangle((x, y, x + 260, y + 40), fill=(0, 0, 0))
        d.text((x + 10, y + 6), labels[k], fill=(255, 255, 255), font=FONT)
    sheet.save(out_png)


def mp4(name, clip, frames, labels):
    tmp = SRC / f"grid-{name}"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    for i in range(frames):
        grid([frame(n, f"{clip}-{i:03d}") for n in range(1, 6)], labels, tmp / f"{i:03d}.png")
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-framerate", "30", "-i", str(tmp / "%03d.png"),
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "23", str(HERE / f"{name}.mp4")],
        check=True,
    )


def label(code, value, n):
    return f"{code} {value}" + (" (current)" if n == CURRENT else "")


metrics = {"trail": [], "churn": []}
for n in range(1, 6):
    for k, f in enumerate(STRIP):
        jpeg(frame(n, f"wo-beach-{f:03d}").crop(SAND_BOX), f"trail-o{n}-{k + 1}", (640, 320))
    jpeg(frame(n, f"wo-beach-{STRIP[0]:03d}"), f"trail-o{n}-full")
    metrics["trail"].append({
        "option": f"O{n}", "trail": TRAIL[n - 1],
        "sand_luma": [round(luma(frame(n, f"wo-beach-{f:03d}"), SAND_BOX), 2) for f in STRIP],
    })
    for s in STILLS:
        jpeg(frame(n, s), f"churn-o{n}-{s.removeprefix('wo-')}")
    metrics["churn"].append({
        "option": f"O{n}", "churn": CHURN[n - 1],
        **{s.removeprefix("wo-") + "_luma": round(luma(frame(n, s), (0, 0, 1280, 800)), 3) for s in STILLS},
    })
mp4("trail-compare", "wo-beach", 150, [label(f"O{n}", f"x{TRAIL[n - 1]}", n) for n in range(1, 6)])
mp4("churn-compare", "wo-fork-motion", 90, [label(f"O{n}", f"x{CHURN[n - 1]}", n) for n in range(1, 6)])
(HERE / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")
print(json.dumps(metrics, indent=2))
