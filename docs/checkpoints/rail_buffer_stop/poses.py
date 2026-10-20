"""Poses of rail_buffer_stop and the engine standing buffered up to it at a straight's and a bend's
open edge 0, in the rail piece's own frame (runtime tile frame: x, z; yaw_deg as kit_scene's
Placement, +90 maps +x to +z). These are the kit_scene constants BUFFER_STOPS and ENGINE_AT_STOPS.

The stop follows docs/checkpoints/rail_buffer_stop/runtime-notes.md: pivot on the track at path
distance STOP_U from the open edge, its +x along the chord to the track point at STOP_U - CHORD.
The engine is posed as hex-sim `Railway::poses` poses it with its front buffer face at STOP_U: both
outer axles on the track at STOP_U + 4.47 -/+ 2.0 (CONSIST), the body on their chord, centred
between them, smokebox (+x) toward the edge. Run: python3 docs/checkpoints/rail_buffer_stop/poses.py"""
import math

APOTHEM = 13.856406
BEND_R, BEND_CY = 8.0, -8.0  # scripts/blender/kit/_rail.py BEND_R, BEND_C (APOTHEM, -8)
STOP_U, CHORD = 3.5, 2.24  # scripts/blender/kit/rail_buffer_stop.py
ENGINE_CENTRE, ENGINE_HALF_WB, ENGINE_HALF_LEN = 4.47, 2.0, 4.47  # hex_sim::rail::CONSIST, train_engine


def track(kind, u):
    """Blender tile-frame (x, y) of the centreline u metres in from edge 0."""
    if kind == "straight":
        return (APOTHEM - u, 0.0)
    t = u / BEND_R
    return (APOTHEM - BEND_R * math.sin(t), BEND_CY + BEND_R * math.cos(t))


def pose(a, b):
    """A pose at a facing along (b - a), in runtime (x, z, yaw_deg)."""
    dx, dy = b[0] - a[0], b[1] - a[1]
    yaw = math.degrees(math.atan2(dy, dx))  # Blender top view, counter-clockwise
    return (round(a[0], 3), round(-a[1], 3) + 0.0, round(-yaw, 2) + 0.0)  # runtime z = -Blender y


def main():
    for kind in ("straight", "bend"):
        stop = pose(track(kind, STOP_U), track(kind, STOP_U - CHORD))
        front = track(kind, STOP_U + ENGINE_CENTRE - ENGINE_HALF_WB)
        back = track(kind, STOP_U + ENGINE_CENTRE + ENGINE_HALF_WB)
        mid = ((front[0] + back[0]) / 2, (front[1] + back[1]) / 2)
        eng = pose(mid, (mid[0] + front[0] - back[0], mid[1] + front[1] - back[1]))
        # Where the engine's buffer-face centre ends up against the stop (stop frame, Blender).
        l = math.hypot(front[0] - back[0], front[1] - back[1])
        d = ((front[0] - back[0]) / l, (front[1] - back[1]) / l)
        face = (mid[0] + ENGINE_HALF_LEN * d[0], mid[1] + ENGINE_HALF_LEN * d[1])
        o = track(kind, STOP_U)
        sy = math.radians(-stop[2])
        rel = (face[0] - o[0], face[1] - o[1])
        along = rel[0] * math.cos(sy) + rel[1] * math.sin(sy)
        across = -rel[0] * math.sin(sy) + rel[1] * math.cos(sy)
        skew = (-eng[2]) - (-stop[2])
        print(f"{kind}: stop offset [{stop[0]}, {stop[1]}] yaw {stop[2]}; engine offset [{eng[0]}, {eng[1]}] "
              f"yaw {eng[2]}; engine face centre in the stop frame x {along:+.3f} y {across:+.3f}, "
              f"skew {skew:+.1f} deg")


main()
