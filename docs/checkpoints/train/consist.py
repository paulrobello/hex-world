"""Consist poses for the train review (no bpy): each vehicle stands with its outer axles on the track
centreline, and consecutive vehicles are spaced by the smallest gap at which no buffer head crosses
its neighbour's buffer face (on the straight: buffers touching).

Track path, in rail_bend_60_center's Blender frame (scripts/blender/kit/_rail.py): s = 0 at the bend's
local edge-0 midpoint (13.856, 0), along the 8 m arc about corner 0 (BEND_C) to its local edge-1
midpoint at s = 16.755 (the joint with the straight in the kit layout, where the bend sits at rot 2),
then straight on along edge 1's outward normal. The engine leads toward s = 0.

`python3 docs/checkpoints/train/consist.py` prints the coupling numbers and the kit_scene.rs
Placement literals (bend at cell (1, 2), rot 2; offset = (x, -y), yaw_deg = -heading)."""
import math

APOTHEM = 16.0 * math.sqrt(3) / 2
BEND_R, BEND_C, BEND_FROM, BEND_TURN = 8.0, (APOTHEM, -8.0), 90.0, 120.0
RAIL_TOP = 0.46
BUF_Y = 0.85
# piece: (half axle spacing of the outer axles, buffer head face |x|)
VEHICLES = {"train_engine": (2.0, 4.47), "train_carriage": (1.5, 3.57)}
CONSIST = ("train_engine", "train_carriage", "train_carriage")
ARC_LEN = BEND_R * math.radians(BEND_TURN)
EDGE1 = (math.cos(math.radians(-60)), math.sin(math.radians(-60)))
ENGINE_TO_CARRIAGE = VEHICLES["train_engine"][1] + VEHICLES["train_carriage"][1]
CARRIAGE_TO_CARRIAGE = 2 * VEHICLES["train_carriage"][1]


def path(s):
    if s <= ARC_LEN:
        a = math.radians(BEND_FROM) + s / BEND_R
        return (BEND_C[0] + BEND_R * math.cos(a), BEND_C[1] + BEND_R * math.sin(a))
    p1 = path(ARC_LEN)
    return (p1[0] + (s - ARC_LEN) * EDGE1[0], p1[1] + (s - ARC_LEN) * EDGE1[1])


def pose(piece, s_front):
    """(pivot x, y, heading rad) with the front axle at s_front and the rear axle 2a behind on the path."""
    a = VEHICLES[piece][0]
    pf = path(s_front)
    lo, hi = s_front, s_front + 4 * a
    for _ in range(60):
        mid = (lo + hi) / 2
        q = path(mid)
        lo, hi = (mid, hi) if math.hypot(q[0] - pf[0], q[1] - pf[1]) < 2 * a else (lo, mid)
    pr = path(hi)
    head = math.atan2(pf[1] - pr[1], pf[0] - pr[0])
    return ((pf[0] + pr[0]) / 2, (pf[1] + pr[1]) / 2, head), hi


def to_local(p, q):
    """q in the frame of vehicle pose p."""
    dx, dy = q[0] - p[0], q[1] - p[1]
    c, s = math.cos(p[2]), math.sin(p[2])
    return (c * dx + s * dy, -s * dx + c * dy)


def to_world(p, local):
    c, s = math.cos(p[2]), math.sin(p[2])
    return (p[0] + c * local[0] - s * local[1], p[1] + s * local[0] + c * local[1])


def penetrates(pa, la, pb, lb):
    """Does vehicle b's front buffer head cross vehicle a's rear buffer face?"""
    return any(to_local(pa, to_world(pb, (lb, y)))[0] > -la + 1e-6 for y in (BUF_Y, -BUF_Y))


def consist_on_path(s0):
    poses, s = [], s0
    prev = None
    for piece in CONSIST:
        a, lb = VEHICLES[piece]
        if prev is None:
            p, s_rear = pose(piece, s)
        else:
            (pa, la, s_prev_rear) = prev
            lo, hi = s_prev_rear, s_prev_rear + 12.0
            for _ in range(60):  # smallest front-axle s with no buffer penetration
                mid = (lo + hi) / 2
                p, _ = pose(piece, mid)
                lo, hi = (mid, hi) if penetrates(pa, la, p, lb) else (lo, mid)
            p, s_rear = pose(piece, hi)
        poses.append((piece, *p))
        prev = (p, lb, s_rear)
    return poses


def bend_poses_blender():
    """(piece, x, y, yaw_deg) in the bend piece's Blender frame; engine buffers 0.3 m inside the
    bend's open end."""
    a, lb = VEHICLES["train_engine"]
    s0 = 0.3 + (lb - a)  # front buffer overhang beyond the front axle, measured along the path
    return [(pc, x, y, math.degrees(h)) for pc, x, y, h in consist_on_path(s0)]


def straight_poses():
    """(piece, x, y, yaw_deg) on rail_straight_center (track along Blender x), buffers touching."""
    x = 5.5
    out = [("train_engine", x, 0.0, 0.0)]
    x -= ENGINE_TO_CARRIAGE
    out.append(("train_carriage", x, 0.0, 0.0))
    x -= CARRIAGE_TO_CARRIAGE
    out.append(("train_carriage", x, 0.0, 0.0))
    return out


def pivot_spacing(poses):
    return [math.hypot(b[1] - a[1], b[2] - a[2]) for a, b in zip(poses, poses[1:])]


def check_axles_on_track(poses):
    """Every outer axle point within 1 cm of the path (arc or straight)."""
    worst = 0.0
    for pc, x, y, yaw in poses:
        a = VEHICLES[pc][0]
        for sx in (1, -1):
            q = to_world((x, y, math.radians(yaw)), (sx * a, 0.0))
            d_arc = abs(math.hypot(q[0] - BEND_C[0], q[1] - BEND_C[1]) - BEND_R)
            p1 = path(ARC_LEN)
            v = (q[0] - p1[0], q[1] - p1[1])
            along = v[0] * EDGE1[0] + v[1] * EDGE1[1]
            d_line = abs(-v[0] * EDGE1[1] + v[1] * EDGE1[0]) if along >= 0 else 1e9
            worst = max(worst, min(d_arc, d_line))
    return worst


if __name__ == "__main__":
    bend = bend_poses_blender()
    err = check_axles_on_track(bend)
    assert err < 0.01, err
    st = pivot_spacing(straight_poses())
    bd = pivot_spacing(bend)
    print(f"straight pivot spacing (buffers touching): {[round(v, 3) for v in st]}")
    print(f"bend pivot spacing (inner buffers touching): {[round(v, 3) for v in bd]} (axle error {err:.4f} m)")
    for pc, x, y, yaw in bend:
        print(f'    ("{pc}", [{x:.3f}, {-y:.3f}], {-yaw:.1f}),')
