"""Grade comparison frames and measurements (owner, 2026-10-14).

Builds three native play worlds from committed scripts (the river chain of
assets-source/kit/river_edge/shots/river-play.json, the scatter world of
docs/checkpoints/scatter/scatter.json and the coast island of
assets-source/kit/beach_base/shots/beach-closeups.json), then for each view switches every grade
live with `set_grade`, waits 30 frames and takes a screenshot. Frames land in target/shots/ as PNG
and here as JPEG (frames/); crops.json holds the measured boxes (the shade-diagnosis boxes where
the camera matches) and measurements.json/.txt the numbers, all taken from the PNGs.

Usage, from the repo root after `cargo build -p hex-app --release`:
    python3 docs/checkpoints/grades/shots.py [--measure-only] [--worlds river,world,coast]
Needs Pillow and ffmpeg.
"""

import argparse
import colorsys
import json
import os
import subprocess
import sys
import time

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
SHOTS = os.path.join(REPO, "target", "shots")
FRAMES = os.path.join(HERE, "frames")
BIN = os.path.join(REPO, "target", "release", "hex-world")
GRADES = ["aces", "tony", "rich-1", "rich-2", "rich-3"]
FLAGS = ["--scenario", "play", "--size", "1600x1000", "--seed", "1", "--fps", "off",
         "--tilt", "off", "--clock", "fixed", "--water_clock", "fixed"]
MAX_LOAD = 20.0


def load_script(rel):
    with open(os.path.join(REPO, rel)) as f:
        return json.load(f)


def build_prefix(cmds, stop):
    """Commands before index `stop`, without screenshots, state dumps or the exit."""
    return [c for c in cmds[:stop] if c["cmd"] not in ("screenshot", "dump_state", "exit")]


def first_index(cmds, pred):
    return next(i for i, c in enumerate(cmds) if pred(c))


def worlds():
    river = load_script("assets-source/kit/river_edge/shots/river-play.json")
    river_build = build_prefix(river, first_index(river, lambda c: c["cmd"] == "dump_state"))
    scatter = load_script("docs/checkpoints/scatter/scatter.json")
    scatter_build = build_prefix(
        scatter, first_index(scatter, lambda c: c == {"cmd": "wait", "frames": 360}) + 1)
    beach = load_script("assets-source/kit/beach_base/shots/beach-closeups.json")
    beach_build = build_prefix(beach, first_index(beach, lambda c: c["cmd"] == "photo") + 1)
    beach_build.append({"cmd": "wait", "frames": 240})
    return {
        "river": (river_build, {
            # The shade-diagnosis cameras (k5-river-04 and -05) and the bridge view (k5-river-06).
            "mouth": ([-40.6, 1.6, -22.4], [-33.5, -1.9, -10]),
            "bridge-arch": ([-19.5, 0.4, 34.0], [-27.7, -1.0, 48.0]),
            "bridge": ([-48, 9, 55], [-34.6, -1, 48]),
        }),
        "world": (scatter_build, {
            "overview": ([30, 92, 92], [28, 0, -18]),
            "houses": ([62, 20, -16], [55.4, 0, -46]),
            "oak": ([55.4, 30, 34], [55.4, 2, 0]),
            "meadow": ([6, 13, 44], [12, 0, 22]),
        }),
        "coast": (beach_build, {
            "coast": ([20, 45, -70], [10, 0, -10]),
            "bay": ([40, 9, -20], [27.7, -1.5, 2]),
        }),
    }


def script_for(build, views):
    out = list(build)
    for view, (pos, look) in views.items():
        out.append({"cmd": "camera", "pos": pos, "look_at": look})
        if view == "mouth":
            # The default look before any set_grade, to prove `tony` changes nothing.
            out += [{"cmd": "wait", "frames": 30}, {"cmd": "screenshot", "name": "grade-mouth-default"}]
        for g in GRADES:
            out += [{"cmd": "set_grade", "grade": g}, {"cmd": "wait", "frames": 30},
                    {"cmd": "screenshot", "name": f"grade-{view}-{g}"}]
    out += [{"cmd": "set_grade", "grade": "tony"}, {"cmd": "dump_state", "name": "grade-state"},
            {"cmd": "exit"}]
    return out


WAIT_UNTIL = [time.time() + 600]


def wait_for_load():
    """Waits (bounded: 10 minutes per session, or --wait-until) for the 1-min load to drop under MAX_LOAD; past the
    bound the run goes ahead and the load is logged with its frames, which are checked for black."""
    while True:
        load = os.getloadavg()[0]
        if load < MAX_LOAD:
            return load
        if time.time() > WAIT_UNTIL[0]:
            print(f"load {load:.1f} still above {MAX_LOAD} at the end of the wait bound; shooting anyway")
            return load
        time.sleep(15)


def frame_ok(path):
    """Black or empty frames read near zero mean and spread."""
    im = Image.open(path).convert("L").resize((160, 100))
    px = list(im.getdata())
    mean = sum(px) / len(px)
    spread = max(px) - min(px)
    return mean > 8 and spread > 40, mean


def shoot(name, build, views, log_dir):
    script = script_for(build, views)
    path = os.path.join(log_dir, f"grades-{name}.json")
    with open(path, "w") as f:
        json.dump(script, f, indent=1)
    names = ([f"grade-{v}-{g}" for v in views for g in GRADES]
             + (["grade-mouth-default"] if "mouth" in views else []))
    for attempt in (1, 2):
        for n in names:
            p = os.path.join(SHOTS, n + ".png")
            if os.path.exists(p):
                os.remove(p)
        before = wait_for_load()
        with open(os.path.join(log_dir, f"grades-{name}-{attempt}.log"), "w") as log:
            ec = subprocess.call(["caffeinate", "-d", "-u", BIN, "--script", path, *FLAGS],
                                 cwd=REPO, stdout=log, stderr=subprocess.STDOUT)
        after = os.getloadavg()[0]
        bad = []
        for n in names:
            p = os.path.join(SHOTS, n + ".png")
            ok, mean = (False, 0.0) if not os.path.exists(p) else frame_ok(p)
            if not ok:
                bad.append((n, round(mean, 1)))
        print(f"{name}: exit {ec}, load {before:.1f} -> {after:.1f}, bad frames {bad}")
        if ec == 0 and not bad:
            break
        if attempt == 2:
            sys.exit(f"{name}: frames still bad after a re-run: {bad}")
    os.makedirs(FRAMES, exist_ok=True)
    for n in names:
        subprocess.check_call(["ffmpeg", "-loglevel", "error", "-y", "-i",
                               os.path.join(SHOTS, n + ".png"), "-q:v", "2",
                               os.path.join(FRAMES, n + ".jpg")])


def lin(c):
    c /= 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def measure_box(path, box):
    im = Image.open(path).convert("RGB").crop(tuple(box))
    px = list(im.getdata())
    n = len(px)
    r = sum(p[0] for p in px) / n
    g = sum(p[1] for p in px) / n
    b = sum(p[2] for p in px) / n
    sat = sum(colorsys.rgb_to_hsv(*(c / 255 for c in p))[1] for p in px) / n
    chroma = sum(max(p) - min(p) for p in px) / n
    return {
        "srgb_luma": round(0.2126 * r + 0.7152 * g + 0.0722 * b, 2),
        "lin_Y": round(0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b), 5),
        "rgb": [round(r, 1), round(g, 1), round(b, 1)],
        "hsv_sat": round(sat, 3),
        "chroma": round(chroma, 1),
    }


def measure():
    with open(os.path.join(HERE, "crops.json")) as f:
        crops = json.load(f)
    out = {}
    lines = []
    for g in GRADES + ["default"]:
        for c in crops:
            p = os.path.join(SHOTS, f"grade-{c['view']}-{g}.png")
            if not os.path.exists(p):
                continue
            m = measure_box(p, c["box"])
            out.setdefault(g, {})[c["name"]] = m
            lines.append(f"{g:8} {c['name']:18} srgb_luma={m['srgb_luma']:7.2f}/255  "
                         f"lin_Y={m['lin_Y']:.5f}  rgb=({m['rgb'][0]},{m['rgb'][1]},{m['rgb'][2]})  "
                         f"hsv_sat={m['hsv_sat']:.3f}  chroma={m['chroma']:.1f}")
    with open(os.path.join(HERE, "measurements.json"), "w") as f:
        json.dump(out, f, indent=1)
    with open(os.path.join(HERE, "measurements.txt"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--measure-only", action="store_true")
    ap.add_argument("--worlds", default="river,world,coast")
    ap.add_argument("--log-dir", default=os.path.join(REPO, "target", "grades"))
    ap.add_argument("--wait-until", type=float, help="epoch seconds ending the load wait")
    a = ap.parse_args()
    if a.wait_until:
        WAIT_UNTIL[0] = a.wait_until
    os.makedirs(a.log_dir, exist_ok=True)
    if not a.measure_only:
        w = worlds()
        for name in a.worlds.split(","):
            build, views = w[name]
            shoot(name, build, views, a.log_dir)
    measure()


if __name__ == "__main__":
    main()
