#!/usr/bin/env python3
"""Per-encoder GPU time from a Metal System Trace (docs/perf/low-camera/README.md).

Reads the `metal-gpu-intervals` table written by `xctrace` (see the README for the command),
keeps one process's intervals in the last `--window` seconds of the trace, and reports, per
encoder label and channel (Vertex, Fragment, Compute), the GPU time per frame: the summed interval
durations divided by the frames in the window. Apple GPUs overlap the vertex and fragment work
of neighbouring passes, so the per-encoder sums add up to more than the GPU-busy time, which is
reported separately as the union of all of the process's intervals.

usage: metal-encoders.py <metal-gpu-intervals.xml> <out.json> [--process hex-world] [--window 8] [--skip-end 0]
  [--frame-ms X]
  --process matches a substring of the process name (a web run's is the browser's GPU process,
  `--process "for Testing Helpe"`, the name as the trace truncates it); --skip-end drops the trace's
  last seconds (a page closing); --frame-ms sets the frame interval the window is divided by, where
  the trace's own frame numbers do not count engine frames (the browser's GPU process).
"""

import json
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict


def main() -> None:
    args = sys.argv[1:]
    src, out = args[0], args[1]
    proc = args[args.index("--process") + 1] if "--process" in args else "hex-world"
    window = float(args[args.index("--window") + 1]) if "--window" in args else 8.0
    skip_end = float(args[args.index("--skip-end") + 1]) if "--skip-end" in args else 0.0
    frame_ms = float(args[args.index("--frame-ms") + 1]) if "--frame-ms" in args else None

    ids: dict[str, tuple[str, str]] = {}
    cols: list[str] = []
    rows: list[dict[str, tuple[str, str]]] = []
    for _, el in ET.iterparse(src, events=("end",)):
        if el.tag == "mnemonic":
            cols.append(el.text or "")
        elif el.tag == "row":
            row: dict[str, tuple[str, str]] = {}
            for name, child in zip(cols, list(el)):
                if "ref" in child.attrib:
                    row[name] = ids.get(child.attrib["ref"], ("", ""))
                else:
                    val = (child.text or "", child.attrib.get("fmt", ""))
                    if "id" in child.attrib:
                        ids[child.attrib["id"]] = val
                    for sub in child.iter():
                        if sub is not child and "id" in sub.attrib:
                            ids[sub.attrib["id"]] = (sub.text or "", sub.attrib.get("fmt", ""))
                    row[name] = val
            rows.append(row)
            el.clear()

    mine = [r for r in rows if proc in r.get("process", ("", ""))[1]]
    if not mine:
        procs = sorted({r.get("process", ("", ""))[1] for r in rows})
        raise SystemExit(f"no intervals for process {proc}; processes: {procs}")
    ivs = []
    for r in mine:
        start = int(r["start"][0])
        dur = int(r["duration"][0])
        raw = r.get("event-label", ("", ""))
        label = (raw[1] or raw[0]).split("  (")[0].strip()
        ivs.append((start, dur, r.get("channel-name", ("", ""))[0], label, r.get("frame-number", ("", ""))[0]))
    end = max(s + d for s, d, *_ in ivs) - int(skip_end * 1e9)
    lo = end - int(window * 1e9)
    win = [iv for iv in ivs if lo <= iv[0] < end]
    frames = {f for *_, f in win if f}
    n = max(len(frames), 1)
    if frame_ms:
        n = window * 1000.0 / frame_ms

    per: dict[tuple[str, str], int] = defaultdict(int)
    for _, d, ch, label, _ in win:
        per[(label, ch)] += d
    spans = sorted((s, s + d) for s, d, *_ in win)
    busy, cur_s, cur_e = 0, None, None
    for s, e in spans:
        if cur_e is None or s > cur_e:
            if cur_e is not None:
                busy += cur_e - cur_s
            cur_s, cur_e = s, e
        else:
            cur_e = max(cur_e, e)
    if cur_e is not None:
        busy += cur_e - cur_s
    span_ns = max(e for _, e in spans) - min(s for s, _ in spans)

    table = sorted(({"label": label, "channel": ch, "ms_per_frame": round(ns / n / 1e6, 3)}
                    for (label, ch), ns in per.items()), key=lambda x: -x["ms_per_frame"])
    by_label: dict[str, float] = defaultdict(float)
    for t in table:
        by_label[t["label"]] += t["ms_per_frame"]
    result = {
        "source": src, "process": proc, "window_s": window, "skip_end_s": skip_end, "frame_ms_given": frame_ms,
        "frames": round(n, 1),
        "frame_interval_ms": round(span_ns / n / 1e6, 3),
        "gpu_busy_ms_per_frame": round(busy / n / 1e6, 3),
        "sum_of_encoders_ms_per_frame": round(sum(per.values()) / n / 1e6, 3),
        "by_label": dict(sorted(((k, round(v, 3)) for k, v in by_label.items()), key=lambda x: -x[1])),
        "by_label_channel": table,
    }
    with open(out, "w") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps({k: result[k] for k in ("frames", "frame_interval_ms", "gpu_busy_ms_per_frame",
                                             "sum_of_encoders_ms_per_frame")}))
    for k, v in list(result["by_label"].items())[:25]:
        print(f"{v:8.3f}  {k}")


if __name__ == "__main__":
    main()
