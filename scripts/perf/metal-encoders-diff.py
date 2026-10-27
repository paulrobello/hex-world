#!/usr/bin/env python3
"""Per-encoder GPU time of two metal-encoders.py tables side by side (docs/perf/large-world-o4o5/).

usage: metal-encoders-diff.py <a-encoders.json> <b-encoders.json> [--out diff.json]
Prints the frame interval, GPU-busy time and encoder sum of each, then every (label, channel) row
with a's and b's ms per frame and b minus a, largest first.
"""

import json
import sys


def rows(t: dict) -> dict[tuple[str, str], float]:
    """(label, channel) -> ms per frame."""
    return {(r["label"].removeprefix("Command Buffer 0:"), r["channel"]): r["ms_per_frame"] for r in t["by_label_channel"]}


def main() -> None:
    args = sys.argv[1:]
    out = None
    if "--out" in args:
        i = args.index("--out")
        out = args[i + 1]
        del args[i : i + 2]
    a, b = (json.load(open(p)) for p in args)
    ra, rb = rows(a), rows(b)
    head = {k: [a[k], b[k], round(b[k] - a[k], 3)] for k in ("frame_interval_ms", "gpu_busy_ms_per_frame", "sum_of_encoders_ms_per_frame")}
    for k, (x, y, d) in head.items():
        print(f"{k:32s} {x:8.3f} {y:8.3f} {d:+8.3f}")
    keys = sorted(set(ra) | set(rb), key=lambda k: -abs(rb.get(k, 0.0) - ra.get(k, 0.0)))
    table = []
    for k in keys:
        x, y = ra.get(k, 0.0), rb.get(k, 0.0)
        table.append({"label": k[0], "channel": k[1], "a": x, "b": y, "delta": round(y - x, 3)})
        short = k[0].replace("Dawn_RenderPassEncoder_", "R:").replace("Dawn_ComputePassEncoder_", "C:")[:80]
        print(f"{x:8.3f} {y:8.3f} {y - x:+8.3f}  {k[1]:8s} {short}")
    if out:
        with open(out, "w") as f:
            json.dump({"a": args[0], "b": args[1], "head": head, "rows": table}, f, indent=1)
            f.write("\n")


if __name__ == "__main__":
    main()
