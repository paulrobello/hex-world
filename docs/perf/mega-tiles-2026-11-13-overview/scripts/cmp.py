#!/usr/bin/env python3
"""cmp.py A B [shots...] : pixel diff of /tmp/ovb/runs/A vs /tmp/ovb/runs/B per shot.
Prints differing pixels / over 2/255 / max channel delta, plus the over-2 count inside the
two overview clusters (sand 1045..1095 x 445..490, sea 1105..1150 x 582..617)."""
import sys
import numpy as np
from PIL import Image

R = "/tmp/ovb/runs/"
a, b = sys.argv[1], sys.argv[2]
shots = sys.argv[3:] or ["overview", "overview-late", "overview-later", "play"]
for s in shots:
    try:
        x = np.asarray(Image.open(f"{R}{a}/flat-{s}.png").convert("RGB")).astype(int)
        y = np.asarray(Image.open(f"{R}{b}/flat-{s}.png").convert("RGB")).astype(int)
    except FileNotFoundError as e:
        print(f"{a} vs {b} {s}: missing {e.filename}")
        continue
    d = np.abs(x - y).max(axis=2)
    sand = (d[445:490, 1045:1095] > 2).sum()
    sea = (d[582:617, 1105:1150] > 2).sum()
    print(f"{a} vs {b} {s}: {(d > 0).sum()} / {(d > 2).sum()} / {d.max()}  clusters>2 sand {sand} sea {sea}")
