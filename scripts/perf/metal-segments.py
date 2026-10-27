#!/usr/bin/env python3
"""GPU time per `gpu_timing` segment from a Metal System Trace of a native `--gpu_timing on` run.

The independent check on the in-engine stamps (docs/perf/low-camera/README.md). With timing on,
each frame has 11 marker compute encoders labelled `gpu_timing_marker`; segment i runs from the
end of marker i to the end of marker i + 1, as the stamps define it. For each frame this reports,
per segment, the GPU-busy time (the union of the process's other intervals, clipped to the
segment) and the wall time between the two marker ends, then the mean over the frames in the
window. Compare `busy_ms` with `diag().gpu.passes.<segment>.mean` from the same run.

usage: metal-segments.py <metal-gpu-intervals.xml> <out.json> [--process hex-world] [--window 8]
  [--skip-end 0.5]
"""

import json
import sys
from collections import defaultdict

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from importlib import import_module

SEGMENTS = ["preprocess", "shadow", "prepass", "ssao", "opaque", "transmission", "transparent", "taa",
            "tilt_shift", "post"]
MARKS = len(SEGMENTS) + 1


def union_ms(spans: list[tuple[int, int]]) -> float:
    busy = 0
    cur: tuple[int, int] | None = None
    for s, e in sorted(spans):
        if cur is None or s > cur[1]:
            if cur is not None:
                busy += cur[1] - cur[0]
            cur = (s, e)
        else:
            cur = (cur[0], max(cur[1], e))
    if cur is not None:
        busy += cur[1] - cur[0]
    return busy / 1e6


def main() -> None:
    args = sys.argv[1:]
    src, out = args[0], args[1]
    proc = args[args.index("--process") + 1] if "--process" in args else "hex-world"
    window = float(args[args.index("--window") + 1]) if "--window" in args else 8.0
    skip_end = float(args[args.index("--skip-end") + 1]) if "--skip-end" in args else 0.5
    rows = import_module("metal-encoders").read_rows(src)
    ivs = []
    for r in rows:
        if proc not in r.get("process", ("", ""))[1]:
            continue
        raw = r.get("event-label", ("", ""))
        label = (raw[1] or raw[0]).split("  (")[0].strip()
        s, d = int(r["start"][0]), int(r["duration"][0])
        ivs.append((s, s + d, label, r.get("channel-name", ("", ""))[0]))
    if not ivs:
        raise SystemExit(f"no intervals for process {proc}")
    end = max(e for _, e, *_ in ivs) - int(skip_end * 1e9)
    lo = end - int(window * 1e9)
    win = [iv for iv in ivs if lo <= iv[0] < end]
    # A marker encoder can show as several intervals (channels); one marker is its latest end.
    marker_ends = sorted({e for _, e, label, _ in win if "gpu_timing_marker" in label})
    work = [(s, e) for s, e, label, _ in win if "gpu_timing_marker" not in label]
    # Frames: runs of MARKS marker ends. A gap longer than any in-frame segment splits frames;
    # find frame starts as the marker after the largest gaps, MARKS markers per frame.
    gaps = [marker_ends[i + 1] - marker_ends[i] for i in range(len(marker_ends) - 1)]
    n_frames = len(marker_ends) // MARKS
    if n_frames < 10:
        raise SystemExit(f"only {len(marker_ends)} marker ends in the window: is gpu_timing on?")
    # The frame boundary is the gap after each `end` marker: try each phase, keep the one whose
    # boundary gaps (every MARKS-th) are largest on average.
    best = max(range(MARKS), key=lambda k: sum(gaps[k + MARKS - 1::MARKS]) / max(len(gaps[k + MARKS - 1::MARKS]), 1))
    per = defaultdict(list)
    wall = defaultdict(list)
    spans = []
    frames = 0
    for f0 in range(best, len(marker_ends) - MARKS + 1, MARKS):
        m = marker_ends[f0:f0 + MARKS]
        for i, name in enumerate(SEGMENTS):
            a, b = m[i], m[i + 1]
            per[name].append(union_ms([(max(s, a), min(e, b)) for s, e in work if s < b and e > a]))
            wall[name].append((b - a) / 1e6)
        spans.append((m[-1] - m[0]) / 1e6)
        frames += 1
    mean = lambda v: round(sum(v) / len(v), 3) if v else 0.0
    result = {
        "source": src, "process": proc, "window_s": window, "skip_end_s": skip_end, "frames": frames,
        "marker_phase": best,
        "span_ms": mean(spans),
        "segments": {n: {"busy_ms": mean(per[n]), "wall_ms": mean(wall[n])} for n in SEGMENTS},
    }
    with open(out, "w") as fh:
        json.dump(result, fh, indent=2)
        fh.write("\n")
    print(json.dumps({"frames": frames, "span_ms": result["span_ms"]}))
    for n in SEGMENTS:
        print(f"{n:13s} busy {result['segments'][n]['busy_ms']:7.3f}  wall {result['segments'][n]['wall_ms']:7.3f}")


if __name__ == "__main__":
    main()
