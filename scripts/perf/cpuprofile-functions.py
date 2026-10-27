#!/usr/bin/env python3
"""Reduces V8 .cpuprofile files (e2e/perf-close-camera.mjs PROFILE_DIR) to ms per engine frame per function.

usage: cpuprofile-functions.py A.cpuprofile[,A2...] [B.cpuprofile[,B2...]] [--top N] [--match REGEX] [--out out.json]

Each side is a comma list of profiles, averaged. For every function name it reports self time and
inclusive time (a function counted once per sample however deep it recurs), in ms per engine frame
(the profile's `frames`, the window's engine callbacks). With two sides it prints B - A, sorted by
the absolute inclusive delta. Names come from the wasm name section; a build without one shows
`wasm-function[N]`.
"""
import argparse
import json
import re
from collections import defaultdict


HASH = re.compile(r"\[[0-9a-f]{16}\]")
SYSTEM = re.compile(r", ([\w:]+)> as [\w:]+::System>::(\w+)$")


def short(name: str) -> str:
    """Drops crate hashes, and names a Bevy function system by its function and method."""
    name = HASH.sub("", name)
    m = SYSTEM.search(name)
    if name.startswith("<bevy_ecs::system::function_system::FunctionSystem<") and m:
        return f"system {m.group(1)}::{m.group(2)}"
    return name


def load(path: str) -> tuple[dict[str, float], dict[str, float], float]:
    p = json.load(open(path))
    nodes = {n["id"]: n for n in p["nodes"]}
    parent: dict[int, int] = {}
    for n in p["nodes"]:
        for c in n.get("children", []):
            parent[c] = n["id"]
    name = {i: short(n["callFrame"]["functionName"] or "(anonymous)") for i, n in nodes.items()}
    stacks: dict[int, list[str]] = {}

    def stack(i: int) -> list[str]:
        if i not in stacks:
            s = []
            j = i
            while j is not None:
                s.append(name[j])
                j = parent.get(j)
            stacks[i] = s
        return stacks[i]

    self_us: dict[str, float] = defaultdict(float)
    incl_us: dict[str, float] = defaultdict(float)
    deltas = p["timeDeltas"]
    samples = p["samples"]
    for k, sid in enumerate(samples):
        dt = deltas[k + 1] if k + 1 < len(deltas) else 0
        s = stack(sid)
        self_us[s[0]] += dt
        for f in set(s):
            incl_us[f] += dt
    frames = p.get("frames") or 1
    return ({k: v / 1000 / frames for k, v in self_us.items()},
            {k: v / 1000 / frames for k, v in incl_us.items()}, frames)


def side(arg: str) -> tuple[dict[str, float], dict[str, float]]:
    paths = arg.split(",")
    s_all: dict[str, float] = defaultdict(float)
    i_all: dict[str, float] = defaultdict(float)
    for path in paths:
        s, i, _ = load(path)
        for k, v in s.items():
            s_all[k] += v / len(paths)
        for k, v in i.items():
            i_all[k] += v / len(paths)
    return s_all, i_all


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("a")
    ap.add_argument("b", nargs="?")
    ap.add_argument("--top", type=int, default=40)
    ap.add_argument("--match", default="")
    ap.add_argument("--out")
    args = ap.parse_args()
    rx = re.compile(args.match) if args.match else None
    sa, ia = side(args.a)
    rows = []
    if args.b:
        sb, ib = side(args.b)
        for k in set(ia) | set(ib):
            if rx and not rx.search(k):
                continue
            rows.append({"function": k, "incl_a": ia.get(k, 0), "incl_b": ib.get(k, 0), "incl_delta": ib.get(k, 0) - ia.get(k, 0),
                         "self_a": sa.get(k, 0), "self_b": sb.get(k, 0), "self_delta": sb.get(k, 0) - sa.get(k, 0)})
        rows.sort(key=lambda r: -abs(r["incl_delta"]))
        print(f"{'incl A':>8} {'incl B':>8} {'d incl':>8} {'d self':>8}  function (ms per frame)")
        for r in rows[: args.top]:
            print(f"{r['incl_a']:8.3f} {r['incl_b']:8.3f} {r['incl_delta']:+8.3f} {r['self_delta']:+8.3f}  {r['function'][:150]}")
    else:
        for k in ia:
            if rx and not rx.search(k):
                continue
            rows.append({"function": k, "incl": ia[k], "self": sa.get(k, 0)})
        rows.sort(key=lambda r: -r["incl"])
        print(f"{'incl':>8} {'self':>8}  function (ms per frame)")
        for r in rows[: args.top]:
            print(f"{r['incl']:8.3f} {r['self']:8.3f}  {r['function'][:150]}")
    if args.out:
        json.dump(rows, open(args.out, "w"), indent=1)


if __name__ == "__main__":
    main()
