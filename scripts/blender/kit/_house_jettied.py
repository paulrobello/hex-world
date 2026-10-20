"""House family, jettied two-storey variant: the builder behind house_twostorey_b.py (PRD §6 Variety,
house_cluster house 2).

A two-storey timber-framed house in the house_longhouse style (owner-approved refs, assets-source/refs/
house_cluster: "including the timber-framed English cottage style"), with a silhouette unlike
house_twostorey's gabled box: the upper storey is jettied over the ground floor on the front (+x), the
roof is hipped (clay-tile courses run round all four slopes) on a short ridge, and a brick stack stands
on that ridge over the door (a lobby-entry plan). A girding band (the bressumer on the front) rings the
house at the first floor; the upper front is close-studded.

Slot fit (data/tiles/house_cluster.ron house 2 at (-7.5, 0) yaw 0; house_pad.py DOORSTEPS[2]; hex-sim
tests/parking.rs "two-storey"): walls inside x ±2.5, |y| <= 3.5 (the parking box is x ±2.5, |y| <= 3.6),
the upper front on house_twostorey's front line x = 2.5 and the ground floor set back under the jetty,
the door at y = 0 with its step reaching over the pad's doorstep (x 2.5..3.4, |y| <= 0.8). LOD2, and so
the box collider, sits inside house_twostorey's collider box (scatter::GROUND_BOXES, Blender x ±2.8,
y -4.25..3.8) with a corner reach under its FOOTPRINTS ground (4.75 m); LOD0 and LOD1 sit inside
house_twostorey's LOD0 extents.

Pipeline: `_house_longhouse.build_piece` runs unchanged, driven through its module globals: `build` is
this module's builder and its fit bounds are house_twostorey's (they are named COTTAGE_* there, so a
fit failure prints "house_cottage's footprint"). So the masks bake, the unbaked-texel fill, the
zero-unbaked-triangle assert, the LOD2 box collider and the export are the family's, and no approved
helper is edited. Frame as _house.py: Blender (x, y, z) exports as runtime (x, z, -y), the pivot is the
footprint centre at ground level.

Timbers never share a face plane where they overlap (z-fighting in close-ups): bands are the proudest
members, and posts, studs and braces step back from them in that order, each ending inside a prouder
member so its open end never shows (the build asserts it for braces); bands that turn a corner run
past it by their own proud at the same width as the other wall's band, so each open end is capped by
the other's face (only their few-cm corner squares share a plane, in one colour, as on house_longhouse);
corner posts on two walls abut rather than overlap. Studs are clipped against every opening on their wall, and the
build fails if any timber's face overlaps a window's or the door's frame, sill or glass."""
import math

from mathutils import Vector

import _house_longhouse as lh
from _house import DROP, PLINTH_OUT, PLINTH_TOP, Z, Mesh, Wall, roof_profile
from _house_longhouse import beam, chimney, plinth

# house_twostorey's slot (Blender x, y): its collider box (its LOD2 hull's bounds, scatter::GROUND_BOXES),
# its LOD0 extents (the porch roof to x 3.7, the gable stack's oversail to y -4.3) and its ground reach
# (scatter::FOOTPRINTS).
TWOSTOREY_BOX = ((-2.8, 2.8), (-4.25, 3.8))
TWOSTOREY_LOD0 = ((-2.816, 3.7), (-4.3, 3.8))
TWOSTOREY_REACH = 4.75
PARKING_WALLS = (2.5, 3.6)  # hex-sim tests/parking.rs "two-storey": walls within |x| <= 2.5, |y| <= 3.6
PAD_STEP = (2.5, 0.8)  # house_pad DOORSTEPS[2] in this frame: starts at x 2.5, |y| <= 0.8
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
# _house.Wall's opening geometry, which the clipping below must cover: the window frame ring's width,
# the sill box's overhang past the frame and its depth below it, and the door frame's width.
WIN_FRAME, SILL_OVER, SILL_BELOW, DOOR_FRAME, DOOR_STEP_OVER = 0.09, 0.06, 0.08, 0.1, 0.2
# The sole band sits on the plinth ledge and the walls run from DROP (the family's constants).
assert DROP == -0.35 and PLINTH_TOP == 0.32, (DROP, PLINTH_TOP)

BUILT = []  # names of the LOD objects this builder made (checked after lh.build_piece returns)


def walls_of(spec):
    xl, xu, xb, hw = spec["front_lo"], spec["front_hi"], spec["back"], spec["half_w"]
    return {"FL": Wall((1, 0, 0), (xl, 0, 0)), "FU": Wall((1, 0, 0), (xu, 0, 0)), "B": Wall((-1, 0, 0), (xb, 0, 0)),
            "SL": Wall((0, -1, 0), (0, -hw, 0)), "SR": Wall((0, 1, 0), (0, hw, 0))}


def openings(spec):
    """Per wall, the (u0, z0, u1, z1) rectangles every opening covers at its LOD0 detail (frame ring,
    sill box, door frame and step): the largest any LOD draws."""
    rects = {}
    for name, u, zs, w, h, major in spec["windows"]:
        if major:  # LOD0 detail 0: frame ring plus the sill box under it
            rects.setdefault(name, []).append((u - w / 2 - WIN_FRAME - SILL_OVER, zs - WIN_FRAME - SILL_BELOW,
                                               u + w / 2 + WIN_FRAME + SILL_OVER, zs + h + WIN_FRAME))
        else:  # LOD0 detail 1: the frame ring
            rects.setdefault(name, []).append((u - w / 2 - WIN_FRAME, zs - WIN_FRAME, u + w / 2 + WIN_FRAME,
                                               zs + h + WIN_FRAME))
    du, dw, dh = spec["door"]
    rects.setdefault("FL", []).extend([(du - dw / 2 - DOOR_FRAME, DROP, du + dw / 2 + DOOR_FRAME, dh + DOOR_FRAME),
                                       (du - dw / 2 - DOOR_STEP_OVER, DROP, du + dw / 2 + DOOR_STEP_OVER, 0.2)])
    return rects


def member_rect(u0, z0, u1, z1, width):
    """Axis-aligned bounds of a member's front face in wall (u, z) (exact for vertical and horizontal
    members, conservative for braces)."""
    d = Vector((u1 - u0, z1 - z0)).normalized()
    h = Vector((-d.y, d.x)) * (width / 2)
    us = [u0 + h.x, u0 - h.x, u1 + h.x, u1 - h.x]
    zs = [z0 + h.y, z0 - h.y, z1 + h.y, z1 - h.y]
    return min(us), min(zs), max(us), max(zs)


def overlaps(a, b, eps=1e-6):
    return a[0] < b[2] - eps and b[0] < a[2] - eps and a[1] < b[3] - eps and b[1] < a[3] - eps


def clip_stud(u, z0, z1, width, rects):
    """The parts of a vertical stud from z0 to z1 that miss every opening rectangle; each cut ends on
    the opening's edge, so the frame's or the sill's outer face caps the open stud end."""
    segs = [(z0, z1)]
    for r in rects:
        if not (u - width / 2 < r[2] and r[0] < u + width / 2):
            continue
        out = []
        for a, b in segs:
            if r[1] > a:
                out.append((a, min(b, r[1])))
            if r[3] < b:
                out.append((max(a, r[3]), b))
        segs = [(a, b) for a, b in out if b - a > 0.05]
    return segs


def members(spec):
    """Every timber as (wall, u0, z0, u1, z1, keep at LOD1, width, proud): wall "S" entries on both
    hip ends (SR mirrored: its u runs along -x), studs clipped by the openings. Fails if any member's
    face overlaps an opening."""
    rects = openings(spec)
    out = []
    for name, u0, z0, u1, z1, keep, width, proud in spec["timbers"]:
        if name == "S":
            out += [("SL", u0, z0, u1, z1, keep, width, proud), ("SR", -u0, z0, -u1, z1, keep, width, proud)]
        else:
            out.append((name, u0, z0, u1, z1, keep, width, proud))
    for name, us, z0, z1, width, proud, keep_us in spec["studs"]:
        for u in us:
            segs = clip_stud(u, z0, z1, width, rects.get(name, []))
            whole = segs == [(z0, z1)]
            for a, b in segs:
                out.append((name, u, a, u, b, whole and u in keep_us, width, proud))
    bad = [(m[0], m[1:5]) for m in out for r in rects.get(m[0], []) if overlaps(member_rect(*m[1:5], m[6]), r)]
    assert not bad, f"timbers over openings: {bad}"
    # A brace's open ends must sit inside a prouder post or band on its wall, so they never show.
    square = [m for m in out if m[1] == m[3] or m[2] == m[4]]
    for name, u0, z0, u1, z1, _, width, proud in out:
        if u0 == u1 or z0 == z1:
            continue
        d = Vector((u1 - u0, z1 - z0)).normalized()
        h = Vector((-d.y, d.x)) * (width / 2)
        for c in ((u0 + h.x, z0 + h.y), (u0 - h.x, z0 - h.y), (u1 + h.x, z1 + h.y), (u1 - h.x, z1 - h.y)):
            hidden = [m for m in square if m[0] == name and m[7] > proud
                      and (lambda r: r[0] <= c[0] <= r[2] and r[1] <= c[1] <= r[3])(member_rect(*m[1:5], m[6]))]
            assert hidden, f"brace end corner {c} on {name} shows (brace {u0, z0, u1, z1})"
    return out


def hip_roof(mesh, spec, slot, lod):
    """The hipped roof over the upper storey's walls, centred on them, at equal pitch all round. Each
    vertex (x, z) of `_house.roof_profile`'s front half (eave underside, tile courses with their
    kicked butts, ridge) becomes a rectangle inset uniformly from the eave line, so every course runs
    round all four slopes and meets at the hips; the top ring is the ridge line. A soffit closes the
    underside from the eave to just inside the walls. Returns the ridge height (no kick)."""
    xu, xb, hw, eave = spec["front_hi"], spec["back"], spec["half_w"], spec["eave"]
    pitch = math.radians(spec["pitch"])
    o, thick = spec["overhang"], spec["roof_thick"]
    hd, cx = (xu - xb) / 2, (xu + xb) / 2
    pts, ridge = roof_profile(hd, pitch, o, spec["courses"] if lod == 0 else 0, spec["kick"], thick)
    nf = (len(pts) - 2) // 2
    front = pts[:nf + 1]
    assert front[-1][0] == 0.0 and all(x > 0 for x, _ in front[:-1]), front
    zb = eave - o * math.tan(pitch)
    half_ridge = hw - hd
    assert half_ridge > 0.3, half_ridge

    def ring(x, z):
        return [(cx + x, -x - half_ridge, zb + z), (cx + x, x + half_ridge, zb + z),
                (cx - x, x + half_ridge, zb + z), (cx - x, -x - half_ridge, zb + z)]

    sides = (X, Y, -X, -Y)  # c0 -> c1 is the front slope, then round counter-clockwise
    rings = [ring(x, z) for x, z in front[:-1]]
    for a, b in zip(rings, rings[1:]):
        for k in range(4):
            mesh.face([a[k], a[(k + 1) % 4], b[(k + 1) % 4], b[k]], slot, sides[k])
    zt = zb + front[-1][1]
    r0, r1 = (cx, -half_ridge, zt), (cx, half_ridge, zt)
    a = rings[-1]
    mesh.face([a[0], a[1], r1, r0], slot, X)
    mesh.face([a[1], a[2], r1], slot, Y)
    mesh.face([a[2], a[3], r0, r1], slot, -X)
    mesh.face([a[3], a[0], r0], slot, -Y)
    inset = o + 0.02  # the soffit ends 2 cm inside the walls, on the slab's underside plane
    soffit = ring(hd + o - inset, -thick / math.cos(pitch) + inset * math.tan(pitch))
    for k in range(4):
        mesh.face([rings[0][k], rings[0][(k + 1) % 4], soffit[(k + 1) % 4], soffit[k]], slot, -Z)
    return zb + ridge


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    xl, xu, xb, hw = spec["front_lo"], spec["front_hi"], spec["back"], spec["half_w"]
    jet, eave = spec["jetty"], spec["eave"]
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    # Wall tops 3 cm above the roof slab's underside at the wall line (the same on all four walls of an
    # equal-pitch, equal-overhang hip), so they end inside the slab, behind the soffit.
    pitch = math.radians(spec["pitch"])
    top = eave - spec["roof_thick"] / math.cos(pitch) + 0.03

    # Walls: the ground-floor front under the jetty, the jetty's soffit, the upper front, and the back and
    # hip ends split at the jetty line (the hip ends also at the ground floor's front), so every corner
    # edge is shared vertex for vertex and no T-junction opens a crack.
    mesh.face([(xl, -hw, z0), (xl, hw, z0), (xl, hw, jet), (xl, -hw, jet)], s["walls"], X)
    mesh.face([(xl, -hw, jet), (xu, -hw, jet), (xu, hw, jet), (xl, hw, jet)], s["timber"], -Z)
    mesh.face([(xu, -hw, jet), (xu, hw, jet), (xu, hw, top), (xu, -hw, top)], s["walls"], X)
    for za, zb_ in ((z0, jet), (jet, top)):
        mesh.face([(xb, -hw, za), (xb, hw, za), (xb, hw, zb_), (xb, -hw, zb_)], s["walls"], -X)
    for y, ny in ((hw, Y), (-hw, -Y)):
        mesh.face([(xb, y, z0), (xl, y, z0), (xl, y, jet), (xb, y, jet)], s["walls"], ny)
        mesh.face([(xb, y, jet), (xl, y, jet), (xl, y, top), (xb, y, top)], s["walls"], ny)
        mesh.face([(xl, y, jet), (xu, y, jet), (xu, y, top), (xl, y, top)], s["walls"], ny)
    if lod < 2:
        plinth(mesh, [(xl, -hw), (xl, hw), (xb, hw), (xb, -hw)], s["stone"])
        # Flagstones under the jetty, from the plinth to the upper front's line (where house_pad's gravel
        # and doorstep start), so no bare cap shows in the set-back strip.
        ax0, ay = xl + PLINTH_OUT, hw + PLINTH_OUT
        az = spec["apron_top"]
        lh.box(mesh, ((ax0 + xu) / 2, 0.0, (DROP + az) / 2), X, Y, Z, (xu - ax0, 2 * ay, az - DROP), s["stone"],
               drop=(-X, -Z))

    ridge_z = hip_roof(mesh, spec, s["roof"], lod)
    chimney(mesh, spec, s, lod, ridge_z)

    W = walls_of(spec)
    du, dw, dh = spec["door"]
    W["FL"].door(mesh, du, dw, dh, s, lod)
    for name, u, zs, ww, hh, major in spec["windows"]:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zs, ww, hh, s, detail, bars=True)
    if lod < 2:
        for name, u0, za, u1, zb, keep, width, proud in members(spec):
            if lod == 0 or keep:
                beam(mesh, W[name], u0, za, u1, zb, width, proud, s["timber"])

    o = mesh.obj(f"{spec['piece']}_lod{lod}", mats)
    if lod == 2:
        xs, ys, _ = zip(*[v.co[:] for v in o.data.vertices])
        reach = max(math.hypot(x, y) for x in (min(xs), max(xs)) for y in (min(ys), max(ys)))
        print(f"{spec['piece']}: LOD2 (collider box) x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f}"
              f" reach {reach:.3f}")
        assert reach <= TWOSTOREY_REACH, f"collider box corner reach {reach:.3f} > {TWOSTOREY_REACH}"
    BUILT.append(o.name)
    return o


def check_spec(spec):
    """The slot contract (module docstring) on the spec's numbers."""
    xl, xu, xb, hw = spec["front_lo"], spec["front_hi"], spec["back"], spec["half_w"]
    assert max(xu, xl, -xb) <= PARKING_WALLS[0] and hw <= PARKING_WALLS[1], (xl, xu, xb, hw)
    assert xu > xl, "the upper storey is jettied over the ground floor"
    du, dw, _ = spec["door"]
    assert du == 0.0 and dw / 2 + DOOR_STEP_OVER <= PAD_STEP[1], spec["door"]
    # _house.Wall.door's step runs 0.4 m out from the wall: it must reach onto the pad's doorstep.
    assert xl + 0.4 >= PAD_STEP[0] + 0.03, f"door step ends at x {xl + 0.4} before the pad step at {PAD_STEP[0]}"
    assert spec["chimney"][1] == 0.0 and spec["chimney_size"][1] / 2 < spec["half_w"] - (xu - xb) / 2


def build_piece(spec, generator):
    check_spec(spec)
    lh.build = build
    lh.COTTAGE_BOUNDS = TWOSTOREY_BOX
    lh.COTTAGE_LOD0_BOUNDS = TWOSTOREY_LOD0
    BUILT.clear()
    root, lods = lh.build_piece(spec, generator)
    assert [o.name for o in lods] == BUILT, ("_house_longhouse.build_piece did not use this builder", BUILT)
    return root, lods
