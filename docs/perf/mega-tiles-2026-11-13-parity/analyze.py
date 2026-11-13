"""Tabulates every arm pair from /tmp/hw-parity/runs: differing / over_2 / max per camera, as markdown,
and writes all rows to /tmp/hw-parity/pairs.json. Also prints the instrumented runs' hold lines."""
import itertools
import json
import pathlib
import re

import numpy as np
from PIL import Image

R = pathlib.Path("/tmp/hw-parity/runs")
CAMS = ["overview", "play", "play-road-farm", "close", "low-road-cars", "low-level1", "low-coast", "low-farm"]


def ok(label: str) -> bool:
    d = R / label
    return d.is_dir() and "EXIT=0" in (d / "run.log").read_text() and len(list(d.glob("flat-*.png"))) == 8


def diff(a: str, b: str) -> dict:
    out = {}
    for cam in CAMS:
        pa, pb = R / a / f"flat-{cam}.png", R / b / f"flat-{cam}.png"
        ia = np.asarray(Image.open(pa).convert("RGB"), dtype=np.int16)
        ib = np.asarray(Image.open(pb).convert("RGB"), dtype=np.int16)
        d = np.abs(ia - ib).max(axis=2)
        out[cam] = {"differing": int((d > 0).sum()), "over_2": int((d > 2).sum()), "max": int(d.max()),
                    "bytes_identical": pa.read_bytes() == pb.read_bytes()}
    return out


def labels(prefix: str) -> list[str]:
    return sorted(p.name for p in R.iterdir() if re.fullmatch(prefix + r"-\d+", p.name) and ok(p.name))


groups = {"main": labels("main"), "fix": labels("fix"), "branch": labels("branch"),
          "fixi": labels("fixi"), "branchi": labels("branchi"),
          "fix-lod0": labels("fix-lod0"), "branch-lod0": labels("branch-lod0")}
print("ok runs:", json.dumps(groups))
pairs = []
for g in groups:
    pairs += [(g + " vs " + g, a, b) for a, b in itertools.combinations(groups[g], 2)]
for x, y in [("fix", "branch"), ("fixi", "branchi"), ("fix-lod0", "branch-lod0"), ("fix", "main")]:
    pairs += [(x + " vs " + y, a, b) for a, b in itertools.product(groups[x], groups[y])]
rows = []
for arm, a, b in pairs:
    rows.append({"arm": arm, "a": a, "b": b, "cams": diff(a, b)})
pathlib.Path("/tmp/hw-parity/pairs.json").write_text(json.dumps(rows, indent=1))
print("| arm | pair | " + " | ".join(CAMS) + " |")
print("|---|---|" + "---|" * len(CAMS))
for r in rows:
    cells = [f"{c['differing']:,} / {c['over_2']:,} / {c['max']}" + (" (=)" if c["bytes_identical"] else "")
             for c in (r["cams"][k] for k in CAMS)]
    print(f"| {r['arm']} | {r['a']}:{r['b']} | " + " | ".join(cells) + " |")
for lab in groups["fixi"] + groups["branchi"]:
    lines = [ln for ln in (R / lab / "run.log").read_text().splitlines() if "ab: hold" in ln or "ab: readback" in ln
             or ("ab: command" in ln and ("Screenshot" in ln or "Camera" in ln or "DumpState" in ln or "Photo" in ln))]
    print(f"\n## {lab}")
    for ln in lines:
        print(re.sub(r"^.*?ab: ", "", ln)[:160])
for lab in sorted(p.name for p in R.iterdir()):
    print(lab, (R / lab / "load.txt").read_text().replace("\n", " ") if (R / lab / "load.txt").exists() else "")
