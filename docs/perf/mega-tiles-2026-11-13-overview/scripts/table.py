#!/usr/bin/env python3
"""Parity table over the proof runs: per camera, ranges of differing pixels / over 2/255 / max delta."""
import itertools
import numpy as np
from PIL import Image

R = "/tmp/ovb/runs/"
SHOTS = ["overview", "play", "play-road-farm", "close", "low-road-cars", "low-level1",
         "low-coast", "low-farm", "overview-late", "overview-later"]
arms = {a: [f"p-{a}-{i}" for i in (1, 2, 3)] for a in ("mf", "bfix", "head")}
cache = {}


def img(run, s):
    k = (run, s)
    if k not in cache:
        cache[k] = np.asarray(Image.open(f"{R}{run}/flat-{s}.png").convert("RGB")).astype(np.int16)
    return cache[k]


def stats(a, b, s):
    d = np.abs(img(a, s) - img(b, s)).max(axis=2)
    return (int((d > 0).sum()), int((d > 2).sum()), int(d.max()))


def rng(vals):
    out = []
    for k in range(3):
        lo, hi = min(v[k] for v in vals), max(v[k] for v in vals)
        out.append(f"{lo:,}" if lo == hi else f"{lo:,}-{hi:,}")
    return " / ".join(out)


groups = {
    "main+fix vs main+fix (3)": list(itertools.combinations(arms["mf"], 2)),
    "branch+fix vs branch+fix (3)": list(itertools.combinations(arms["bfix"], 2)),
    "**main+fix vs branch+fix (9)**": list(itertools.product(arms["mf"], arms["bfix"])),
    "main+fix vs branch unfixed (9)": list(itertools.product(arms["mf"], arms["head"])),
}
print("| Camera | " + " | ".join(groups) + " |")
print("|---|" + "---|" * len(groups))
for s in SHOTS:
    row = [rng([stats(a, b, s) for a, b in pairs]) for pairs in groups.values()]
    print(f"| {s} | " + " | ".join(row) + " |")
