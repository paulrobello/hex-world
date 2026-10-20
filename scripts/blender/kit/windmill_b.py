"""windmill_b: the windmill slot's hero variant (PRD §5.4 row 2b, §6 Variety), a white smock mill: an
octagonal weatherboarded smock on a buff brick base, a railed stage round it at the base top, a domed
ogee cap with an iron finial, a still six-bladed fantail and four double-shuttered patent sails on a
rotor that the runtime turns. Shorter and stouter than `windmill` (hub 10.7 m against 12.0 m, sails
6.2 m against 8.4 m). Owner-approved references: assets-source/refs/windmill (ref-1..3); the owner's
answer (approval.json, A17) wins where they disagree: white, a flat tile, sails turning, fantail
still. Scripted route.

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the tower's centre at ground
level (z = 0); the sails face +x (yaw 0), as `windmill`'s do, so the tile's yaw 120 turns them toward
the play camera. The base runs from DROP (below the meadow cap's deepest relief, -0.25 m).

Nodes: `windmill_b_lod{0,1,2}` (base, stage, smock, cap, fantail: still), `windmill_b_rotor_lod{0,1,2}`
(canister, stocks and sails: turned by the runtime) and `windmill_b_collider` (the base and smock
only). The rotor contract is `windmill`'s: each rotor node sits on the hub with its local +X along the
windshaft (tilted THETA up from +x), its mesh is authored round that local X axis, and the gate counts
each LOD as its body plus its rotor (LOD0 <= 1,500 as a prop, LOD1 <= 50 %, LOD2 <= 15 %).

`windmill.py` runs its build at import, so it cannot be imported; its bmesh accumulator and its bake
sequence are the monolithic steps copied here (pitfalls.md, Variants). The unbaked-texel fill and the
centroid check come from `_house.py`.

Look: every surface is a colour-only slot plus the piece's `masks` (an octagonal smock and a curved
cap cannot take a world-space tileable set, and a unique 256 px/m atlas would not fit a prop's
budget). The weatherboard courses, stage rail, sail bars and shutters are geometry. masks: R snow 1
(0 on glass), G porosity per surface, B night glow on the glass, A baked AO, every part and LOD on its
own islands of one 1024² atlas, with sub-texel strips filled from their slot (`_house.fill_unbaked`).
The body and the rotor are baked as separate objects (bake_to hides every other mesh), so no shadow of
the smock turns with the sails."""
import json
import math
import os
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

from _house import centroid_samples, fill_unbaked  # noqa: E402
from lib import bake, budgets, collider, export, geom, materials, scene, uv  # noqa: E402

P = "windmill_b"
DROP = -0.35
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
SHARP_DEG = 40.0  # below the octagons' 45° corners, so the smock and base read faceted
UV_MARGIN = 0.012
BAKE_MARGIN = 4

# Octagons have a flat facet toward +x (the doors, under the sails); radii are corner radii.
OCT = 8
# Brick base, battered slightly.
B0, B1, BASE_TOP = 2.75, 2.62, 2.8
# Smock: six weatherboard courses, each board's foot LAP proud of the one below.
S0, S1, SMOCK_TOP = 2.32, 1.62, 9.6
COURSES, LAP, EPS = 6, 0.045, 0.02
# Stage: a railed octagonal deck round the smock foot, at the base top.
STAGE_R, DECK_T, RAIL_H = 3.7, 0.14, 0.95
RAIL_TOP = BASE_TOP + RAIL_H
# Cap: a curb, a boarded petticoat and a domed ogee with an iron finial.
CAP_R = 2.0
CURB_Z = SMOCK_TOP + 0.15
PETTI_TOP = CURB_Z + 0.4
DOME = [(CAP_R, PETTI_TOP), (CAP_R - 0.03, PETTI_TOP + 0.35), (1.80, PETTI_TOP + 0.75),
        (1.35, PETTI_TOP + 1.10), (0.75, PETTI_TOP + 1.35), (0.32, PETTI_TOP + 1.55), (0.12, PETTI_TOP + 1.85)]
DOME_TOP = DOME[-1][1]
# Windshaft: tilted THETA up, leaving the dome front at SHAFT_Z; the hub sits 0.7 m out along it.
THETA = math.radians(10.0)
AXLE = Vector((math.cos(THETA), 0.0, math.sin(THETA)))
SHAFT_Z = PETTI_TOP + 0.45
HUB = Vector((1.92, 0.0, SHAFT_Z)) + AXLE * 0.7
# Double patent sails: shutters on both sides of the whip, SAIL_W each side, from SAIL_R0 to SAIL_R1.
SAIL_R0, SAIL_R1, SAIL_W = 1.4, 6.2, 0.9
# Fantail (still, owner A17): six blades on an axle along y, on two raking legs behind the cap.
FAN_C = Vector((-3.15, 0.0, PETTI_TOP + 1.0))
FAN_R0, FAN_R1 = 0.3, 1.05
# Clearances the swept sails keep (asserted over a full turn).
SAIL_CLEAR_Z = 0.5   # above the stage rail
SAIL_CLEAR_R = 0.15  # off the smock and cap

# Blender colours are linear and equal the manifest colours (every slot is colour-only).
# (colour, roughness, (snow, porosity, night))
SLOTS = {
    "smock": ((0.66, 0.65, 0.61), 0.85, (1.0, 0.5, 0.0)),
    "brick": ((0.44, 0.35, 0.25), 0.9, (1.0, 0.6, 0.0)),
    "cap": ((0.64, 0.66, 0.64), 0.75, (1.0, 0.45, 0.0)),
    "timber": ((0.16, 0.12, 0.085), 0.85, (1.0, 0.6, 0.0)),
    "sail": ((0.66, 0.65, 0.60), 0.8, (1.0, 0.5, 0.0)),
    "iron": ((0.05, 0.05, 0.055), 0.55, (1.0, 0.15, 0.0)),
    "door": ((0.10, 0.17, 0.12), 0.7, (1.0, 0.5, 0.0)),
    "glass": ((0.20, 0.16, 0.10), 0.2, (0.0, 0.05, 1.0)),
}
NAMES = [f"{P}_{s}" for s in SLOTS]

# Windows: (part, facet azimuth deg, sill z, width, height). The base door faces +x under the stage,
# the stage door above it on the smock.
WINDOWS = [("base", 180.0, 1.0, 0.5, 0.7), ("smock", 90.0, 4.6, 0.5, 0.7), ("smock", 270.0, 6.0, 0.5, 0.7),
           ("smock", 180.0, 5.3, 0.5, 0.7), ("smock", 0.0, 7.4, 0.45, 0.65)]
DOOR_W, DOOR_H = 0.85, 1.9
STAGE_DOOR_W, STAGE_DOOR_H = 0.8, 1.75

LOD = [
    {"courses": True, "cap_seg": 12, "dome": DOME, "finial": 6, "petticoat": True, "stage": 2, "frames": 2,
     "door": 2, "fan": 2, "canister": 8, "bars": 6, "shutters": True},
    {"courses": False, "cap_seg": 8, "dome": [DOME[0], DOME[2], DOME[4], DOME[6]], "finial": 0,
     "petticoat": True, "stage": 1, "frames": 0, "door": 0, "fan": 1, "canister": 6, "bars": 3,
     "shutters": False},
    {"courses": False, "cap_seg": 8, "dome": [DOME[2], DOME[6]], "finial": 0, "petticoat": False,
     "stage": 0, "frames": -1, "door": -1, "fan": 0, "canister": 0, "bars": 0, "shutters": False},
]


def base_r(z):
    return B0 + (B1 - B0) * (z - DROP) / (BASE_TOP - DROP)


def smock_r(z):
    return S0 + (S1 - S0) * (z - BASE_TOP) / (SMOCK_TOP - BASE_TOP)


def cap_r(z):
    """The cap's radius at height z (curb to dome top), 0 above it."""
    if z < SMOCK_TOP or z > DOME_TOP:
        return 0.0
    if z < PETTI_TOP:
        return CAP_R
    for (r0, z0), (r1, z1) in zip(DOME, DOME[1:]):
        if z <= z1:
            return r0 + (r1 - r0) * (z - z0) / (z1 - z0)
    return 0.0


class Mesh:
    """bmesh accumulator with per-face slot indices (windmill.py's). Parts never share vertices
    unless built together, so no global weld runs (double-sided boards keep their own back sheets)."""

    def __init__(self):
        self.bm = bmesh.new()
        self.idx = {s: i for i, s in enumerate(SLOTS)}

    def face(self, pts, slot, expect=None):
        f = self.bm.faces.new([v if isinstance(v, bmesh.types.BMVert) else self.bm.verts.new(v) for v in pts])
        f.material_index = self.idx[slot]
        if expect is not None:
            f.normal_update()
            if f.normal.dot(expect) < 0:
                f.normal_flip()
        return f

    def board(self, pts, n, slot, thick=0.02):
        """A double-sided board: the front quad on pts facing n and a back quad `thick` behind it on
        its own vertices (a colour-only slot is culled single-sided)."""
        n = Vector(n).normalized()
        self.face([Vector(p) for p in pts], slot, n)
        self.face([Vector(p) - n * thick for p in pts], slot, -n)

    def prism(self, a, b, half, up, slot, half_b=None, caps=(False, False)):
        """A rectangular bar from a to b, half sizes (width, height) at a and half_b at b, height along
        `up` (made perpendicular to the bar)."""
        a, b = Vector(a), Vector(b)
        t = (b - a).normalized()
        n1 = (Vector(up) - t * Vector(up).dot(t)).normalized()
        n2 = t.cross(n1)
        hb = half_b or half

        def ring(c, h):
            return [self.bm.verts.new(c + n2 * sw * h[0] + n1 * sh * h[1])
                    for sw, sh in ((1, 1), (-1, 1), (-1, -1), (1, -1))]

        ra, rb = ring(a, half), ring(b, hb)
        for i in range(4):
            j = (i + 1) % 4
            mid = (ra[i].co + ra[j].co + rb[i].co + rb[j].co) / 4
            axis = a + t * (mid - a).dot(t)
            self.face([ra[i], ra[j], rb[j], rb[i]], slot, mid - axis)
        if caps[0]:
            self.face(ra, slot, -t)
        if caps[1]:
            self.face(rb, slot, t)

    def lathe(self, profile, seg, slots, offset=0.5, axis="z", centre=Vector()):
        """Surface of revolution: profile [(radius, h), ...] bottom to top, bands slotted by slots[i].
        axis "z" revolves about +Z; "x" about +X (h along x). Bands face outward."""
        rings = []
        for r, h in profile:
            ring = []
            for k in range(seg):
                a = 2 * math.pi * (k + offset) / seg
                if axis == "z":
                    p = Vector((r * math.cos(a), r * math.sin(a), h))
                else:
                    p = Vector((h, r * math.cos(a), r * math.sin(a)))
                ring.append(self.bm.verts.new(centre + p))
            rings.append(ring)
        for i in range(len(rings) - 1):
            for k in range(seg):
                j = (k + 1) % seg
                q = [rings[i][k], rings[i][j], rings[i + 1][j], rings[i + 1][k]]
                mid = sum((v.co for v in q), Vector()) / 4 - centre
                out = Vector((mid.x, mid.y, 0.0)) if axis == "z" else Vector((0.0, mid.y, mid.z))
                self.face(q, slots[i], out)
        return rings

    def obj(self, name, location=(0.0, 0.0, 0.0), rotation=(0.0, 0.0, 0.0)):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        for f in self.bm.faces:
            f.smooth = True
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        mesh.set_sharp_from_angle(angle=math.radians(SHARP_DEG))
        o = scene.link(bpy.data.objects.new(name, mesh))
        o.location = location
        o.rotation_euler = rotation
        for s in SLOTS:
            o.data.materials.append(bpy.data.materials[f"{P}_{s}"])
        return o


def oct_pt(r, k, z):
    """Corner k of an octagon of corner radius r at height z (corners at 22.5° + 45°·k)."""
    a = 2 * math.pi * (k + 0.5) / OCT
    return Vector((r * math.cos(a), r * math.sin(a), z))


def facet_frame(part, az_deg, z, proud=0.0):
    """Point on the octagonal facet centred at azimuth az_deg of the base or the smock at height z, with
    that facet's tangent t, up-slope u and outward normal n."""
    rf, z0, z1 = (base_r, DROP, BASE_TOP) if part == "base" else (smock_r, BASE_TOP, SMOCK_TOP)
    c8 = math.cos(math.pi / OCT)
    alpha = math.atan((rf(z0) - rf(z1)) * c8 / (z1 - z0))
    a = math.radians(az_deg)
    radial = Vector((math.cos(a), math.sin(a), 0.0))
    n = (radial * math.cos(alpha) + Vector((0, 0, 1)) * math.sin(alpha)).normalized()
    t = Vector((-math.sin(a), math.cos(a), 0.0))
    u = n.cross(t).normalized()
    lap = LAP + 0.01 if part == "smock" else 0.0
    c = radial * rf(z) * c8 + Vector((0, 0, z)) + n * (proud + lap)
    return c, t, u, n


def window(m, part, az, z0, w, h, detail):
    """Glass on a facet with a picture frame proud of it: front ring, outer sides and an inner reveal
    (detail 2), the front ring only (1), or the glass alone (0)."""
    f, p, g = 0.08, 0.07, 0.012
    c, t, u, n = facet_frame(part, az, z0 + h / 2)

    def q(du, dv, out):
        return c + t * du + u * dv + n * out

    hw, hh = w / 2, h / 2
    m.face([q(-hw, -hh, g), q(hw, -hh, g), q(hw, hh, g), q(-hw, hh, g)], "glass", n)
    if detail <= 0:
        return
    outer = [(-hw - f, -hh - f), (hw + f, -hh - f), (hw + f, hh + f), (-hw - f, hh + f)]
    inner = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
    for i in range(4):
        j = (i + 1) % 4
        (ou0, ov0), (ou1, ov1) = outer[i], outer[j]
        (iu0, iv0), (iu1, iv1) = inner[i], inner[j]
        m.face([q(ou0, ov0, p), q(ou1, ov1, p), q(iu1, iv1, p), q(iu0, iv0, p)], "timber", n)
        if detail == 2:
            side = (t * ((ou0 + ou1) / 2) + u * ((ov0 + ov1) / 2)).normalized()
            m.face([q(ou0, ov0, -0.01), q(ou1, ov1, -0.01), q(ou1, ov1, p), q(ou0, ov0, p)], "timber", side)
            m.face([q(iu0, iv0, p), q(iu1, iv1, p), q(iu1, iv1, g), q(iu0, iv0, g)], "timber", -side)


def doors(m, detail):
    """The base door on the +x facet (jambs, lintel and a stone step at LOD0), and the stage door on the
    smock above it, opening onto the stage."""
    c, t, u, n = facet_frame("base", 0.0, 0.0)
    hw = DOOR_W / 2

    def q(du, z, out):
        return c + t * du + u * (z / u.z) + n * out

    m.face([q(-hw, 0.0, 0.02), q(hw, 0.0, 0.02), q(hw, DOOR_H, 0.02), q(-hw, DOOR_H, 0.02)], "door", n)
    sc, st, su, sn = facet_frame("smock", 0.0, BASE_TOP)
    shw = STAGE_DOOR_W / 2

    def sq(du, dz, out):
        return sc + st * du + su * (dz / su.z) + sn * out

    m.face([sq(-shw, 0.0, 0.0), sq(shw, 0.0, 0.0), sq(shw, STAGE_DOOR_H, 0.0), sq(-shw, STAGE_DOOR_H, 0.0)],
           "door", sn)
    if detail <= 0:
        return
    jamb = (0.06, 0.05)
    lintel = q(0.0, DOOR_H + 0.06, 0.05)
    m.prism(lintel - t * (hw + 0.12), lintel + t * (hw + 0.12), jamb, n, "timber", caps=(True, True))
    slintel = sq(0.0, STAGE_DOOR_H + 0.06, 0.04)
    m.prism(slintel - st * (shw + 0.1), slintel + st * (shw + 0.1), jamb, sn, "timber", caps=(True, True))
    if detail == 2:
        for s in (-1, 1):
            m.prism(q(s * (hw + 0.06), DROP, 0.05), q(s * (hw + 0.06), DOOR_H, 0.05), jamb, n, "timber")
        step = c + n * 0.28 + Vector((0, 0, (DROP + 0.12) / 2))
        m.prism(step - t * (hw + 0.2), step + t * (hw + 0.2), (0.28, (0.12 - DROP) / 2), Vector((0, 0, 1)),
                "brick", caps=(True, True))


def smock(m, courses):
    """The weatherboarded smock: each course's foot stands LAP proud of the course below and leans
    back to flush at its head, so every course casts a dark lap line (a down-facing strip) under it."""
    if not courses:
        m.lathe([(smock_r(BASE_TOP), BASE_TOP), (smock_r(SMOCK_TOP), SMOCK_TOP)], OCT, ["smock"])
        return
    zs = np.linspace(BASE_TOP, SMOCK_TOP, COURSES + 1)
    profile = [(smock_r(zs[0]) + LAP, zs[0])]
    for z in zs[1:-1]:
        profile += [(smock_r(z - EPS), z - EPS), (smock_r(z) + LAP, z)]
    profile.append((smock_r(SMOCK_TOP), SMOCK_TOP))
    m.lathe(profile, OCT, ["smock"] * (len(profile) - 1))


def stage(m, detail):
    """The octagonal deck at the base top (top, underside and edge), its rail (corner posts and a top
    rail at LOD0, solid balustrade boards at LOD1) and corner struts back to the base (LOD0); a thin
    deck at LOD2."""
    top, bot = BASE_TOP, BASE_TOP - DECK_T
    r_in_top, r_in_bot = smock_r(BASE_TOP) - 0.1, B1 - 0.1
    for k in range(OCT):
        j = (k + 1) % OCT
        m.face([oct_pt(r_in_top, k, top), oct_pt(STAGE_R, k, top), oct_pt(STAGE_R, j, top),
                oct_pt(r_in_top, j, top)], "timber", Vector((0, 0, 1)))
        m.face([oct_pt(r_in_bot, k, bot), oct_pt(STAGE_R, k, bot), oct_pt(STAGE_R, j, bot),
                oct_pt(r_in_bot, j, bot)], "timber", Vector((0, 0, -1)))
        if detail >= 1:
            mid = (oct_pt(STAGE_R, k, 0) + oct_pt(STAGE_R, j, 0)) / 2
            m.face([oct_pt(STAGE_R, k, bot), oct_pt(STAGE_R, j, bot), oct_pt(STAGE_R, j, top),
                    oct_pt(STAGE_R, k, top)], "timber", mid)
    if detail == 1:
        for k in range(OCT):
            j = (k + 1) % OCT
            a, b = oct_pt(STAGE_R - 0.08, k, top), oct_pt(STAGE_R - 0.08, j, top)
            mid = (a + b) / 2
            m.board([a, b, b + Vector((0, 0, RAIL_H)), a + Vector((0, 0, RAIL_H))],
                    Vector((mid.x, mid.y, 0.0)), "timber", 0.04)
    if detail == 2:
        rp = STAGE_R - 0.08
        for k in range(OCT):
            j = (k + 1) % OCT
            radial = oct_pt(1.0, k, 0.0)
            m.prism(oct_pt(rp, k, top), oct_pt(rp, k, RAIL_TOP), (0.045, 0.045), radial, "timber")
            m.prism(oct_pt(rp, k, RAIL_TOP - 0.03), oct_pt(rp, j, RAIL_TOP - 0.03), (0.035, 0.035),
                    Vector((0, 0, 1)), "timber")
            # A strut from the base's corner up to the deck's underside, near its outer edge.
            m.prism(oct_pt(base_r(1.9) - 0.02, k, 1.9), oct_pt(STAGE_R - 0.35, k, bot + 0.01), (0.05, 0.05),
                    Vector((0, 0, 1)), "timber")


def cap(m, lod):
    seg = lod["cap_seg"]
    if lod["petticoat"]:
        m.lathe([(S1 - 0.12, SMOCK_TOP - 0.08), (CAP_R + 0.05, CURB_Z)], seg, ["timber"])
        m.lathe([(CAP_R, CURB_Z), (CAP_R, PETTI_TOP)], seg, ["cap"])
        dome = lod["dome"]
    else:
        dome = [(CAP_R, SMOCK_TOP)] + lod["dome"]
    rings = m.lathe(dome, seg, ["cap"] * (len(dome) - 1))
    m.face([Vector(v.co) for v in rings[-1]], "cap", Vector((0, 0, 1)))
    if lod["finial"]:
        f = lod["finial"]
        z = DOME_TOP - 0.02
        m.lathe([(0.07, z), (0.19, z + 0.14), (0.17, z + 0.3), (0.04, z + 0.42)], f, ["iron"] * 3)
        m.prism((0, 0, z + 0.4), (0, 0, z + 0.62), (0.025, 0.025), (1, 0, 0), "iron", caps=(False, True))


def fantail(m, detail):
    """The still fantail (owner A17): six blades in the XZ plane on an axle along y, on two raking
    legs from the back of the cap."""
    c = FAN_C
    for s in (-1, 1):
        m.prism((-(CAP_R - 0.1), s * 0.75, PETTI_TOP + 0.05), (c.x, s * 0.36, c.z), (0.06, 0.07), (1, 0, 1),
                "timber")
    if detail == 0:
        ring = [c + Vector((FAN_R1 * math.cos(a), 0.0, FAN_R1 * math.sin(a)))
                for a in (2 * math.pi * k / 6 for k in range(6))]
        m.board([p + Vector((0, 0.01, 0)) for p in ring], (0, 1, 0), "sail")
        return
    m.prism(c - Vector((0, 0.42, 0)), c + Vector((0, 0.42, 0)), (0.05, 0.05), (0, 0, 1), "timber",
            caps=(True, True))
    if detail == 2:
        m.prism(c - Vector((0, 0.12, 0)), c + Vector((0, 0.12, 0)), (0.15, 0.15), (0, 0, 1), "iron",
                caps=(True, True))
    for k in range(6):
        a = 2 * math.pi * (k + 0.5) / 6
        d = Vector((math.cos(a), 0.0, math.sin(a)))
        e = Vector((-math.sin(a), 0.0, math.cos(a)))
        w0, w1 = 0.1, 0.26
        pts = [c + d * FAN_R0 - e * w0, c + d * FAN_R1 - e * w1, c + d * FAN_R1 + e * w1, c + d * FAN_R0 + e * w0]
        m.board([p + Vector((0, 0.01, 0)) for p in pts], (0, 1, 0), "sail")


def body(i):
    lod = LOD[i]
    m = Mesh()
    m.lathe([(B0, DROP), (B1, BASE_TOP)], OCT, ["brick"])
    smock(m, lod["courses"])
    stage(m, lod["stage"])
    cap(m, lod)
    if lod["frames"] >= 0:
        for part, az, z0, w, h in WINDOWS:
            window(m, part, az, z0, w, h, lod["frames"])
        doors(m, lod["door"])
    fantail(m, lod["fan"])
    return m.obj(f"{P}_lod{i}")


STOCK_DX = (0.05, -0.17)  # local x of the two stocks' centres (they cross at the canister)
STOCK_HALF = (0.09, 0.12)  # half width, half depth (along the windshaft)


def sail(m, k, lod):
    """Sail k in the local YZ plane at 45° + 90°·k: a double patent sail, its lattice on the front face
    of its stock with a hemlath along each edge, cross bars, and shutters in every bay on both sides of
    the whip (LOD0), one board each side (LOD1), or one board (LOD2)."""
    psi = math.radians(45.0 + 90.0 * k)
    d = Vector((0.0, math.cos(psi), math.sin(psi)))
    e = Vector((0.0, -math.sin(psi), math.cos(psi)))
    x = Vector((1.0, 0.0, 0.0))
    face = STOCK_DX[k % 2] + STOCK_HALF[1]

    def at(r, w, dx):
        return d * r + e * w + x * (face + dx)

    if lod["bars"] == 0:
        m.board([at(SAIL_R0, -SAIL_W, 0.03), at(SAIL_R1, -SAIL_W, 0.03),
                 at(SAIL_R1, SAIL_W, 0.03), at(SAIL_R0, SAIL_W, 0.03)], x, "sail", 0.04)
        return
    for w in (-SAIL_W, SAIL_W):
        m.prism(at(SAIL_R0 - 0.1, w, 0.0), at(SAIL_R1 + 0.05, w, 0.0), (0.04, 0.05), x, "timber")
    rs = np.linspace(SAIL_R0, SAIL_R1, lod["bars"])
    for r in rs:
        m.prism(at(r, -SAIL_W - 0.06, 0.0), at(r, SAIL_W + 0.06, 0.0), (0.035, 0.04), x, "timber")
    gap, whip = 0.05, 0.13
    if lod["shutters"]:
        for r0, r1 in zip(rs[:-1], rs[1:]):
            for w0, w1 in ((-SAIL_W + gap, -whip), (whip, SAIL_W - gap)):
                m.board([at(r0 + gap, w0, 0.03), at(r1 - gap, w0, 0.03),
                         at(r1 - gap, w1, 0.03), at(r0 + gap, w1, 0.03)], x, "sail")
    else:
        for w0, w1 in ((-SAIL_W + gap, -whip), (whip, SAIL_W - gap)):
            m.board([at(SAIL_R0 + 0.1, w0, 0.03), at(SAIL_R1 - 0.1, w0, 0.03),
                     at(SAIL_R1 - 0.1, w1, 0.03), at(SAIL_R0 + 0.1, w1, 0.03)], x, "sail")


def rotor(i):
    """The turning part, in the rotor node's local frame: origin on the hub, windshaft along +X."""
    lod = LOD[i]
    m = Mesh()
    if lod["canister"]:
        rings = m.lathe([(0.18, -0.7), (0.27, -0.3), (0.27, 0.25)], lod["canister"], ["iron", "iron"], axis="x")
        m.face([Vector(v.co) for v in rings[-1]], "iron", Vector((1, 0, 0)))
    for k in range(2):
        psi = math.radians(45.0 + 90.0 * k)
        d = Vector((0.0, math.cos(psi), math.sin(psi)))
        dx = Vector((STOCK_DX[k], 0.0, 0.0))
        m.prism(dx - d * SAIL_R1, dx + d * SAIL_R1, STOCK_HALF, (1, 0, 0), "timber", caps=(i < 2, i < 2))
    for k in range(4):
        sail(m, k, lod)
    o = m.obj(f"{P}_rotor_lod{i}", location=HUB, rotation=(0.0, -THETA, 0.0))
    o["kind"] = "rotor"
    co = np.array([v.co[:] for v in o.data.vertices])
    centre = co[:, 1:].mean(axis=0)
    assert np.abs(centre).max() < 0.01, f"rotor lod{i} off its axle by {centre}"
    return o


def sail_clearance(rotors):
    """Sweep every rotor LOD's vertices through a full turn about local X (5° steps) and return the
    lowest point and the smallest radial gap to the smock or cap at the same height (Blender frame).
    The gap counts only the stocks and sails beyond 1 m of the axle: the canister enters the cap."""
    bpy.context.view_layer.update()
    low, gap = 1e9, 1e9
    for o in rotors:
        co = np.array([v.co[:] for v in o.data.vertices])
        far = np.hypot(co[:, 1], co[:, 2]) >= 1.0
        mw = np.array(o.matrix_world)
        for a in np.radians(np.arange(0.0, 360.0, 5.0)):
            c, s = math.cos(a), math.sin(a)
            rot = np.array([[1, 0, 0], [0, c, -s], [0, s, c]])
            w = (mw[:3, :3] @ (rot @ co.T)).T + mw[:3, 3]
            low = min(low, float(w[:, 2].min()))
            w = w[far]
            r = np.hypot(w[:, 0], w[:, 1])
            for (x, y, z), rr in zip(w, r):
                if BASE_TOP <= z <= SMOCK_TOP:
                    gap = min(gap, rr - (smock_r(z) + LAP))
                elif SMOCK_TOP < z <= DOME_TOP:
                    gap = min(gap, rr - (cap_r(z) + 0.05))
    return low, gap


def tower_collider():
    """Hull of the base and smock (no stage, cap, fantail or sails): an octagonal frustum pair round
    the walls, the base's foot corners its reach."""
    m = Mesh()
    rings = m.lathe([(B0 + 0.02, DROP), (B1 + 0.02, BASE_TOP), (S1 + LAP + 0.02, SMOCK_TOP)], OCT,
                    ["brick", "brick"])
    m.face([Vector(v.co) for v in rings[0]], "brick", Vector((0, 0, -1)))
    m.face([Vector(v.co) for v in rings[-1]], "brick", Vector((0, 0, 1)))
    bmesh.ops.triangulate(m.bm, faces=m.bm.faces)
    mesh = bpy.data.meshes.new(f"{P}_trunk")
    m.bm.to_mesh(mesh)
    m.bm.free()
    trunk = scene.link(bpy.data.objects.new(f"{P}_trunk", mesh))
    col = collider.collider_from(trunk, "hull", f"{P}_collider")
    bpy.data.objects.remove(trunk)
    bpy.context.view_layer.update()
    return col


def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(P)
    root = scene.empty(P)
    for s, (col, rough, _) in SLOTS.items():
        materials.pbr_material(f"{P}_{s}", (*col, 1.0), rough)
    bodies = [body(i) for i in range(3)]
    rotors = [rotor(i) for i in range(3)]
    parts = bodies + rotors
    tris = [geom.tri_count(b) + geom.tri_count(r) for b, r in zip(bodies, rotors)]
    split = [(geom.tri_count(b), geom.tri_count(r)) for b, r in zip(bodies, rotors)]
    low, gap = sail_clearance(rotors)
    print(f"{P}: tris per LOD {tris} (body, rotor) {split}; sails sweep down to z {low:.3f} "
          f"(rail top {RAIL_TOP:.2f}), radial gap to smock/cap {gap:.3f}")
    budgets.check(tuple(tris), "prop", P)
    assert low >= RAIL_TOP + SAIL_CLEAR_Z, f"sails sweep to z {low:.3f}, rail top {RAIL_TOP}"
    assert gap >= SAIL_CLEAR_R, f"sails come within {gap:.3f} m of the smock or cap"

    uv.atlas_unwrap(parts, margin=UV_MARGIN)
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in parts}
    print(f"{P}: masks {MASK_SIZE}² density {density}")
    signature = bake.receiver_signature(parts)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(P)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{P}.ao", MASK_SIZE, non_color=True)
        # The bake margin stays inside half the island spacing, so a later part's bake cannot spill
        # onto an earlier part's islands; each part bakes alone (bake_to hides the others).
        for i, o in enumerate(parts):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{P}.ao.tmp.png"))
        mats = [bpy.data.materials[n] for n in NAMES]
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for s, (_, _, hk) in SLOTS.items():
            bpy.data.materials[f"{P}_{s}"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*hk, 1.0)
        hooks = bake.new_image(f"{P}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(parts):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{P}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{P}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        px, filled = fill_unbaked(px, parts, MASK_SIZE, [hk for (_, _, hk) in SLOTS.values()])
        bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        samples = centroid_samples(parts, px, MASK_SIZE, NAMES, P)
        stats = {"aoMean": float(a.mean()), "aoP05": float(np.percentile(a, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": {o.name: samples[f"lod{i}"] for i, o in enumerate(parts)}}
        print(f"{P}: masks {stats}")
        bad = {o.name: samples[f"lod{i}"]["unbakedTris"] for i, o in enumerate(parts)
               if samples[f"lod{i}"]["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

    for o in parts:
        o.parent = root
    col = tower_collider()
    col.parent = root
    export.export_glb(root, str(out / f"{P}.glb"))
    report = export.write_export_report(str(out / "export-report.json"), root, P, __file__)
    # The gate counts each LOD as its body plus its rotor; fold the rotor into lodTris (provenance
    # reads one entry per LOD) and keep the split for the record.
    report["lodTris"] = {f"{P}_lod{i}": t for i, t in enumerate(tris)}
    report["partTris"] = {o.name: geom.tri_count(o) for o in parts}
    report["rotor"] = {"nodes": [o.name for o in rotors], "axisLocal": [1.0, 0.0, 0.0],
                       "tiltDeg": math.degrees(THETA), "hubBlender": [round(c, 4) for c in HUB]}
    report["sailClearance"] = {"lowestZ": round(low, 3), "railTopZ": RAIL_TOP, "radialGapM": round(gap, 3)}
    (out / "export-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{P}_lod0"], masksDensityPxPerM=density,
                           uvMargin=UV_MARGIN, maskStats=stats, skipped=skip_bake)


main()
