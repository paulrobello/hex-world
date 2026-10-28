"""Per-arm medians and paired deltas against a reference arm for grass-web.json / grass-native.json.

usage: python3 summary.py <result.json> [<reference arm>]   (default reference: off)
A delta is paired within a round; "real" when every paired delta has the same sign and the median
is larger than their spread (the close-camera-draws rule).
"""

import json
import statistics
import sys


def med(xs: list[float]) -> float:
    return round(statistics.median(xs), 2)


def main() -> None:
    path = sys.argv[1]
    ref = sys.argv[2] if len(sys.argv) > 2 else "off"
    res = json.load(open(path))
    rows = [r for r in res["rows"] if not r.get("error")]
    failed = [r for r in res["rows"] if r.get("error")]
    loads = [x for r in rows for x in (r["load_before"], r["load_after"])]
    print(f"{path}: {len(rows)} valid rows, {len(failed)} failed, loads {min(loads):.2f}-{max(loads):.2f}, "
          f"discarded {sum(r.get('load_discarded', 0) for r in rows)}")
    for r in failed:
        print(f"  failed round {r['round']} {r['arm']}: {r['error'][:200]}")
    arms = list(dict.fromkeys(r["arm"] for r in rows))
    for arm in arms:
        mine = [r for r in rows if r["arm"] == arm]
        line = f"  {arm}: n={len(mine)}"
        for k in ("mean", "p95", "p50", "max", "dropped_over_25ms"):
            vals = [r[k] for r in mine if r.get(k) is not None]
            line += f" {k} {med(vals)} [{min(vals)}, {max(vals)}]"
        g = mine[0].get("grass") or {}
        line += f" blades {g.get('blades')} groups {g.get('groups')}"
        if "draws" in mine[0]:
            line += f" draws {sorted({r['draws'] for r in mine})}"
        print(line)
        if arm == ref:
            continue
        for k in ("mean", "p95"):
            d = []
            for r in mine:
                b = next((x for x in rows if x["arm"] == ref and x["round"] == r["round"]), None)
                if b:
                    d.append(r[k] - b[k])
            if d:
                m = statistics.median(d)
                real = len(d) > 1 and (all(x > 0 for x in d) or all(x < 0 for x in d)) and abs(m) > max(d) - min(d)
                print(f"    {k} - {ref}: {m:+.2f} [{min(d):+.2f}, {max(d):+.2f}] n={len(d)}{' REAL' if real else ''}")


if __name__ == "__main__":
    main()
