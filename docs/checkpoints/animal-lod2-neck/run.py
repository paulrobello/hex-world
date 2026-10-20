"""In-engine forced-LOD shots of the farm animals for the LOD2 neck fix (handoff known item: dark neck
patches on the grazing animals at LOD2). The kit scenario's farm-variants farm at (0, 5) holds every
animal piece on main (animal_cow, animal_cow_b, animal_sheep, animal_sheep_b, animal_sheep_c); shots.json
shoots it with the debug Force LOD at 2, 1 and 0 from the same cameras, natively at 1600x1000 with
tilt-shift off. Native runs load cwd/assets, so the same binary shoots the committed maps before the
fix and the rebuilt maps after it.

Usage (from the repo root, after cargo build -p hex-app --release):
  python3 docs/checkpoints/animal-lod2-neck/run.py <label>   # label: before | after
Writes target/lod2neck/<label>/*.png. Shots are not timing measurements: they run under caffeinate with
no load wait; every PNG's mean is checked and a run with a black frame is retried once. Needs Pillow."""
import json
import pathlib
import shutil
import subprocess
import sys

from PIL import Image, ImageStat

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/animal-lod2-neck"
SHOTS = ROOT / "target/shots"
label = sys.argv[1]
out = ROOT / "target/lod2neck" / label
out.mkdir(parents=True, exist_ok=True)
script = json.loads((DOCS / "shots.json").read_text())
names = [c["name"] for c in script if c["cmd"] == "screenshot"]


def run() -> list:
    for n in names:
        (SHOTS / f"{n}.png").unlink(missing_ok=True)
    with open(out / "run.log", "w") as log:
        subprocess.run(["caffeinate", "-u", "-d", "-t", "600", str(ROOT / "target/release/hex-world"),
                        "--script", str(DOCS / "shots.json"), "--size", "1600x1000", "--seed", "1",
                        "--fps", "off", "--tilt", "off"], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                       check=True)
    dark = []
    for n in names:
        p = SHOTS / f"{n}.png"
        mean = sum(ImageStat.Stat(Image.open(p).convert("L")).mean) / 255.0 if p.exists() else 0.0
        print(f"{n}: mean {mean:.3f}")
        if mean < 0.02:
            dark.append(n)
    return dark


dark = run()
if dark:
    print("black frames", dark, "- retrying once")
    dark = run()
    if dark:
        sys.exit(f"black frames after retry: {dark}")
for n in names:
    shutil.copy(SHOTS / f"{n}.png", out / f"{n}.png")
for st in ("lod2neck-state-lod2", "lod2neck-state-lod0"):
    js = json.loads((SHOTS / f"{st}.json").read_text())
    print(st, "debug.lod", json.dumps(js.get("debug", {}).get("lod")))
print("wrote", out)
