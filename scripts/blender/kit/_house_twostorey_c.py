"""House family, cross-wing two-storey variant: the builder behind house_twostorey_c.py (PRD §6 Variety,
house_cluster house 2).

A two-storey stone-and-render farmhouse in the village style (owner-approved refs, assets-source/refs/
house_cluster: ref-1's middle house has a rubble-stone ground floor under a rendered upper storey and a
cross gable to the front). Its silhouette differs from both other designs of the slot: house_twostorey
is a gabled box with a stack outside one gable and a small porch, house_twostorey_b a jettied, hipped
box with a stack on its short ridge. This one is a T: a gabled main range with a two-storey gabled
wing projecting from the middle of its front, whose roof runs back into the main roof's front slope
(a cross ridge from above, a front-facing gable at eye level), a broad stepped stone stack outside
the +y gable only, rubble stone to the first floor under a dressed string course, a pale lime render
above, stone-slate roofs, oak bargeboards on the wing gable and a lean-to oak door hood on brackets.

Slot fit (data/tiles/house_cluster.ron house 2 at (-7.5, 0) yaw 0; house_pad.py DOORSTEPS[2] and its
yard from x 2.5; hex-sim tests/parking.rs "two-storey" and "doorstep 2"): walls inside x ±2.5,
|y| <= 3.6; the wing's front on house_twostorey's front line x = 2.5 with the door at y 0, its step
reaching over the pad's doorstep (x 2.5..3.4, |y| <= 0.8), and the door hood inside that box and
0.3 m short of park-2's parked car (its nose at x 3.8). LOD2, and so the box collider, sits inside
house_twostorey's collider box (read from its GLB: Blender x ±2.8, y -4.25..3.8; scatter::GROUND_BOXES)
with a corner reach under its FOOTPRINTS ground (4.75 m); LOD0 and LOD1 sit inside its LOD0 extents
(x -2.816..3.7, y -4.3..3.8). The box collider fills the two front corners beside the wing, where the
main range stands back 0.6 m: flagstones pave them from the plinth to the yard's edge, so the strip
the box keeps clear of grass and walkers reads as paving.

Pipeline: `_house_longhouse.build_piece` runs unchanged, driven through its module globals as
`_house_jettied` and `_house_slate` do: `build` is this module's builder and the fit bounds are
house_twostorey's (named COTTAGE_* there, so a fit failure prints "house_cottage's footprint"). So the
masks bake, the unbaked-texel fill, the zero-unbaked-triangle assert, budgets.check, the LOD2 box
collider and the export are the family's, and no approved helper is edited. Frame as _house.py:
Blender (x, y, z) exports as runtime (x, z, -y), the pivot is the footprint centre at ground level,
and the front door faces +x.

The wing roof is a closed sweep whose back end lies on the main front slope's plane, each vertex
OVERLAP inside the main slab (house_longhouse's hall-into-wing method), and its eave stands above the
main eave line, so every end vertex is inside the slab and the valley is a clean intersection. The
walls split at the stone/render line on shared vertices, so no T-junction opens a crack."""
import math

import bmesh
from mathutils import Matrix, Vector

import _house_longhouse as lh
from _house import DROP, PLINTH_OUT, PLINTH_TOP, Z, Mesh, Wall, roof_profile
from _house_longhouse import beam, box, plinth, sweep

# house_twostorey's slot (Blender x, y), read from assets/kit/p1/house_twostorey/house_twostorey.glb:
# its collider box (= its LOD2 bounds, scatter::GROUND_BOXES), its LOD0 extents (the porch roof to
# x 3.7, the gable stack's oversail to y -4.3) and its ground reach (scatter::FOOTPRINTS).
TWOSTOREY_BOX = ((-2.8, 2.8), (-4.25, 3.8))
TWOSTOREY_LOD0 = ((-2.816, 3.7), (-4.3, 3.8))
TWOSTOREY_REACH = 4.75
PARKING_WALLS = (2.5, 3.6)  # hex-sim tests/parking.rs "two-storey": walls within |x| <= 2.5, |y| <= 3.6
PAD_STEP = (2.5, 3.4, 0.8)  # house_pad DOORSTEPS[2] in this frame: x 2.5..3.4, |y| <= 0.8
PARK_NOSE = 3.8  # park-2's car (tile x -1.0, yaw 180, 5.4 m long) reaches house-frame x 3.8
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
OVERLAP = 0.08  # how far each wing roof vertex ends inside the main front slope's plane (m, horizontal)
assert DROP == -0.35 and PLINTH_TOP == 0.32, (DROP, PLINTH_TOP)

BUILT = []  # names of the LOD objects this builder made (checked after lh.build_piece returns)


def plan_of(spec):
    """The T plan, counter-clockwise from the main front's -y corner."""
    xf, xb, hw = spec["front"], spec["back"], spec["half_w"]
    wx, wh = spec["wing_front"], spec["wing_half"]
    return [(xf, -hw), (xf, -wh), (wx, -wh), (wx, wh), (xf, wh), (xf, hw), (xb, hw), (xb, -hw)]


def walls_of(spec):
    xf, xb, hw, wx = spec["front"], spec["back"], spec["half_w"], spec["wing_front"]
    return {"F": Wall((1, 0, 0), (xf, 0, 0)), "WF": Wall((1, 0, 0), (wx, 0, 0)), "B": Wall((-1, 0, 0), (xb, 0, 0)),
            "GL": Wall((0, -1, 0), (0, -hw, 0)), "GR": Wall((0, 1, 0), (0, hw, 0))}


def main_roof(spec):
    """(centre x, half depth, eave-line x, eave-line z, pitch) of the main range's roof."""
    xf, xb = spec["front"], spec["back"]
    p = math.radians(spec["pitch"])
    return (xf + xb) / 2, (xf - xb) / 2, xf + spec["overhang"], spec["eave"] - spec["overhang"] * math.tan(p), p


def wing_roof(spec):
    """(eave-line z, pitch) of the wing's roof."""
    wp = math.radians(spec["wing_pitch"])
    return spec["wing_eave"] - spec["wing_overhang"] * math.tan(wp), wp


def wing_ends(spec, pts):
    """Each wing profile vertex's back end (Blender x): on the main front slope's plane at the vertex's
    height, OVERLAP further inside the main slab."""
    xc, hd, xe, zb, p = main_roof(spec)
    w_zb, _ = wing_roof(spec)
    return [xe - (w_zb + z - zb) / math.tan(p) - OVERLAP for _, z in pts]


def band(mesh, plan, z0, z1, out, slot):
    """A string course round a counter-clockwise, axis-aligned plan, `out` proud of the walls: each
    edge's outer face, its top ledge and its underside (nothing behind the walls)."""
    n = len(plan)
    nrm = []
    for i in range(n):
        (ax, ay), (bx, by) = plan[i], plan[(i + 1) % n]
        ln = math.hypot(bx - ax, by - ay)
        nrm.append(((by - ay) / ln, -(bx - ax) / ln))
    outer = [(plan[i][0] + (nrm[i - 1][0] + nrm[i][0]) * out, plan[i][1] + (nrm[i - 1][1] + nrm[i][1]) * out)
             for i in range(n)]
    for i in range(n):
        j = (i + 1) % n
        (ax, ay), (bx, by) = outer[i], outer[j]
        nv = Vector((*nrm[i], 0.0))
        mesh.face([(ax, ay, z0), (bx, by, z0), (bx, by, z1), (ax, ay, z1)], slot, nv)
        mesh.face([(ax, ay, z1), (bx, by, z1), (*plan[j], z1), (*plan[i], z1)], slot, Z)
        mesh.face([(ax, ay, z0), (bx, by, z0), (*plan[j], z0), (*plan[i], z0)], slot, -Z)


def quoins(mesh, spec, s):
    """Dressed corner stones on the rubble storey's convex corners (the main range's four and the
    wing's two front corners), alternately long on each face, each standing 4 cm proud of both faces
    (no faces inside the walls). Per corner group: (corners, (long, short) lengths)."""
    xf, xb, hw, wx, wh = spec["front"], spec["back"], spec["half_w"], spec["wing_front"], spec["wing_half"]
    n_q, qh, step = spec["quoins"]
    groups = ((((xf, hw), (1, 1)), ((xf, -hw), (1, -1)), ((xb, hw), (-1, 1)), ((xb, -hw), (-1, -1))),
              (((wx, wh), (1, 1)), ((wx, -wh), (1, -1))))
    for corners, (lng, sht) in zip(groups, spec["quoin_sizes"]):
        for (cx, cy), (sx, sy) in corners:
            for q in range(n_q):
                a, b = (lng, sht) if q % 2 == 0 else (sht, lng)  # a along y (front or back face), b along x
                c = (cx - sx * (b / 2 - 0.04), cy - sy * (a / 2 - 0.04), PLINTH_TOP + qh / 2 + step * q)
                box(mesh, c, X, Y, Z, (b, a, qh), s["stone"], drop=(-sx * X, -sy * Y))


def end_stack(mesh, spec, s, lod, ridge_z):
    """The broad rubble stack outside the +y gable: a base to `base_top` and a narrower shaft through
    the verge to `stack_above` over the ridge, with an oversail course and clay pots along x. Faces
    buried in the gable wall and the ground are left out. Returns the shaft top."""
    hw, cx = spec["half_w"], spec["stack_x"]
    (bx, by, btop), (sx, sy) = spec["stack_base"], spec["stack_shaft"]
    y0 = hw - 0.05  # 5 cm into the gable wall
    box(mesh, (cx, y0 + by / 2, (DROP + btop) / 2), X, Y, Z, (bx, by, btop - DROP), s["rubble"], drop=(-Z, -Y))
    top = ridge_z + spec["stack_above"]
    sc = y0 + 0.02 + sy / 2
    box(mesh, (cx, sc, (btop + top) / 2), X, Y, Z, (sx, sy, top - btop), s["rubble"], drop=(-Z,))
    if lod == 2:
        return top
    box(mesh, (cx, sc, top + 0.06), X, Y, Z, (sx + 0.14, sy + 0.14, 0.12), s["rubble"], drop=(-Z,))
    for px in ((-0.24, 0.24) if lod == 0 else (0.0,)):
        res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=6 if lod == 0 else 5, radius1=0.12,
                                    radius2=0.1, depth=0.4, matrix=Matrix.Translation((cx + px, sc, top + 0.32)))
        for f in list({f for v in res["verts"] for f in v.link_faces}):
            f.normal_update()
            if f.normal.z < -0.9:
                mesh.bm.faces.remove(f)
            else:
                f.material_index = mesh.idx[s["pots"]]
    return top


def door_hood(mesh, spec, s, lod):
    """A lean-to stone-slate hood over the wing door, its slab tilted down and out from the wall, on two
    oak brackets (LOD0)."""
    wx = spec["wing_front"]
    z_top, depth, half, pitch_deg, thick = spec["hood"]
    a = math.radians(pitch_deg)
    ax = Vector((math.cos(a), 0.0, -math.sin(a)))  # down the slope, away from the wall
    az = ax.cross(Y)
    ln = depth / math.cos(a) + 0.05  # 5 cm of it inside the wall
    c = Vector((wx, 0.0, z_top)) + ax * (ln / 2 - 0.05) - az * (thick / 2)
    box(mesh, c, ax, Y, az, (ln, 2 * half, thick), s["roof"], drop=(-ax,))
    if lod == 0:
        for sg in (-1, 1):
            y = sg * (half - 0.09)
            a0 = Vector((wx - 0.02, y, z_top - 0.7))
            a1 = Vector((wx + depth * 0.72, y, z_top - depth * 0.72 * math.tan(a) - thick / math.cos(a) + 0.02))
            d = (a1 - a0).normalized()
            box(mesh, (a0 + a1) / 2, d, Y, d.cross(Y), ((a1 - a0).length, 0.08, 0.08), s["timber"])


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    xf, xb, hw, eave = spec["front"], spec["back"], spec["half_w"], spec["eave"]
    wx, wh, weave = spec["wing_front"], spec["wing_half"], spec["wing_eave"]
    zs = spec["stone_top"]  # rubble stone below, render above
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    xc, hd, xe, zb, p = main_roof(spec)
    w_zb, wp = wing_roof(spec)
    apex = eave + hd * math.tan(p) - 0.05  # inside the roof slab
    w_apex = weave + wh * math.tan(wp) - 0.05

    # Main range: the front either side of the wing, the back and both gables, split at the stone line.
    for y0, y1 in ((-hw, -wh), (wh, hw)):
        mesh.face([(xf, y0, z0), (xf, y1, z0), (xf, y1, zs), (xf, y0, zs)], s["rubble"], X)
        mesh.face([(xf, y0, zs), (xf, y1, zs), (xf, y1, eave), (xf, y0, eave)], s["walls"], X)
    mesh.face([(xb, -hw, z0), (xb, hw, z0), (xb, hw, zs), (xb, -hw, zs)], s["rubble"], -X)
    mesh.face([(xb, -hw, zs), (xb, hw, zs), (xb, hw, eave), (xb, -hw, eave)], s["walls"], -X)
    for y, n in ((hw, Y), (-hw, -Y)):
        mesh.face([(xf, y, z0), (xb, y, z0), (xb, y, zs), (xf, y, zs)], s["rubble"], n)
        mesh.face([(xf, y, zs), (xb, y, zs), (xb, y, eave), (xc, y, apex), (xf, y, eave)], s["walls"], n)
    # The wing: its front gable, and its sides, which run on over the main roof's front slope (3 cm
    # inside its slab) back to where the wing eave meets it.
    mesh.face([(wx, -wh, z0), (wx, wh, z0), (wx, wh, zs), (wx, -wh, zs)], s["rubble"], X)
    mesh.face([(wx, -wh, zs), (wx, wh, zs), (wx, wh, weave), (wx, 0.0, w_apex), (wx, -wh, weave)], s["walls"], X)
    x_meet = xe - (weave - zb) / math.tan(p) - 0.03
    for y, n in ((wh, Y), (-wh, -Y)):
        mesh.face([(xf, y, z0), (wx, y, z0), (wx, y, zs), (xf, y, zs)], s["rubble"], n)
        mesh.face([(xf, y, zs), (wx, y, zs), (wx, y, weave), (x_meet, y, weave), (xf, y, eave - 0.03)], s["walls"], n)
    plan = plan_of(spec)
    if lod == 0:
        quoins(mesh, spec, s)
    if lod < 2:
        plinth(mesh, plan, s["stone"])
        b0, b1, bout = spec["band"]
        band(mesh, plan, b0, b1, bout, s["stone"])
        # Flagstones in the two front corners beside the wing, from the plinth to the yard's edge (x 2.5),
        # 2 cm over house_pad's gravel; the faces against the plinths are left out.
        az = spec["apron_top"]
        for sg in (-1, 1):
            ya, yb = sg * (wh + PLINTH_OUT), sg * (hw + PLINTH_OUT)
            box(mesh, ((xf + PLINTH_OUT + wx) / 2, (ya + yb) / 2, (DROP + az) / 2), X, Y, Z,
                (wx - xf - PLINTH_OUT, abs(yb - ya), az - DROP), s["stone"], drop=(-X, -Z, -sg * Y))

    # Roofs: the main range swept along y between its verges; the wing swept along +x from the main
    # front slope to its own front verge (profile x maps to -y, as house_longhouse's cross wing).
    thick, kick = spec["roof_thick"], spec["kick"]
    pts, rise = roof_profile(hd, p, spec["overhang"], spec["courses"] if lod == 0 else 0, kick, thick)
    gov = spec["gable_overhang"]
    sweep(mesh, s["roof"], pts, lambda x, z, t: (xc + x, t, zb + z), -hw - gov, hw + gov, (True, True))
    ridge_z = zb + rise
    w_over, w_gov = spec["wing_overhang"], spec["wing_gable_overhang"]
    wpts, w_rise = roof_profile(wh, wp, w_over, spec["wing_courses"] if lod == 0 else 0, kick, thick)
    ends = wing_ends(spec, wpts)
    assert min(w_zb + z for _, z in wpts) > zb + 0.05, "the wing eave must stand above the main eave line"
    assert xc < min(ends) and max(ends) < xe, ("wing roof ends outside the main front slope", ends)
    sweep(mesh, s["roof"], wpts, lambda x, z, t: (t, -x, w_zb + z), ends, wx + w_gov, (False, True))
    assert w_zb + w_rise < ridge_z - 0.2, "the wing ridge must sit below the main ridge"

    end_stack(mesh, spec, s, lod, ridge_z)

    W = walls_of(spec)
    du, dw, dh = spec["door"]
    W["WF"].door(mesh, du, dw, dh, s, lod)
    for name, u, zw, ww, hh, major in spec["windows"]:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zw, ww, hh, s, detail, bars=True)
    if lod < 2:
        door_hood(mesh, spec, s, lod)
        # Oak bargeboards on the wing's front verge, over the course ends, starting `inset` up the
        # slope so their lower corners stay inside the eave line.
        inset = 0.11 * math.sin(wp) + 0.01
        run = wh + w_over
        fw = Wall((1, 0, 0), (wx, 0, 0))
        for sg in (-1, 1):
            beam(mesh, fw, sg * (run - inset), w_zb - 0.05 + inset * math.tan(wp), 0.0, w_zb + w_rise - 0.05, 0.22,
                 0.05, s["timber"], out=w_gov + 0.002)

    o = mesh.obj(f"{spec['piece']}_lod{lod}", mats)
    xs, ys, zz = zip(*[v.co[:] for v in o.data.vertices])
    reach = max(math.hypot(x, y) for x in (min(xs), max(xs)) for y in (min(ys), max(ys)))
    print(f"{spec['piece']}: LOD{lod} x {min(xs):.3f}..{max(xs):.3f} y {min(ys):.3f}..{max(ys):.3f}"
          f" z {min(zz):.3f}..{max(zz):.3f} corner reach {reach:.3f}")
    if lod == 2:
        assert reach <= TWOSTOREY_REACH, f"collider box corner reach {reach:.3f} > {TWOSTOREY_REACH}"
    BUILT.append(o.name)
    return o


def check_spec(spec):
    """The slot contract (module docstring) on the spec's numbers."""
    xf, xb, hw, wx, wh = spec["front"], spec["back"], spec["half_w"], spec["wing_front"], spec["wing_half"]
    assert max(wx, -xb) <= PARKING_WALLS[0] and hw <= PARKING_WALLS[1], (xb, wx, hw)
    assert xb < xf < wx and 0 < wh < hw, "the wing projects from the middle of the main range's front"
    du, dw, _ = spec["door"]
    # _house.Wall.door's step runs 0.4 m out from the wall and 0.2 m past the door frame each side.
    assert du == 0.0 and dw / 2 + 0.2 <= PAD_STEP[2], spec["door"]
    assert wx + 0.4 >= PAD_STEP[0] + 0.03, f"door step ends at x {wx + 0.4} before the pad step at {PAD_STEP[0]}"
    z_top, depth, half, _, _ = spec["hood"]
    assert wx + depth + 0.05 <= min(PAD_STEP[1], PARK_NOSE - 0.3) and half <= PAD_STEP[2], spec["hood"]
    assert z_top < spec["band"][0], "the door hood sits under the string course"
    (bx, by, _), (sx, sy) = spec["stack_base"], spec["stack_shaft"]
    assert sx < bx and sy + 0.02 <= by, "the shaft stands on the base"
    assert hw - 0.05 + 0.02 + sy + 0.07 <= TWOSTOREY_LOD0[1][1], "the oversail inside house_twostorey's LOD0"


def build_piece(spec, generator):
    check_spec(spec)
    lh.build = build
    lh.COTTAGE_BOUNDS = TWOSTOREY_BOX
    lh.COTTAGE_LOD0_BOUNDS = TWOSTOREY_LOD0
    BUILT.clear()
    root, lods = lh.build_piece(spec, generator)
    assert [o.name for o in lods] == BUILT, ("_house_longhouse.build_piece did not use this builder", BUILT)
    return root, lods
