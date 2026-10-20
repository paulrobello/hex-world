"""Writes docs/checkpoints/bird_goldfinch/goldfinch.json: a land world around two oaks (the boids
checkpoint's oak cells) big enough for nine flocks, then play-distance flock shots, the roost close-ups
on both oaks and a flap pair 6 fixed frames (0.1 s) apart."""
import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
c = (0, -2)
cells = []
for dq in range(-3, 4):
    for dr in range(max(-3, -dq - 3), min(3, -dq + 3) + 1):
        cells.append((c[0] + dq, c[1] + dr))
oaks = {(0, -1), (0, -2)}
# Place outward from the start tile so every placement touches the world.
cells.sort(key=lambda p: (abs(p[0]) + abs(p[1]) + abs(p[0] + p[1]), p))
s = [
    {"cmd": "camera", "pos": [30, 150, 190], "look_at": [30, 0, 10]},
    {"cmd": "wait", "frames": 30},
    {"cmd": "select_tile", "id": "meadow"},
]
for q, r in cells:
    if (q, r) in oaks or (q, r) == (0, 0):
        continue
    s.append({"cmd": "place", "q": q, "r": r})
s += [{"cmd": "select_tile", "id": "oak"}]
s += [{"cmd": "place", "q": q, "r": r} for q, r in sorted(oaks, reverse=True)]
s += [
    {"cmd": "hover", "q": 12, "r": -12},
    {"cmd": "toggle", "what": "photo"},
    {"cmd": "photo", "dof": "off", "pause_agents": False},
    {"cmd": "wait", "frames": 240},
    {"cmd": "dump_state", "name": "gf-01-placed"},
    # Play distance: the strategic camera over the whole world, the flocks 80-110 m from it.
    {"cmd": "camera", "pos": [-14, 125, 75], "look_at": [-14, 0, -32]},
]
for k in range(4):
    s += [{"cmd": "wait", "frames": 2 if k == 0 else 110}, {"cmd": "screenshot", "name": f"gf-02-flocks-{k}"}]
s += [
    {"cmd": "wait", "frames": 30},
    # The roosts: the boids checkpoint's oak (0, -1) cameras, then the same about oak (0, -2).
    {"cmd": "camera", "pos": [-1, 17, -8], "look_at": [-14.5, 7, -25]},
    {"cmd": "wait", "frames": 30},
    {"cmd": "screenshot", "name": "gf-03-roost-a"},
    {"cmd": "dump_state", "name": "gf-03-roost"},
    {"cmd": "camera", "pos": [-4.6, 8.7, -23.4], "look_at": [-8.9, 7.45, -25.1]},
    {"cmd": "wait", "frames": 2},
    {"cmd": "screenshot", "name": "gf-03-roost-a-close"},
    {"cmd": "camera", "pos": [-15, 17, -32], "look_at": [-28.4, 7, -49]},
    {"cmd": "wait", "frames": 2},
    {"cmd": "screenshot", "name": "gf-03-roost-b"},
    {"cmd": "camera", "pos": [-18.5, 8.7, -47.4], "look_at": [-22.8, 7.45, -49.1]},
    {"cmd": "wait", "frames": 2},
    {"cmd": "screenshot", "name": "gf-03-roost-b-close"},
    # Each oak's canopy top from above and to the sun side, the meadow behind the perched birds.
    {"cmd": "camera", "pos": [-5, 19, -15], "look_at": [-13.9, 10, -25.5]},
    {"cmd": "wait", "frames": 2},
    {"cmd": "screenshot", "name": "gf-03-roost-a-top"},
    {"cmd": "camera", "pos": [-18.8, 19, -39], "look_at": [-27.7, 10, -49.5]},
    {"cmd": "wait", "frames": 2},
    {"cmd": "screenshot", "name": "gf-03-roost-b-top"},
    # Both oaks' flocks launching (22 s into each oak's wave) over the meadow, from the strategic
    # camera about 70 m above them: play distance.
    {"cmd": "camera", "pos": [-21, 80, 12], "look_at": [-21, 10, -36]},
    {"cmd": "wait", "frames": 360},
]
for k in range(30):
    s += [{"cmd": "wait", "frames": 20}, {"cmd": "screenshot", "name": f"gf-05-launch-{k:02d}"}]
s += [
    {"cmd": "dump_state", "name": "gf-05-launch"},
    # Flap pair: bulldoze an oak and undo it, then the birds flap back in; 6 fixed frames apart.
    {"cmd": "bulldoze", "q": 0, "r": -2},
    {"cmd": "wait", "frames": 30},
    {"cmd": "undo"},
    {"cmd": "camera", "pos": [-17, 21, -37], "look_at": [-27.7, 9, -49]},
    {"cmd": "wait", "frames": 270},
]
for k in range(5):
    s += [{"cmd": "wait", "frames": 54}, {"cmd": "screenshot", "name": f"gf-04-flap-{k}a"},
          {"cmd": "wait", "frames": 6}, {"cmd": "screenshot", "name": f"gf-04-flap-{k}b"}]
s += [
    {"cmd": "dump_state", "name": "gf-04-flap"},
    {"cmd": "wait", "frames": 10},
    {"cmd": "exit"},
]
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("[\n" + ",\n".join("  " + json.dumps(x) for x in s) + "\n]\n")
print(f"wrote {out}: {len(s)} commands, {len(cells) - 1} tiles")
