"""fence_paddock LOD2 defect fix: the same cameras at Force LOD 0 and Force LOD 2 on the farm tile, before
and after, and the review sheet docs/checkpoints/fence-lod2/sheet.jpg.

  python3 docs/checkpoints/fence-lod2/run.py [rev]     (from the repo root; rev = the before GLB, 29a51b6)

Each column clones assets/ (APFS copy-on-write) into target/fence-lod2/<col>/assets (the before column
overwrites fence_paddock.glb with the GLB at `rev`) and runs target/release/hex-world with that
directory as cwd (native assets load from cwd/assets), shooting play.json (the farm tile in play at
play distance and close up) and kit.json (the kit scenario's farm variants: eye level inside the
paddock, and the sun-side close-up). Shots are `photo_export`s, rendered offscreen: on 2026-10-20 every
window `screenshot` came back all black (16-26 KB) at 1280x800 and 1600x1000 while the export of the
same frame rendered. Each PNG's mean is checked and a run with a black frame is retried once. Not a
timing measurement, so it does not wait on host load. Needs Pillow."""
import pathlib
import shutil
import subprocess
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFont, ImageStat

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/fence-lod2"
WORK = ROOT / "target/fence-lod2"
GLB = "assets/kit/p1/fence_paddock/fence_paddock.glb"
RUNS = (("play.json", ["--scenario", "play", "--clock", "fixed", "--water_clock", "fixed"],
         ("fl2-play-lod0", "fl2-play-lod2", "fl2-close-lod0", "fl2-close-lod2")),
        ("kit.json", [], ("fl2-eye-lod0", "fl2-eye-lod2", "fl2-sun-lod0", "fl2-sun-lod2")))
ROWS = (("play", "play distance, farm in play (60 m)"), ("play-crop", "play distance, fence crop x2"),
        ("close", "play, 12 m"), ("eye", "eye level inside the paddock (kit)"),
        ("sun", "sun side, 4 m (kit)"))
CROP = (440, 360, 900, 560)


def run(cmd, cwd, log):
    with open(log, "w") as f:
        r = subprocess.run(cmd, cwd=cwd, stdout=f, stderr=subprocess.STDOUT)
    if r.returncode:
        sys.exit(f"failed ({r.returncode}): {' '.join(map(str, cmd))} (log {log})")


def mean(p):
    return sum(ImageStat.Stat(Image.open(p).convert("L")).mean) / 255.0


def shoot(col, rev):
    work = WORK / col
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    run(["cp", "-cR", ROOT / "assets", work / "assets"], ROOT, work / "clone.log")
    if col == "before":
        (work / GLB).write_bytes(subprocess.run(["git", "show", f"{rev}:{GLB}"], cwd=ROOT, check=True,
                                                capture_output=True).stdout)
    for script, flags, names in RUNS:
        shots = [work / f"target/shots/{n}.png" for n in names]
        for attempt in range(2):
            run(["caffeinate", "-u", "-d", "-t", "600", ROOT / "target/release/hex-world", "--script",
                 DOCS / script, "--size", "1280x800", "--seed", "1", "--fps", "off", "--tilt", "off", *flags],
                work, work / f"{script}.log")
            if all(p.exists() and mean(p) > 0.03 for p in shots):
                break
            print(f"{col} {script}: black or missing frame, retrying once")
        else:
            sys.exit(f"{col} {script}: still black after a retry")
    out = WORK / "shots"
    out.mkdir(exist_ok=True)
    for _, _, names in RUNS:
        for n in names:
            shutil.copyfile(work / f"target/shots/{n}.png", out / f"{col}-{n}.png")
    shutil.rmtree(work / "assets")


def cell(col, row, lod):
    name = "play" if row == "play-crop" else row
    im = Image.open(WORK / f"shots/{col}-fl2-{name}-lod{lod}.png").convert("RGB")
    if row == "play-crop":
        im = im.crop(CROP).resize(((CROP[2] - CROP[0]) * 2, (CROP[3] - CROP[1]) * 2), Image.LANCZOS)
    return im.resize((640, 400), Image.LANCZOS)


def sheet():
    cols = (("before", 0, "Force LOD 0 (LOD0 unchanged)"), ("before", 2, "Force LOD 2, before (78 tris)"),
            ("after", 2, "Force LOD 2, after (168 tris)"))
    pad, head, label = 12, 56, 28
    W = pad + len(cols) * (640 + pad)
    H = head + len(ROWS) * (label + 400 + pad) + pad
    s = Image.new("RGB", (W, H), (24, 24, 24))
    d = ImageDraw.Draw(s)
    try:
        f = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 20)
        fb = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 26)
    except OSError:
        f = fb = ImageFont.load_default()
    d.text((pad, 14), "fence_paddock LOD2: posts added (every other post per run, every corner and gate post), "
           "rails unchanged, leaf as its top bar", fill=(235, 235, 235), font=fb)
    for r, (row, text) in enumerate(ROWS):
        y = head + r * (label + 400 + pad)
        for c, (col, lod, title) in enumerate(cols):
            x = pad + c * (640 + pad)
            d.text((x, y + 4), f"{title} | {text}", fill=(220, 220, 220), font=f)
            s.paste(cell(col, row, lod), (x, y + label))
    s.save(DOCS / "sheet.jpg", quality=88)
    # LOD0 must be unchanged: the before and after Force LOD 0 frames of every camera.
    for row, _ in ROWS:
        if row == "play-crop":
            continue
        a = Image.open(WORK / f"shots/before-fl2-{row}-lod0.png").convert("RGB")
        b = Image.open(WORK / f"shots/after-fl2-{row}-lod0.png").convert("RGB")
        diff = sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3
        print(f"{row}: LOD0 before/after mean |diff| {diff:.3f} / 255")
    print(f"wrote {DOCS / 'sheet.jpg'}")


if __name__ == "__main__":
    rev = sys.argv[1] if len(sys.argv) > 1 else "29a51b6"
    run(["cargo", "build", "-p", "hex-app", "--release"], ROOT, ROOT / "target/fence-lod2-build.log")
    for col in ("before", "after"):
        shoot(col, rev)
    sheet()
