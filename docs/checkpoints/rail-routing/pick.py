#!/usr/bin/env python3
"""Picks the kept frames of the rail-routing shots from their state dumps (used by play.sh).

Prints `source:kept` pairs: the overview, then for each station of rail_routing.py the first
`routing-NNN` dump whose train stands at that station (`waiting` "station") with it as its
`destination`. Exits non-zero when a station never had a train standing at it.

Usage: python3 docs/checkpoints/rail-routing/pick.py target/shots
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from rail_routing import STATIONS  # noqa: E402


def trains(state):
    if isinstance(state, dict):
        if "train_list" in state:
            return state["train_list"]
        for v in state.values():
            found = trains(v)
            if found is not None:
                return found
    return None


def main():
    out = Path(sys.argv[1])
    pairs = ["routing-overview:routing-overview"]
    dumps = sorted(out.glob("routing-[0-9][0-9][0-9].json"))
    for name, (q, r) in STATIONS.items():
        hit = None
        for d in dumps:
            for t in trains(json.loads(d.read_text())) or []:
                if t["waiting"] == "station" and t.get("destination") == {"q": q, "r": r}:
                    hit = d.stem
                    break
            if hit:
                break
        if hit is None:
            sys.exit(f"no frame with a train standing at the {name} station ({q}, {r})")
        pairs.append(f"{hit}:routing-{name}")
    print(" ".join(pairs))


if __name__ == "__main__":
    main()
