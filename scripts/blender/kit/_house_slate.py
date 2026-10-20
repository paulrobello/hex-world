"""House family, slate cottage variant: the builder behind house_cottage_b.py (PRD §6 Variety,
house_cluster's cottage slots).

A single-storey rendered cottage after the slate-roofed house on the right of the owner-approved
ref-1 (assets-source/refs/house_cluster): cream lime render with dressed stone quoins, a grey slate
roof with slate courses, a stone stack rising through the ridge at each gable end, a small gabled
porch over the door whose slate roof runs back into the main slope, oak bargeboards, oak casements and
an oak plank door. Unlike house_cottage (timber-framed, steep clay roof, one stack outside the gable)
and house_longhouse (timber-framed L plan, clay), it reads at play distance as a low, dark-roofed,
pale-walled box with two stacks and a porch.

Slot fit (data/tiles/house_cluster.ron cottage slots, house_pad.py DOORSTEPS[0..1], hex-sim
tests/parking.rs "cottage 0/1" and "doorstep 0/1"): walls inside house_cottage's 4.4 x 6.4 m wall box
(x ±2.2, y ±3.2), the door at y 0 with its step on the pad's doorstep; LOD2, and so the box collider,
inside house_cottage's collider box (scatter::GROUND_BOXES, Blender x ±2.5, y -3.45..3.85); LOD0 and
LOD1 inside house_cottage's LOD0 extents except the porch, which stays inside the parking test's
doorstep box (x 2.2..3.1, |y| <= 0.7): `porch_errors` asserts both.

Pipeline: `_house_longhouse.build_piece` runs unchanged, driven through its module globals as
`_house_jettied` does: `build` is this module's builder and the fit bounds are house_cottage's with the
LOD0 x bound widened to the doorstep box (the porch rule above is asserted separately). So the masks
bake, the unbaked-texel fill, the zero-unbaked-triangle assert, budgets.check, the LOD2 box collider
and the export are the family's, and no approved helper is edited. Frame as _house.py: Blender
(x, y, z) exports as runtime (x, z, -y), the pivot is the footprint centre at ground level, and the
front door faces +x.

The porch roof is swept from behind the front wall, from a constant end inside the main roof's slab,
so both closed roofs interpenetrate and meet in a clean valley; its back cap sits under the main roof
and its underside behind the wall is inside the house (`porch_back`)."""
import math

from mathutils import Vector

import _house_longhouse as lh
from _house import DROP, PLINTH_TOP, Z, Mesh, Wall, roof_profile
from _house_longhouse import beam, box, chimney, plinth, sweep

# house_cottage's slot (Blender x, y): its collider box (scatter::GROUND_BOXES), its LOD0 extents with
# x widened to the doorstep box, and the parking test's wall and doorstep boxes.
COTTAGE_BOX = ((-2.5, 2.5), (-3.45, 3.85))
LOD0_BOUNDS = ((-2.53, 3.1), (-3.45, 3.9))
COTTAGE_LOD0_X = 2.6
PARKING_WALLS = (2.2, 3.2)
DOORSTEP = (3.1, 0.7)  # parking.rs doorstep 0/1 in this frame: x 2.2..3.1, |y| <= 0.7
COTTAGE_REACH = 4.3  # scatter::FOOTPRINTS house_cottage ground
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
OVERLAP = 0.08  # how far the porch roof's back end sits inside the main roof's slab (m, horizontal)
assert DROP == -0.35 and PLINTH_TOP == 0.32, (DROP, PLINTH_TOP)

BUILT = []  # names of the LOD objects this builder made (checked after lh.build_piece returns)


def walls_of(spec):
    hd, hw = spec["depth"] / 2, spec["width"] / 2
    return {"front": Wall((1, 0, 0), (hd, 0, 0)), "back": Wall((-1, 0, 0), (-hd, 0, 0)),
            "left": Wall((0, -1, 0), (0, -hw, 0)), "right": Wall((0, 1, 0), (0, hw, 0))}


def main_roof(spec):
    """(profile, ridge rise, eave-line z, pitch) of the main roof at full detail's geometry."""
    hd, pitch = spec["depth"] / 2, math.radians(spec["pitch"])
    zb = spec["eave"] - spec["overhang"] * math.tan(pitch)
    return hd + spec["overhang"], zb, pitch


def porch_back(spec):
    """The porch roof's constant back end (Blender x): where the main roof's top surface reaches the
    porch's kicked ridge, OVERLAP further inside the main slab."""
    run, zb, pitch = main_roof(spec)
    pu, pw, pd, pe, pp, po, _ = spec["porch"]
    pzb = pe - po * math.tan(math.radians(pp))
    top = pzb + (pw / 2 + po) * math.tan(math.radians(pp)) + spec["porch_kick"]
    return run - (top - zb) / math.tan(pitch) - OVERLAP


def porch(mesh, spec, s, lod):
    """Gabled porch over the door: two oak posts on the pad's doorstep (buried to DROP), a tie beam,
    a rendered tympanum, a slate roof swept back into the main slope and oak bargeboards."""
    hd = spec["depth"] / 2
    pu, pw, pd, pe, pp_deg, po, pthick = spec["porch"]
    pp = math.radians(pp_deg)
    front = hd + pd  # the posts' outer face
    post, tie_h = 0.12, 0.14
    pzb = pe - po * math.tan(pp)  # the porch roof's eave line (its profile's z = 0)
    tie_z = pzb - 0.15  # the tie beam's centre, under the roof's underside at the posts
    py = pw / 2 - post / 2
    for sg in (-1, 1):
        box(mesh, (front - post / 2, pu + sg * py, (DROP + tie_z - tie_h / 2) / 2), X, Y, Z,
            (post, post, tie_z - tie_h / 2 - DROP), s["timber"], drop=(-Z, Z))
    box(mesh, (front - post / 2, pu, tie_z), X, Y, Z, (post, pw, tie_h), s["timber"])
    pts, prise = roof_profile(pw / 2, pp, po, spec["porch_courses"] if lod == 0 else 0, spec["porch_kick"], pthick)
    s1 = front + spec["porch_front_over"]
    sweep(mesh, s["roof"], pts, lambda x, z, t: (t, pu + x, pzb + z), porch_back(spec), s1, (False, True))
    # The tympanum: under the roof's underside, above the tie beam, both faces (2 cm apart).
    under = pzb + prise - pthick / math.cos(pp)  # the underside's apex
    zt = tie_z + tie_h / 2
    half = (under - zt) / math.tan(pp)
    for x, n in ((front - post / 2, X), (front - post / 2 - 0.02, -X)):
        mesh.face([(x, pu - half, zt), (x, pu + half, zt), (x, pu, under - 0.01)], s["walls"], n)
    if lod < 2:
        # Bargeboards on the porch's front verge, starting 5 cm up the slope so their lower corners
        # stay inside the doorstep box.
        fw = Wall((1, 0, 0), (s1, 0, 0))
        r = pw / 2 + po - 0.05
        for sg in (-1, 1):
            beam(mesh, fw, pu + sg * r, pzb - 0.03 + 0.05 * math.tan(pp), pu, pzb + prise - 0.03, 0.14, 0.03,
                 s["timber"], out=0.002)


def quoins(mesh, spec, s):
    """Dressed corner stones, alternately long on the front/back face and on the gable face, each
    standing 4 cm proud of both faces (no faces inside the walls)."""
    hd, hw = spec["depth"] / 2, spec["width"] / 2
    n_q, qh, step = spec["quoins"]
    for sx in (1, -1):
        for sy in (1, -1):
            for q in range(n_q):
                a, b = (0.46, 0.28) if q % 2 == 0 else (0.28, 0.46)
                c = (sx * (hd - a / 2 + 0.04), sy * (hw - b / 2 + 0.04), PLINTH_TOP + qh / 2 + step * q)
                box(mesh, c, X, Y, Z, (a, b, qh), s["stone"], drop=(-sx * X, -sy * Y))


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    hd, hw, eave = spec["depth"] / 2, spec["width"] / 2, spec["eave"]
    run, zb, pitch = main_roof(spec)
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    apex = eave + hd * math.tan(pitch) - 0.05  # inside the roof slab

    mesh.face([(hd, -hw, z0), (hd, hw, z0), (hd, hw, eave), (hd, -hw, eave)], s["walls"], X)
    mesh.face([(-hd, -hw, z0), (-hd, hw, z0), (-hd, hw, eave), (-hd, -hw, eave)], s["walls"], -X)
    for y, n in ((hw, Y), (-hw, -Y)):
        mesh.face([(hd, y, z0), (-hd, y, z0), (-hd, y, eave), (0.0, y, apex), (hd, y, eave)], s["walls"], n)
    if lod < 2:
        plinth(mesh, [(hd, -hw), (hd, hw), (-hd, hw), (-hd, -hw)], s["stone"])

    gov = spec["gable_overhang"]
    pts, rise = roof_profile(hd, pitch, spec["overhang"], spec["courses"] if lod == 0 else 0, spec["kick"],
                             spec["roof_thick"])
    sweep(mesh, s["roof"], pts, lambda x, z, t: (x, t, zb + z), -hw - gov, hw + gov, (True, True))
    ridge_z = zb + rise
    for cy in spec["stacks"]:
        chimney(mesh, {**spec, "chimney": (0.0, cy)}, s, lod, ridge_z)

    if lod < 2:
        # Oak bargeboards along both gables' verges, over the slate course ends. They start `inset` up
        # the slope so their lower corners (half their width across the slope) stay inside the eave
        # line, within house_cottage's LOD0 extent at the back (x -2.53).
        inset = 0.11 * math.sin(pitch) + 0.01
        for name in ("left", "right"):
            w = walls_of(spec)[name]
            for sg in (-1, 1):
                beam(mesh, w, sg * (run - inset), zb - 0.05 + inset * math.tan(pitch), 0.0, ridge_z - 0.05, 0.22,
                     0.05, s["timber"], out=gov + 0.002)
    if lod == 0:
        quoins(mesh, spec, s)
    if lod < 2:
        porch(mesh, spec, s, lod)

    W = walls_of(spec)
    du, dw, dh = spec["door"]
    W["front"].door(mesh, du, dw, dh, s, lod)
    for name, u, zs, ww, hh, major in spec["windows"]:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zs, ww, hh, s, detail, bars=True)

    o = mesh.obj(f"{spec['piece']}_lod{lod}", mats)
    xs, ys, _ = zip(*[v.co[:] for v in o.data.vertices])
    print(f"{spec['piece']}: LOD{lod} x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f}")
    bad = porch_errors(o)
    assert not bad, bad
    if lod == 2:
        reach = max(math.hypot(x, y) for x in (min(xs), max(xs)) for y in (min(ys), max(ys)))
        print(f"{spec['piece']}: LOD2 (collider box) reach {reach:.3f}")
        assert reach <= COTTAGE_REACH, f"collider box corner reach {reach:.3f} > {COTTAGE_REACH}"
    BUILT.append(o.name)
    return o


def porch_errors(o):
    """Every vertex past house_cottage's LOD0 front (x 2.6) stays inside the parking doorstep box."""
    out = [v.co for v in o.data.vertices if v.co.x > COTTAGE_LOD0_X + 1e-4]
    return [f"{o.name}: ({c.x:.3f}, {c.y:.3f}) outside the doorstep box" for c in out
            if c.x > DOORSTEP[0] + 1e-4 or abs(c.y) > DOORSTEP[1] + 1e-4]


def check_spec(spec):
    """The slot contract (module docstring) on the spec's numbers."""
    hd, hw = spec["depth"] / 2, spec["width"] / 2
    assert hd <= PARKING_WALLS[0] and hw <= PARKING_WALLS[1], (hd, hw)
    pu, pw, pd, pe, pp, po, _ = spec["porch"]
    assert pu == 0.0 and pw / 2 + po <= DOORSTEP[1] and hd + pd + spec["porch_front_over"] + 0.035 <= DOORSTEP[0]
    du, dw, dh = spec["door"]
    # _house.Wall.door's step runs 0.4 m out from the wall and the door frame 0.1 m above the door.
    assert du == 0.0 and hd + 0.4 < hd + pd - 0.12, "the door step must end before the porch posts"
    assert dh + 0.1 < pe - po * math.tan(math.radians(pp)) - 0.22, "the porch tie beam must clear the door frame"
    assert -hd < porch_back(spec) < hd, porch_back(spec)
    assert all(hw - abs(cy) >= spec["chimney_size"][1] / 2 + 0.05 for cy in spec["stacks"]), "stacks inside the gables"


def build_piece(spec, generator):
    check_spec(spec)
    lh.build = build
    lh.COTTAGE_BOUNDS = COTTAGE_BOX
    lh.COTTAGE_LOD0_BOUNDS = LOD0_BOUNDS
    BUILT.clear()
    root, lods = lh.build_piece(spec, generator)
    assert [o.name for o in lods] == BUILT, ("_house_longhouse.build_piece did not use this builder", BUILT)
    return root, lods
