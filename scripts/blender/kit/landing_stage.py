"""landing_stage: a short weathered timber landing stage on posts at a river bank (owner-approved refs
assets-source/refs/boat_rowing ref-2, BQ5 2026-10-20: "the landing stage as its own prop"), sized
to moor one rowing boat. Scripted route; the mesh accumulator and the bake are in `_boat.py`.

Frame: authored against the straight river profile (docs/kit/water-contract.md §2). The origin is
the bank's waterline point: on the water surface at rest (the runtime stands it at WATER_Y = -1.4 m),
4.0 m from the channel centre line, with +x toward the centre line (so the profile distance is
d = 4 - x) and y along the flow. The deck stands DECK_Z over the water from the bank, where the bank
reaches the deck's underside, out to X_OUT; its landward end tucks into the bank. Every post foot
sits POST_BURY into the bed at its own d, and the generator checks the deck, posts and bearers
against the analytic profile.

Moored boat: `anchor-prop-0` (kind "mooring") is the rowing boat's origin, alongside the outer edge
with its bow along +y and MOOR_GAP between its side and the fender; `anchor-prop-1` (kind "bollard")
is the outer post the painter ties to; `anchor-prop-2` (kind "step") is where a walker steps on."""
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _boat as B  # noqa: E402
from lib import export, geom, scene  # noqa: E402

P = "landing_stage"
SLOTS = {
    "timber": ((0.34, 0.3, 0.24), 0.85, (1.0, 0.6, 0.0)),
    "post": ((0.13, 0.105, 0.08), 0.9, (1.0, 0.75, 0.0)),
}
LAND, W_HALF = 1.4, 4.33
DECK_Z, DECK_T = 0.45, 0.045
X_OUT, X_IN = 1.4, -0.75
HALF_Y = 1.4
PLANK_W, PLANK_GAP = 0.22, 0.02
BEARER_Y = (-1.25, 0.0, 1.25)
BEARER_H = 0.12
POSTS = [(1.3, -1.25, 0.09, True), (1.3, 0.0, 0.09, True), (1.3, 1.25, 0.09, True),
         (0.45, -1.25, 0.08, False), (0.45, 1.25, 0.08, False)]
POST_TOP = DECK_Z + 0.55
POST_BURY = 0.15
BOAT_HALF_BEAM = 0.705  # boat_rowing's beam / 2
MOOR_GAP = 0.37  # room for the stage-side oar trailed aft (reaches 0.30 m past the gunwale)
FENDER_X = X_OUT + 0.03


def smooth(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def profile(x):
    """Bed or bank height over the water at local x (water contract §2, d = 4 - x)."""
    d = abs(4.0 - x)
    bed = -1.5 * max(0.0, 1 - (d / W_HALF) ** 2) - 0.05
    bank = smooth(0.7 * W_HALF, W_HALF + 2.5, d)
    return min(LAND, bed + (LAND - bed) * bank)


def planks(m, lod):
    n = round((X_OUT - X_IN) / (PLANK_W + PLANK_GAP))
    if lod == 0:
        for k in range(n):
            x0 = X_IN + k * (PLANK_W + PLANK_GAP)
            # A weathered deck: each plank a few mm proud or sunk and a few cm long or short.
            dz = 0.006 * math.sin(2.3 * k + 0.7)
            dy0, dy1 = 0.04 * math.sin(1.7 * k), 0.04 * math.cos(2.9 * k)
            m.box((x0, -HALF_Y + dy0, DECK_Z - DECK_T), (x0 + PLANK_W, HALF_Y + dy1, DECK_Z + dz), "timber")
    elif lod == 1:
        third = (X_OUT - X_IN) / 3
        for k in range(3):
            x0 = X_IN + k * third
            m.box((x0 + 0.01, -HALF_Y, DECK_Z - DECK_T), (x0 + third - 0.01, HALF_Y, DECK_Z), "timber")
    else:
        m.box((X_IN, -HALF_Y, DECK_Z - DECK_T), (X_OUT, HALF_Y, DECK_Z), "timber", skip=("-z", "-x"))


def body(i):
    m = B.Mesh(P, SLOTS)
    planks(m, i)
    under = DECK_Z - DECK_T
    if i < 2:
        for y in BEARER_Y if i == 0 else BEARER_Y[::2]:
            m.box((X_IN - 0.1, y - 0.05, under - BEARER_H), (X_OUT + 0.05, y + 0.05, under), "timber", skip=("+z",))
        # The bank sill the landward end rests on, half sunk in the bank.
        m.tube((X_IN + 0.05, -HALF_Y - 0.1, under - 0.08), (X_IN + 0.05, HALF_Y + 0.1, under - 0.08), 0.11,
               (6, 4)[i], "timber", caps=(True, True), up=(1.0, 0.0, 0.0))
        # A fender board along the outer edge, where the moored boat lies.
        m.board([(FENDER_X, -HALF_Y, 0.12), (FENDER_X, HALF_Y, 0.12), (FENDER_X, HALF_Y, under),
                 (FENDER_X, -HALF_Y, under)], (1, 0, 0), "timber", 0.03)
    posts = POSTS if i < 2 else [p for p in POSTS if p[3] and p[1] != 0.0]
    for x, y, r, tall in posts:
        foot = profile(x) - POST_BURY
        top = POST_TOP if tall else under - BEARER_H
        m.tube((x, y, foot), (x, y, top), r, (6, 4, 3)[i], "post", caps=(False, tall and i < 2))
    if i == 0:
        for y0, y1 in ((-1.25, 1.25), (1.25, -1.25)):
            m.tube((X_OUT - 0.08, y0, profile(1.3) + 0.25), (X_OUT - 0.08, y1, under - BEARER_H - 0.02), 0.045, 4,
                   "timber")
    return m.obj(f"{P}_lod{i}")


def profile_errors():
    """Deck underside clears the bed and bank from where the bank meets it out to X_OUT, the landward
    end is tucked into the bank, and every post foot is in the bed."""
    e = []
    under = DECK_Z - DECK_T
    meet = None
    for k in range(int((X_OUT - X_IN) / 0.01) + 1):
        x = X_IN + 0.01 * k
        if profile(x) < under - 0.02:
            meet = x if meet is None else meet
        elif meet is not None:
            e.append(f"bank rises over the deck underside at x {x:.2f}")
    if profile(X_IN) < DECK_Z:
        e.append(f"landward end floats over the bank (bank {profile(X_IN):.3f} < deck {DECK_Z})")
    for x, y, _, _ in POSTS:
        if profile(x) >= under - BEARER_H:
            e.append(f"post at x {x} stands on dry bank above its bearer")
    return e, meet


def main():
    out = B.out_dir()
    scene.reset_scene(P)
    root = scene.empty(P)
    B.make_materials(P, SLOTS)
    errs, meet = profile_errors()
    bodies = [body(i) for i in range(3)]
    moor_x = FENDER_X + 0.03 + MOOR_GAP + BOAT_HALF_BEAM
    geometry = {"deckZ": DECK_Z, "deckRuntimeY": B.WATER_Y + DECK_Z, "deckX": [X_IN, X_OUT],
                "deckY": [-HALF_Y, HALF_Y], "bankMeetsDeckX": meet, "bankMeetsDeckD": 4.0 - meet if meet else None,
                "outerEdgeD": 4.0 - X_OUT, "waterDepthAtOuterEdgeM": -profile(X_OUT),
                "postFeetZ": {f"{x},{y}": round(profile(x) - POST_BURY, 3) for x, y, _, _ in POSTS},
                "mooringX": moor_x, "mooringD": 4.0 - moor_x, "waterDepthAtMooringM": -profile(moor_x),
                "frame": "origin on the bank waterline, 4.0 m from the channel centre line; +x toward it; y along the flow"}
    print(f"{P}: geometry {geometry}; profile errors {errs}")
    assert not errs, errs
    export.add_socket("anchor-prop-0", (moor_x, 0.0, 0.0), root, 90.0, kind="mooring")
    export.add_socket("anchor-prop-1", (1.3, 1.25, POST_TOP), root, 0.0, kind="bollard")
    export.add_socket("anchor-prop-2", (X_IN + 0.3, 0.0, DECK_Z), root, 0.0, kind="step")
    col = B.box_collider(bodies[2], f"{P}_collider")
    tris = B.finish(P, SLOTS, bodies, [[], [], []], col, out, __file__,
                    report_extra={"geometry": geometry}, bake_extra={"geometry": geometry})
    print(f"{P}: lodTris {tris}; body {[geom.tri_count(b) for b in bodies]}")


main()
