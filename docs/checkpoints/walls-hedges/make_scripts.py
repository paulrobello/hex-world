"""Writes the B2 shot scripts in this folder (run from the repo root: python3 docs/checkpoints/walls-hedges/make_scripts.py).

- farm-world.json / farm-world-off.json: the farm-heavy draw-census world, 37 tiles (the start
  meadow and rings 1 to 3 round it): 7 farms and 8 wheat fields among meadows, forests, oak tiles
  and a windmill, laid so most fields touch land on several edges. It frames every tile from the
  mixed scatter world's overview height and dumps the pass census (`dump_state`). The `-off` copy
  is the same world for a run under `--kit_ablate boundaries` (no field boundaries), so the two
  dumps give the draws with and without B2 on one build.
- review.json: the owner-review views of one farm and its neighbours: play distance, a close
  three-quarter of a corner, the gate, eye level (walk mode, 1.7 m) along a wall and a hedge.
"""
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent

# (q, r, tile): ring 1 to 3 round the start meadow at the origin.
FARM_WORLD = [
    (1, 0, "farm"), (0, 1, "meadow"), (-1, 1, "wheat_field"), (-1, 0, "forest"), (0, -1, "farm"),
    (1, -1, "oak"),
    (2, 0, "meadow"), (1, 1, "wheat_field"), (0, 2, "forest"), (-1, 2, "farm"), (-2, 2, "meadow"),
    (-2, 1, "oak"), (-2, 0, "wheat_field"), (-1, -1, "meadow"), (0, -2, "forest"), (1, -2, "wheat_field"),
    (2, -2, "meadow"), (2, -1, "farm"),
    (3, 0, "wheat_field"), (2, 1, "forest"), (1, 2, "meadow"), (0, 3, "farm"), (-1, 3, "oak"),
    (-2, 3, "wheat_field"), (-3, 3, "meadow"), (-3, 2, "farm"), (-3, 1, "forest"), (-3, 0, "meadow"),
    (-2, -1, "farm"), (-1, -2, "oak"), (0, -3, "meadow"), (1, -3, "windmill"), (2, -3, "wheat_field"),
    (3, -3, "forest"), (3, -2, "meadow"), (3, -1, "wheat_field"),
]


def place(tiles):
    out = []
    for q, r, tile in tiles:
        out += [{"cmd": "select_tile", "id": tile}, {"cmd": "hover", "q": q, "r": r}, {"cmd": "place", "q": q, "r": r}]
    return out


def farm_world(suffix: str) -> list:
    return ([{"cmd": "camera", "pos": [0, 172, 165], "look_at": [0, 0, -2]}, {"cmd": "wait", "frames": 30}]
            + place(FARM_WORLD)
            + [{"cmd": "hover", "q": 9, "r": -9}, {"cmd": "toggle", "what": "photo"},
               {"cmd": "photo", "dof": "off", "pause_agents": False}, {"cmd": "wait", "frames": 700},
               {"cmd": "dump_state", "name": f"wh-farm-world{suffix}"},
               {"cmd": "screenshot", "name": f"wh-farm-overview{suffix}"}, {"cmd": "wait", "frames": 5},
               {"cmd": "exit"}])


def write(name: str, cmds: list) -> None:
    (HERE / name).write_text(json.dumps(cmds, indent=1) + "\n")
    print("wrote", HERE / name)


# The review world: the farm east of the start meadow (`play::boundaries::tests::REVIEW`), whose
# boundaries at seed 1 are a hedge on its edge 1 (toward the oak tile), walls on edges 2, 3 (the
# gate, toward the start meadow) and 4, stubs only on the barn's edge 5, and none on edge 0 (the
# wheat field). World positions: the farm's centre is (27.71, 0).
REVIEW_WORLD = [(0, 1, "forest"), (1, 1, "oak"), (2, 0, "wheat_field"), (2, -1, "meadow"), (1, -1, "meadow"),
                (1, 0, "farm")]


def review() -> list:
    def cam(pos, look, name, frames=40):
        return [{"cmd": "camera", "pos": pos, "look_at": look}, {"cmd": "wait", "frames": frames},
                {"cmd": "screenshot", "name": name}]
    return ([{"cmd": "camera", "pos": [28, 70, 75], "look_at": [28, 0, 0]}, {"cmd": "wait", "frames": 30}]
            + place([(q, r, t) for q, r, t in REVIEW_WORLD])
            + [{"cmd": "hover", "q": 9, "r": -9}, {"cmd": "toggle", "what": "photo"},
               {"cmd": "photo", "dof": "off", "pause_agents": True}, {"cmd": "wait", "frames": 400},
               {"cmd": "dump_state", "name": "wh-review-state"}]
            + cam([28, 70, 75], [28, 0, 0], "wh-play")
            + cam([28, 150, 140], [28, 0, 0], "wh-overview")
            + cam([20.5, 3.2, -3.5], [13.86, 0.7, 0.0], "wh-gate")
            + cam([20.0, 4.5, -2.0], [13.86, 0.6, -8.0], "wh-bend")
            + cam([35.5, 4.0, 1.0], [41.57, 0.9, 8.0], "wh-mixed-corner")
            + cam([31.0, 2.8, 6.0], [34.64, 1.0, 12.0], "wh-hedge-shade")
            + cam([40.0, 2.8, 20.0], [34.64, 1.0, 12.0], "wh-hedge-sun")
            + cam([6.0, 3.0, -6.0], [13.86, 0.8, 0.0], "wh-gate-outside")
            + [{"cmd": "toggle", "what": "photo"}, {"cmd": "walk", "on": True, "pointer_lock": None},
               {"cmd": "walk_to", "x": 36.14, "z": 14.6, "heading": 330, "pitch": 0}, {"cmd": "wait", "frames": 40},
               {"cmd": "screenshot", "name": "wh-eye-hedge"},
               {"cmd": "walk_to", "x": 22.28, "z": -9.4, "heading": 330, "pitch": -4}, {"cmd": "wait", "frames": 40},
               {"cmd": "screenshot", "name": "wh-eye-wall"},
               {"cmd": "walk_to", "x": 17.36, "z": 0.0, "heading": 270, "pitch": -3}, {"cmd": "wait", "frames": 40},
               {"cmd": "screenshot", "name": "wh-eye-gate"},
               {"cmd": "walk_step", "forward": 1, "right": 0, "seconds": 3, "sprint": False}, {"cmd": "wait", "frames": 200},
               {"cmd": "dump_state", "name": "wh-walk-gate-state"},
               {"cmd": "walk", "on": False, "pointer_lock": None}, {"cmd": "wait", "frames": 5}, {"cmd": "exit"}])


MIXED = HERE.parent / "scatter" / "scatter-mixed.json"
# Two farms on the free cells between the mixed world's meadows, forest and river (the cells beside its
# roads and coast refuse field tiles: their lanes and shores face them).
MIXED_FIELDS = [(0, 1, "farm"),
                (-1, 2, "farm")]


def mixed_farms(suffix: str) -> list:
    """The mixed scatter world (scatter-mixed.json, every scatter piece) with field tiles added, so the
    overview census holds B1, B3 and B2 together."""
    cmds = json.loads(MIXED.read_text())
    at = next(i for i, c in enumerate(cmds) if c["cmd"] == "hover" and (c["q"], c["r"]) == (9, -9))
    out = cmds[:at] + place(MIXED_FIELDS) + cmds[at:]
    names = {"scatter-mixed-world": f"wh-mixed-farms{suffix}", "scatter-15-mixed-overview": f"wh-mixed-farms-overview{suffix}",
             "scatter-mixed-20tile-camera": f"wh-mixed-farms-pose{suffix}"}
    for c in out:
        if "name" in c and c["cmd"] in ("dump_state", "screenshot"):
            c["name"] = names[c["name"]]
    return out


write("mixed-farms.json", mixed_farms(""))
write("mixed-farms-off.json", mixed_farms("-off"))
write("review.json", review())
write("farm-world.json", farm_world(""))
write("farm-world-off.json", farm_world("-off"))
