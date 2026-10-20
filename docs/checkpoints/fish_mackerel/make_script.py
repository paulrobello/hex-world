"""Writes docs/checkpoints/fish_mackerel/mackerel.json: the boids checkpoint's world (docs/checkpoints/boids/
boids.json up to its placed-world state dump: a four-tile river chain under a road bridge, a twelve-tile coast and
ocean region, meadows and two oaks), then the fish at play distance in the sea (the one mackerel school)
and in the river (trout alone and in small groups), each from above so the water shows them, with a
closer look and a swim pair 6 fixed frames (0.1 s) apart.

Usage: python3 docs/checkpoints/fish_mackerel/make_script.py docs/checkpoints/fish_mackerel/mackerel.json
Run: bash scripts/shots/run.sh ./target/release/hex-world --scenario play --script
docs/checkpoints/fish_mackerel/mackerel.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed
--water_clock fixed, copy target/shots/mk-0*.json here, then `node docs/checkpoints/fish_mackerel/check.mjs` and
`python3 docs/checkpoints/fish_mackerel/sheet.py`. The framed shots' cameras were read off the k = 1 overviews."""
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
out = pathlib.Path(sys.argv[1])
world = json.loads((REPO / "docs" / "checkpoints" / "boids" / "boids.json").read_text())
s = [c for c in world[: next(i for i, c in enumerate(world) if c.get("name") == "boids-01-placed")]
     if c["cmd"] != "dump_state"]
s.append({"cmd": "dump_state", "name": "mk-01-placed"})


def shot(name, pos, look, wait=2, fov=None):
    cmds = [{"cmd": "camera", "pos": pos, "look_at": look}]
    if fov is not None:
        cmds.append({"cmd": "photo", "dof": "off", "pause_agents": False, "fov": fov})
    return cmds + [{"cmd": "wait", "frames": wait}, {"cmd": "screenshot", "name": name}]


# Scouting overviews (play distance, from above), then the framed shots chosen from them.
SEA_CAM, RIVER_CAM = [80, 70, 64], [-22, 58, 84]
for k, gap in enumerate((30, 300, 300)):
    s += [{"cmd": "wait", "frames": gap}]
    s += shot(f"mk-sea-{k}", SEA_CAM, [80, -2.5, 6])
    s += shot(f"mk-sea-{k}-z", SEA_CAM, [46, -1.2, 25], fov=14)
    s += shot(f"mk-river-{k}-zb", RIVER_CAM, [-14, -1.0, 24], fov=14)
    s += shot(f"mk-river-{k}-zs", RIVER_CAM, [-27.7, -1.0, 2], fov=14)
    s += shot(f"mk-river-{k}", RIVER_CAM, [-25, -2.5, 34], fov=45)
    if k == 1:
        # Framed from the k = 1 overviews: the school's west end and the trout in the straight
        # river tile, at play distance (35-40 m, looking down about 40 degrees so the water shows
        # them), then a swim pair of each 6 fixed frames (0.1 s) apart through a long lens.
        s += shot("mk-sea-play", [58, 24, 52], [49, -1.2, 25])
        s += shot("mk-river-play", [-20, 24, 26], [-27, -0.5, 4])
        s += shot("mk-sea-pair-a", [58, 24, 52], [51.5, -1.2, 20.9], fov=6)
        s += [{"cmd": "wait", "frames": 6}, {"cmd": "screenshot", "name": "mk-sea-pair-b"}]
        s += shot("mk-river-pair-a", [-20, 24, 26], [-25.6, -0.5, 4.2], fov=6)
        s += [{"cmd": "wait", "frames": 6}, {"cmd": "screenshot", "name": "mk-river-pair-b"}]
        s += [{"cmd": "photo", "dof": "off", "pause_agents": False, "fov": 45}]
s += [{"cmd": "dump_state", "name": "mk-02-state"}, {"cmd": "wait", "frames": 30}, {"cmd": "exit"}]
out.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
print(f"wrote {out} ({len(s)} commands)")
