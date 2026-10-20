"""Write docs/checkpoints/rail_buffer_stop/shots.json: the kit-scenario review cameras for the two
buffer stops in kit_scene BUFFER_STOPS (the straight on (-3, 6) and the bend on (1, 3), both rot 2,
each with the engine buffered up to it). World positions come from the tile-frame poses in poses.py
through the cell centre and piece_rotation(2) (a -120 degree turn about +Y). The sun is at +x/+z, so
the sun side of a stop is its +p side below. Run: python3 docs/checkpoints/rail_buffer_stop/cameras.py"""
import json
import math
import pathlib

R = 16.0
STOPS = {  # cell, stop (x, z, yaw), engine (x, z, yaw): kit_scene BUFFER_STOPS
    "straight": ((-3, 6), (10.356, 0.0, 0.0), (5.886, 0.0, 0.0)),
    "bend": ((1, 3), (10.467, 0.753, -17.05), (7.35, 3.788, -57.08)),
}
ROT = math.radians(-120.0)


def cell(q, r):
    return (R * math.sqrt(3) * (q + r / 2), R * 1.5 * r)


def world(c, x, z):
    cx, cz = cell(*c)
    return (cx + x * math.cos(ROT) + z * math.sin(ROT), cz - x * math.sin(ROT) + z * math.cos(ROT))


def heading(yaw_deg):
    """World (x, z) of a piece's local +x for a placement yaw on a rot-2 tile."""
    y = math.radians(yaw_deg)
    lx, lz = math.cos(y), math.sin(y)  # +90 maps +x to +z
    return (lx * math.cos(ROT) + lz * math.sin(ROT), -lx * math.sin(ROT) + lz * math.cos(ROT))


def r3(p):
    return [round(v, 2) for v in p]


def main():
    cmds = [{"cmd": "scenario", "name": "kit"}, {"cmd": "wait", "frames": 120}]
    names = []

    def shot(name, pos, look):
        cmds.extend([{"cmd": "camera", "pos": r3(pos), "look_at": r3(look)}, {"cmd": "wait", "frames": 20},
                     {"cmd": "screenshot", "name": name}])
        names.append(name)

    for kind, (c, stop, eng) in STOPS.items():
        sx, sz = world(c, stop[0], stop[1])
        ex, ez = world(c, eng[0], eng[1])
        d = heading(stop[2])  # toward the open edge
        p = (d[1], -d[0])
        if p[0] + p[1] < 0:
            p = (-p[0], -p[1])  # the sun side (+x/+z)
        mid = ((sx + ex) / 2, 1.0, (sz + ez) / 2)
        # The straight's play view looks back up the line from beyond its open end, at the stop's rear.
        back, side = (4.0, 30.0) if kind == "bend" else (-22.0, 14.0)
        shot(f"bufstop-{kind}-play", (mid[0] + side * p[0] - back * d[0], 24.0,
                                      mid[2] + side * p[1] - back * d[1]), mid)
        beam = (sx + 0.6 * d[0], 1.4, sz + 0.6 * d[1])
        shot(f"bufstop-{kind}-sun", (sx + 4.5 * p[0] - 1.8 * d[0], 2.3, sz + 4.5 * p[1] - 1.8 * d[1]), beam)
        shot(f"bufstop-{kind}-rear", (sx + 6.0 * d[0] - 2.5 * p[0], 2.6, sz + 6.0 * d[1] - 2.5 * p[1]),
             (sx + 1.2 * d[0], 1.0, sz + 1.2 * d[1]))
        shot(f"bufstop-{kind}-top", (sx + 1.2 * d[0] + 1.5 * p[0], 7.5, sz + 1.2 * d[1] + 1.5 * p[1]),
             (sx + 1.2 * d[0], 0.4, sz + 1.2 * d[1]))
        if kind == "straight":
            shot("bufstop-eye", (sx + 4.0 * p[0] + 3.0 * d[0], 1.7, sz + 4.0 * p[1] + 3.0 * d[1]), (sx, 1.3, sz))
    cmds.append({"cmd": "exit"})
    here = pathlib.Path(__file__).parent
    (here / "shots.json").write_text(json.dumps(cmds, indent=1) + "\n")
    # The lamp at night (run with --kit_hooks night): the straight's stop from the sun side, a little
    # in front of the beam, so the lens (facing the engine) shows past the engine's buffer beam.
    sx, sz = world(STOPS["straight"][0], *STOPS["straight"][1][:2])
    d = heading(0.0)
    p = (d[1], -d[0])
    if p[0] + p[1] < 0:
        p = (-p[0], -p[1])
    night = [{"cmd": "scenario", "name": "kit"}, {"cmd": "wait", "frames": 120},
             {"cmd": "camera", "pos": r3((sx - 1.2 * d[0] + 2.2 * p[0], 2.4, sz - 1.2 * d[1] + 2.2 * p[1])),
              "look_at": r3((sx + 0.52 * d[0], 1.9, sz + 0.52 * d[1]))},
             {"cmd": "wait", "frames": 20}, {"cmd": "screenshot", "name": "bufstop-night"}, {"cmd": "exit"}]
    (here / "night.json").write_text(json.dumps(night, indent=1) + "\n")
    print(" ".join(names))


main()
