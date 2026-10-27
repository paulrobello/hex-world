"""Writes the B3 water-props review shot scripts from the scatter checkpoint's mixed world
(docs/checkpoints/scatter/scatter-mixed.json: the same 32 tiles at the same rotations, seed 1), so
the river_spring at (-4, 3) and the river_bends at (-3, 2) and (0, 3) carry the seeded water props.
Usage: python3 docs/checkpoints/water-props/make_scripts.py
Then: make shots-piece SCRIPT=docs/checkpoints/water-props/<name>.json OUT=docs/checkpoints/water-props
      ARGS="--scenario play --clock fixed --water_clock fixed\""""
import json
import math
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
MIXED = HERE.parent / "scatter" / "scatter-mixed.json"


def centre(q, r):
    return [16.0 * math.sqrt(3.0) * (q + r / 2.0), 16.0 * 1.5 * r]


def world():
    cmds = json.loads(MIXED.read_text())
    out = []
    for c in cmds:
        if c.get("cmd") == "toggle" and c.get("what") == "photo":
            break
        out.append(c)
    out += [{"cmd": "hover", "q": 9, "r": -9}, {"cmd": "wait", "frames": 700}]
    return out


def cam(name, pos, look):
    return [{"cmd": "camera", "pos": pos, "look_at": look}, {"cmd": "wait", "frames": 45},
            {"cmd": "photo_export", "scale": 1, "name": name}, {"cmd": "wait", "frames": 20}]


def locate():
    s = world() + [{"cmd": "debug", "scatter": True}]
    for name, (q, r) in {"wp-top-bend-a": (0, 3), "wp-top-spring": (-4, 3), "wp-top-bend-b": (-3, 2), "wp-top-spring-b": (0, 2)}.items():
        x, z = centre(q, r)
        s += cam(name, [x, 42.0, z + 0.01], [x, 0.0, z])
    s += [{"cmd": "debug", "scatter": False}]
    for name, (q, r) in {"wp-play-bend-a": (0, 3), "wp-play-spring": (-4, 3)}.items():
        x, z = centre(q, r)
        s += cam(name, [x + 38.0, 52.0, z + 38.0], [x, -1.0, z])
    return s + [{"cmd": "wait", "frames": 30}, {"cmd": "exit"}]


def review():
    """Play distance, sun-side close-ups, eye level (walk, 1.7 m) and the spring's built-in pads
    beside a lily cluster. Instance spots read off locate.json's top-down frames (34.8 mm a pixel):
    on the bend at (0, 3) lily_cluster_b at (38.9, 78.4) and the reed bed at (42.0, 72.2); on the
    spring at (-4, 3) lily_cluster_c at (-67.9, 70.3) beside the centre's pads round the node."""
    s = world()
    for name, (q, r) in {"wp-play-bend-a": (0, 3), "wp-play-spring": (-4, 3)}.items():
        x, z = centre(q, r)
        s += cam(name, [x + 38.0, 52.0, z + 38.0], [x, -1.0, z])
    s += cam("wp-close-bend-a", [47.4, 3.6, 82.3], [40.4, -0.9, 75.3])
    s += cam("wp-close-spring", [-63.4, 3.2, 75.3], [-68.3, -1.39, 70.8])
    s += cam("wp-pads-side-by-side", [-67.6, 7.5, 77.0], [-68.6, -1.39, 70.6])
    s += [{"cmd": "walk", "on": True, "pointer_lock": None}, {"cmd": "wait", "frames": 30},
          {"cmd": "walk_to", "x": 47.0, "z": 77.2, "heading": 315, "pitch": -4}, {"cmd": "wait", "frames": 90},
          {"cmd": "photo_export", "scale": 1, "name": "wp-eye-bend-a"}, {"cmd": "wait", "frames": 20},
          {"cmd": "dump_state", "name": "wp-review-state"}]
    return s + [{"cmd": "wait", "frames": 30}, {"cmd": "exit"}]


if __name__ == "__main__":
    (HERE / "locate.json").write_text(json.dumps(locate(), indent=1) + "\n")
    (HERE / "review.json").write_text(json.dumps(review(), indent=1) + "\n")
    print("wrote", HERE / "locate.json", HERE / "review.json")
