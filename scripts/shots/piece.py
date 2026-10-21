"""Owner-checkpoint shot runner behind `make shots-piece` (docs/agent-operability.md, Make targets).

Runs `target/release/hex-world --script <json>` under `caffeinate -d -u` (an idle display draws
black frames) with `--size 1600x1000 --seed 1 --fps off --tilt off` and any extra flags, never
waiting on host load: shots are not timing measurements. The frames it expects are the script's
`screenshot` and `photo_export` names; each is deleted from `target/shots/` first, so a stale frame
never passes. A frame that is missing, empty, unreadable or black (mean below 0.02 of full scale; a
black 1600x1000 frame is about 26 KB with mean 0), a non-zero exit or a timeout re-runs the whole
script once. If the retry still fails it exits 1 and leaves `<out>` untouched.

On success it writes `<out>/<name>.jpg` (JPEG quality 85) for every frame, copies the script's
`dump_state` JSON files, and writes `<out>/<script stem>-shots.json`: the command, each attempt's
1/5/15-minute host load before and after, its exit code and every frame's mean.

Usage: python3 scripts/shots/piece.py --script <json> --out <dir> [--bin <path>] [--size WxH]
       [--timeout <s>] [-- <extra hex-world flags>]
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

from PIL import Image, ImageStat

BLACK_MEAN = 0.02
SHOTS = pathlib.Path("target/shots")
BASE_FLAGS = ["--seed", "1", "--fps", "off", "--tilt", "off"]


def frame_mean(path: pathlib.Path) -> float:
    """Mean of the RGB channels as a share of full scale (0 black, 1 white)."""
    with Image.open(path) as im:
        stat = ImageStat.Stat(im.convert("RGB"))
    return sum(stat.mean) / 3 / 255


def frame_problem(path: pathlib.Path) -> tuple[str | None, float | None]:
    """(problem, mean): problem is None for a usable frame."""
    if not path.exists():
        return "missing", None
    if path.stat().st_size == 0:
        return "empty", None
    try:
        mean = frame_mean(path)
    except (OSError, ValueError) as e:
        return f"unreadable ({e})", None
    if mean < BLACK_MEAN:
        return f"black (mean {mean:.4f})", mean
    return None, mean


def script_outputs(script: pathlib.Path) -> tuple[list[str], list[str]]:
    """(frames, dumps): the PNG names (`screenshot`, `photo_export`) and `dump_state` names."""
    frames: list[str] = []
    dumps: list[str] = []
    for c in json.loads(script.read_text()):
        if c.get("cmd") in ("screenshot", "photo_export"):
            frames.append(c["name"])
        elif c.get("cmd") == "dump_state":
            dumps.append(c["name"])
    return frames, dumps


def load() -> list[float]:
    return [round(x, 2) for x in os.getloadavg()]


def attempt(
    cmd: list[str], frames: list[str], dumps: list[str], shots: pathlib.Path, timeout: float, log: pathlib.Path
) -> dict:
    """Run the capture once and check every expected frame."""
    shots.mkdir(parents=True, exist_ok=True)
    for n in frames:
        (shots / f"{n}.png").unlink(missing_ok=True)
    for n in dumps:
        (shots / f"{n}.json").unlink(missing_ok=True)
    rec: dict = {"load_before": load()}
    start = time.monotonic()
    with log.open("w") as out:
        try:
            rec["exit"] = subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT, timeout=timeout).returncode
            rec["timed_out"] = False
        except subprocess.TimeoutExpired:
            rec["exit"] = None
            rec["timed_out"] = True
    rec["seconds"] = round(time.monotonic() - start, 1)
    rec["load_after"] = load()
    rec["frames"] = {}
    for n in frames:
        problem, mean = frame_problem(shots / f"{n}.png")
        rec["frames"][n] = {"mean": None if mean is None else round(mean, 4), "problem": problem}
    bad = [f"{n}: {f['problem']}" for n, f in rec["frames"].items() if f["problem"]]
    if not frames:
        bad.append("the script takes no screenshot")
    if rec["timed_out"]:
        bad.append(f"timed out after {timeout:g} s")
    elif rec["exit"] != 0:
        bad.append(f"exit {rec['exit']}")
    rec["problems"] = bad
    return rec


def shoot(
    cmd: list[str], frames: list[str], dumps: list[str], shots: pathlib.Path, timeout: float, log: pathlib.Path
) -> list[dict]:
    """Attempt the capture, and once more if the first attempt failed."""
    tries = [attempt(cmd, frames, dumps, shots, timeout, log)]
    if tries[0]["problems"]:
        print("retrying once: " + "; ".join(tries[0]["problems"]), file=sys.stderr)
        tries.append(attempt(cmd, frames, dumps, shots, timeout, log))
    return tries


def publish(frames: list[str], dumps: list[str], shots: pathlib.Path, out: pathlib.Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    for n in frames:
        with Image.open(shots / f"{n}.png") as im:
            im.convert("RGB").save(out / f"{n}.jpg", "JPEG", quality=85)
    for n in dumps:
        src = shots / f"{n}.json"
        if src.exists():
            shutil.copyfile(src, out / f"{n}.json")


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--script", required=True, type=pathlib.Path)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--bin", default="target/release/hex-world")
    ap.add_argument("--size", default="1600x1000")
    ap.add_argument("--timeout", type=float, default=900)
    ap.add_argument("extra", nargs="*", help="hex-world flags after --, e.g. -- --scenario play")
    a = ap.parse_args(argv)
    frames, dumps = script_outputs(a.script)
    app = [a.bin, "--script", str(a.script), "--size", a.size, *BASE_FLAGS, *a.extra]
    log = SHOTS / f"{a.script.stem}-shots.log"
    tries = shoot(["caffeinate", "-d", "-u", *app], frames, dumps, SHOTS, a.timeout, log)
    for t in tries:
        for n, f in t["frames"].items():
            print(f"{n}: {f['problem'] or 'mean %.3f' % f['mean']}")
        print(f"load {t['load_before'][0]} -> {t['load_after'][0]}, {t['seconds']} s, exit {t['exit']}")
    if tries[-1]["problems"]:
        print(f"shots-piece failed (log {log}): " + "; ".join(tries[-1]["problems"]), file=sys.stderr)
        return 1
    publish(frames, dumps, SHOTS, a.out)
    report = {"command": app, "env": {k: v for k, v in os.environ.items() if k.startswith("HEXW_")}, "attempts": tries}
    (a.out / f"{a.script.stem}-shots.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"wrote {len(frames)} JPEGs and {a.script.stem}-shots.json to {a.out}/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
