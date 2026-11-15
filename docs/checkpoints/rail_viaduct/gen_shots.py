"""Write docs/checkpoints/rail_viaduct/viaduct-shots.json (run: python3 docs/checkpoints/rail_viaduct/gen_shots.py;
shoot: make shots-piece SCRIPT=docs/checkpoints/rail_viaduct/viaduct-shots.json OUT=docs/checkpoints/rail_viaduct
ARGS='--scenario play --clock fixed --water_clock fixed', under caffeinate -d -u).

Row r = -1 carries three chains (rail at deck level N, viaduct lN at level 0, rail at level N), with an
empty cell between chains. Meadow rows r = 0 and r = -2 at level 0 hold the valley. Every placement is
next to the world when it is made (the play scenario starts with one meadow at the origin), and every
rail edge faces rail or an empty cell, so each tile takes rotation 0.
"""
import json
import math
import pathlib

OUT = pathlib.Path(__file__).resolve().parent / "viaduct-shots.json"
S3 = 16 * math.sqrt(3)


def world(q: int, r: int) -> tuple[float, float]:
    return (round(S3 * (q + r / 2), 2), 24.0 * r)


cmds: list[dict] = []


def place(tile: str, q: int, r: int, level: int | None = None) -> None:
    cmds.append({"cmd": "select_tile", "id": tile})
    c: dict = {"cmd": "place", "q": q, "r": r}
    if level is not None:
        c["level"] = level
    cmds.append(c)


def shot(name: str, pos: list[float], look: list[float]) -> None:
    cmds.append({"cmd": "camera", "pos": [round(v, 2) for v in pos], "look_at": [round(v, 2) for v in look]})
    cmds.append({"cmd": "wait", "frames": 30})
    cmds.append({"cmd": "screenshot", "name": name})


for q in range(1, 12):
    place("meadow", q, 0, 0)
chains = {1: 2, 2: 6, 3: 10}
for n, q in chains.items():
    place("rail_straight", q - 1, -1, n)
    place("rail_straight", q + 1, -1, n)
    place(f"rail_viaduct_l{n}", q, -1, 0)
for q in range(1, 13):
    place("meadow", q, -2, 0)
cmds.append({"cmd": "hover", "q": 30, "r": -30})
cmds.append({"cmd": "wait", "frames": 900})
cmds.append({"cmd": "dump_state", "name": "viaduct-world-state"})

mid = world(chains[2], -1)
shot("viaduct-overview", [mid[0] - 20, 150, 150], [mid[0], 0, -24])
for n, q in chains.items():
    cx, cz = world(q, -1)
    h = 4.0 * n
    shot(f"viaduct-l{n}-three-quarter", [cx + 26 + 4 * n, 8 + 4 * n, cz + 34 + 6 * n], [cx, h / 2 + 0.5, cz])
    shot(f"viaduct-l{n}-front", [cx, 2 + 1.5 * n, cz + 34 + 10 * n], [cx, h / 2 + 0.5, cz])
    if n == 2:
        shot("viaduct-l2-deck", [cx + 6.4, h + 4.5, cz + 6], [cx - 5.6, h + 0.4, cz])
        shot("viaduct-l2-under", [cx + 7, 1.6, cz + 16], [cx + 7, 4.5, cz - 23])
    shot(f"viaduct-l{n}-seam", [cx + 22, h + 6, cz + 14], [cx + 13.86, h + 0.4, cz])
    if n == 2:
        shot("viaduct-l2-top", [cx, 110, cz + 1], [cx, 0, cz])
cx, cz = world(chains[2], -1)
cmds.append({"cmd": "debug", "force_lod": "1"})
shot("viaduct-l2-lod1", [cx + 34, 16, cz + 46], [cx, 4.5, cz])
cmds.append({"cmd": "debug", "force_lod": "2"})
shot("viaduct-l2-lod2", [cx + 34, 16, cz + 46], [cx, 4.5, cz])
cmds.append({"cmd": "debug", "force_lod": "auto"})
cmds.append({"cmd": "wait", "frames": 10})
cmds.append({"cmd": "exit"})

OUT.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in cmds) + "\n]\n")
print(f"wrote {OUT} ({len(cmds)} commands)")
