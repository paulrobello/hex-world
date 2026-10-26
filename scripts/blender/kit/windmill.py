"""windmill: a white English tower mill with a boat-shaped cap, a still eight-bladed fantail and four
lattice sails on a rotor that the runtime turns (PRD §6 props, §7.2 animated props as rigid object
transforms). Owner-approved references: assets-source/refs/windmill (ref-1..3). The owner's answer
to those references (approval.json, A17) overrides two things they draw: the tower is white, not
black-tarred, and the tile is flat (no mound, no gravel path). Scripted route.

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the tower's centre at ground
level (z = 0); the sails face +x (yaw 0, as the houses' doors do). The tower runs from DROP (below
the meadow cap's deepest relief, -0.25 m) so it never floats over a cap dip.

Nodes: `windmill_lod{0,1,2}` (tower, cap, fantail: still), `windmill_rotor_lod{0,1,2}` (canister,
stocks and sails: turned by the runtime) and `windmill_collider` (the tower trunk only). Each rotor
node sits on the hub with its local +X along the windshaft (tilted THETA up from +x), and its mesh is
authored round that local X axis, so the runtime spins it as one rigid transform about local X. The
gate counts each LOD as its body plus its rotor (LOD0 <= 1,500 as a prop, LOD1 <= 50 %, LOD2 <= 15 %).

Look: stylized-real at real-metre scale. Every surface is a colour-only slot plus the piece's
`masks` (the round tower and the curved cap cannot take a world-space tileable set, and a unique
atlas at 256 px/m would not fit a prop's budget). Directional detail (sail bars, shutters, frames)
is geometry. masks: R snow 1 (0 on glass), G porosity per surface, B night glow on the glass, A
baked AO, every LOD on its own islands of one 1024² atlas. The body and the rotor are baked as
separate objects (bake_to hides every other mesh), so no shadow of the tower turns with the sails."""
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

from lib import bake, collider, export, geom, materials, scene, uv  # noqa: E402

import _lod_copy  # noqa: E402

P = "windmill"
DROP = -0.35
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
SHARP_DEG = 50.0
PROP_LOD0_MAX = 1500

# Tower: a tapered round brick tower, rendered white (owner, A17).
RB, RT, H = 2.7, 1.9, 10.5
PLINTH_TOP = 0.5
# Cap: a curb flaring out over the tower top, a boarded drum, and a boat-shaped roof whose flat
# front gable carries the windshaft.
RD = RT + 0.22
CURB_Z0, CURB_Z1 = H - 0.35, H - 0.1
EAVE = H + 0.55
CAP_H = 1.85
XF = RD + 0.25
XB = -(RD + 0.5)
# Windshaft: tilted THETA up (real mills 5-15°) so the sails clear the tower's wider foot.
THETA = math.radians(10.0)
AXLE = Vector((math.cos(THETA), 0.0, math.sin(THETA)))
HUB = Vector((XF, 0.0, EAVE + 0.85)) + AXLE * 0.75
# Sails: the stock runs from the hub to SAIL_R1; the lattice frame spans SAIL_R0..SAIL_R1 along it,
# SAIL_TRAIL on the trailing (driving) side and SAIL_LEAD on the leading side.
SAIL_R0, SAIL_R1 = 1.7, 8.4
SAIL_TRAIL, SAIL_LEAD = -1.15, 0.5
# Fantail (still, owner A17): eight blades on an axle along y, behind the cap.
FAN_C = Vector((XB - 0.9, 0.0, EAVE + 2.2))
FAN_R0, FAN_R1 = 0.35, 1.4

# Blender colours are linear and equal the manifest colours (every slot is colour-only).
SLOTS = {
    "tower": ((0.62, 0.60, 0.55), 0.9, (1.0, 0.5, 0.0)),
    "cap": ((0.68, 0.68, 0.64), 0.75, (1.0, 0.45, 0.0)),
    "tar": ((0.035, 0.033, 0.03), 0.6, (1.0, 0.3, 0.0)),
    "timber": ((0.16, 0.12, 0.085), 0.85, (1.0, 0.6, 0.0)),
    "sail": ((0.66, 0.65, 0.60), 0.8, (1.0, 0.5, 0.0)),
    "iron": ((0.05, 0.05, 0.055), 0.55, (1.0, 0.15, 0.0)),
    "door": ((0.12, 0.07, 0.035), 0.7, (1.0, 0.5, 0.0)),
    "glass": ((0.20, 0.16, 0.10), 0.2, (0.0, 0.05, 1.0)),
}
NAMES = [f"{P}_{s}" for s in SLOTS]

# Windows on the tower: (azimuth deg, sill z, width, height); the door faces +x under the sails.
WINDOWS = [(0.0, 4.4, 0.5, 0.75), (0.0, 7.6, 0.45, 0.65), (90.0, 5.8, 0.5, 0.75),
           (270.0, 3.0, 0.5, 0.75), (180.0, 4.8, 0.5, 0.75)]
DOOR_W, DOOR_H = 0.8, 2.0

# Detail per LOD.
LOD = [
    {"seg": 20, "roof_seg": 8, "roof_phi": (30.0, 60.0, 80.0), "bars": 9, "shutters": True,
     "frames": 2, "door": 2, "fan": 2, "canister": 8, "gallery": True},
    {"seg": 12, "roof_seg": 4, "roof_phi": (45.0, 75.0), "bars": 4, "shutters": False,
     "frames": 1, "door": 1, "fan": 1, "canister": 6, "gallery": False},
    {"seg": 8, "roof_seg": 2, "roof_phi": (55.0,), "bars": 0, "shutters": False,
     "frames": 0, "door": 0, "fan": 0, "canister": 0, "gallery": False},
]


def radius(z):
    return RB + (RT - RB) * z / H


class Mesh:
    """bmesh accumulator with per-face slot indices. Parts never share vertices unless built
    together, so no global weld runs (double-sided boards keep their own back sheets)."""

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
        """A double-sided board: the front quad on pts facing n and a back quad `thick` behind it
        on its own vertices (a colour-only slot is culled single-sided)."""
        n = Vector(n).normalized()
        self.face([Vector(p) for p in pts], slot, n)
        self.face([Vector(p) - n * thick for p in pts], slot, -n)

    def prism(self, a, b, half, up, slot, half_b=None, caps=(False, False)):
        """A rectangular bar from a to b, half sizes (width, height) at a and half_b at b, height
        along `up` (made perpendicular to the bar)."""
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
        axis "z" revolves about +Z; "x" about +X (h along x)."""
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


def facet_frame(az_deg, z, seg, proud=0.0):
    """Point on the flat tower facet centred at azimuth az_deg (facets are centred on multiples of
    360/seg), with that facet's tangent t, up-slope u and outward normal n."""
    a = math.radians(az_deg)
    alpha = math.atan((RB - RT) / H)
    r = radius(z) * math.cos(math.pi / seg)
    radial = Vector((math.cos(a), math.sin(a), 0.0))
    n = (radial * math.cos(alpha) + Vector((0, 0, 1)) * math.sin(alpha)).normalized()
    t = Vector((-math.sin(a), math.cos(a), 0.0))
    u = n.cross(t).normalized()
    c = radial * r + Vector((0, 0, z)) + n * proud
    return c, t, u, n


def window(m, az, z0, w, h, detail, seg):
    """Glass on a facet with a picture frame proud of it: front ring, outer sides and an inner reveal
    (detail 2), the front ring only (1), or the glass alone (0)."""
    f, p, g = 0.08, 0.07, 0.012
    c, t, u, n = facet_frame(az, z0 + h / 2, seg)

    def q(du, dv, out):
        return c + t * du + u * dv + n * out

    hw, hh = w / 2, h / 2
    m.face([q(-hw, -hh, g), q(hw, -hh, g), q(hw, hh, g), q(-hw, hh, g)], "glass", n)
    if detail == 0:
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


def door(m, detail, seg):
    c, t, u, n = facet_frame(0.0, 0.0, seg)
    hw = DOOR_W / 2

    def q(du, z, out):
        return c + t * du + u * (z / u.z) + n * out

    m.face([q(-hw, 0.0, 0.02), q(hw, 0.0, 0.02), q(hw, DOOR_H, 0.02), q(-hw, DOOR_H, 0.02)], "door", n)
    if detail == 0:
        return
    jamb = (0.06, 0.05)
    lintel = q(0.0, DOOR_H + 0.06, 0.05)
    m.prism(lintel - t * (hw + 0.12), lintel + t * (hw + 0.12), jamb, n, "timber", caps=(True, True))
    if detail == 2:
        for s in (-1, 1):
            m.prism(q(s * (hw + 0.06), DROP, 0.05), q(s * (hw + 0.06), DOOR_H, 0.05), jamb, n, "timber")
        step = c + n * 0.28 + Vector((0, 0, (DROP + 0.12) / 2))
        m.prism(step - t * (hw + 0.2), step + t * (hw + 0.2), (0.28, (0.12 - DROP) / 2), Vector((0, 0, 1)),
                "tar", caps=(True, True))


def roof(m, lod):
    """The boat-shaped cap roof: a half-ellipse section swept from the flat front gable (XF) back
    through x = 0, then shrinking as a quarter ellipsoid to a point at XB, with a soffit under it."""
    segs = lod["roof_seg"]
    stations = [(XF, 1.0), (0.0, 1.0)] + [(XB * math.sin(math.radians(a)), math.cos(math.radians(a)))
                                           for a in lod["roof_phi"]]
    rings = []
    for x, s in stations:
        ring = []
        for k in range(segs + 1):
            b = math.pi * k / segs
            ring.append(m.bm.verts.new((x, -RD * s * math.cos(b) - 0.1 * s * math.cos(b), EAVE + CAP_H * s * math.sin(b))))
        rings.append(ring)
    apex = m.bm.verts.new((XB, 0.0, EAVE))
    for i in range(len(rings) - 1):
        for k in range(segs):
            q = [rings[i][k], rings[i + 1][k], rings[i + 1][k + 1], rings[i][k + 1]]
            mid = sum((v.co for v in q), Vector()) / 4
            m.face(q, "cap", Vector((0.0, mid.y, mid.z - EAVE)))
    for k in range(segs):
        tri = [rings[-1][k], apex, rings[-1][k + 1]]
        mid = sum((v.co for v in tri), Vector()) / 3
        m.face(tri, "cap", Vector((-1.0, mid.y, mid.z - EAVE)))
    # Front gable: the section closed at XF (own vertices, so it shades flat), and the soffit.
    m.face([Vector(v.co) for v in rings[0]], "cap", Vector((1, 0, 0)))
    outline = [Vector(r[0].co) for r in rings] + [Vector(apex.co)] + [Vector(r[-1].co) for r in reversed(rings)]
    m.face(outline, "cap", Vector((0, 0, -1)))


def gallery(m):
    """A rail along both sides of the cap at the eave (ref-2): two rails over four posts a side."""
    y = RD + 0.35
    for s in (-1, 1):
        for z in (EAVE + 0.45, EAVE + 0.9):
            m.prism((XF - 0.2, s * y, z), (XB * 0.55, s * y, z), (0.03, 0.03), (0, 0, 1), "timber",
                    caps=(True, True))
        for x in np.linspace(XF - 0.25, XB * 0.55 + 0.05, 4):
            m.prism((x, s * y, EAVE - 0.15), (x, s * y, EAVE + 0.95), (0.035, 0.035), (1, 0, 0), "timber")
        m.board([(XF - 0.2, s * (RD + 0.08), EAVE), (XB * 0.55, s * (RD + 0.08), EAVE),
                 (XB * 0.55, s * (y + 0.06), EAVE), (XF - 0.2, s * (y + 0.06), EAVE)], (0, 0, 1), "timber", 0.05)


def fantail(m, detail):
    """The still fantail (owner A17): blades in the XZ plane on an axle along y, on two raking legs
    from the back of the cap."""
    c = FAN_C
    for s in (-1, 1):
        m.prism((XB * 0.62, s * 0.95, EAVE - 0.05), (c.x, s * 0.42, c.z), (0.06, 0.07), (1, 0, 1), "timber")
    if detail == 0:
        ring = [c + Vector((FAN_R1 * math.cos(a), 0.0, FAN_R1 * math.sin(a)))
                for a in (2 * math.pi * k / 6 for k in range(6))]
        m.board([p + Vector((0, 0.01, 0)) for p in ring], (0, 1, 0), "sail")
        return
    m.prism(c - Vector((0, 0.45, 0)), c + Vector((0, 0.45, 0)), (0.05, 0.05), (0, 0, 1), "timber",
            caps=(True, True))
    if detail == 2:
        m.prism(c - Vector((0, 0.12, 0)), c + Vector((0, 0.12, 0)), (0.16, 0.16), (0, 0, 1), "iron",
                caps=(True, True))
    for k in range(8):
        a = 2 * math.pi * (k + 0.5) / 8
        d = Vector((math.cos(a), 0.0, math.sin(a)))
        e = Vector((-math.sin(a), 0.0, math.cos(a)))
        w0, w1 = 0.12, 0.24
        pts = [c + d * FAN_R0 - e * w0, c + d * FAN_R1 - e * w1, c + d * FAN_R1 + e * w1, c + d * FAN_R0 + e * w0]
        m.board([p + Vector((0, 0.01, 0)) for p in pts], (0, 1, 0), "sail")


def body(i):
    lod = LOD[i]
    seg = lod["seg"]
    m = Mesh()
    if i < 2:
        m.lathe([(radius(DROP), DROP), (radius(PLINTH_TOP), PLINTH_TOP), (radius(H), H)], seg, ["tar", "tower"])
    else:
        m.lathe([(radius(DROP), DROP), (radius(H), H)], seg, ["tower"])
    m.lathe([(RT - 0.05, CURB_Z0), (RD, CURB_Z1)], seg, ["tar"])
    m.lathe([(RD, CURB_Z1), (RD, EAVE + 0.02)], seg, ["cap"])
    roof(m, lod)
    for az, z0, w, h in WINDOWS:
        window(m, az, z0, w, h, lod["frames"], seg)
    door(m, lod["door"], seg)
    fantail(m, lod["fan"])
    if lod["gallery"]:
        gallery(m)
    return m.obj(f"{P}_lod{i}")


STOCK_DX = (0.05, -0.17)  # local x of the two stocks' centres (they cross at the canister)
STOCK_HALF = (0.1, 0.13)  # half width, half depth (along the windshaft)


def sail(m, k, lod):
    """Sail k in the local YZ plane at 45° + 90°·k, its lattice on the front face of its stock:
    hemlaths along both edges, cross bars, shutter boards on the trailing side and a leading board
    (LOD0/1), or one board (LOD2)."""
    psi = math.radians(45.0 + 90.0 * k)
    d = Vector((0.0, math.cos(psi), math.sin(psi)))
    e = Vector((0.0, -math.sin(psi), math.cos(psi)))  # direction of travel (+X right-hand spin)
    x = Vector((1.0, 0.0, 0.0))
    face = STOCK_DX[k % 2] + STOCK_HALF[1]

    def at(r, w, dx):
        return d * r + e * w + x * (face + dx)

    if lod["bars"] == 0:
        m.board([at(SAIL_R0, SAIL_TRAIL, 0.03), at(SAIL_R1, SAIL_TRAIL, 0.03),
                 at(SAIL_R1, SAIL_LEAD, 0.03), at(SAIL_R0, SAIL_LEAD, 0.03)], x, "sail", 0.04)
        return
    for w in (SAIL_TRAIL, SAIL_LEAD):
        m.prism(at(SAIL_R0 - 0.1, w, 0.0), at(SAIL_R1 + 0.05, w, 0.0), (0.04, 0.05), x, "timber")
    for r in np.linspace(SAIL_R0, SAIL_R1, lod["bars"]):
        m.prism(at(r, SAIL_TRAIL - 0.06, 0.0), at(r, SAIL_LEAD + 0.06, 0.0), (0.035, 0.04), x, "timber")
    gap = 0.06
    if lod["shutters"]:
        rs = np.linspace(SAIL_R0, SAIL_R1, lod["bars"])
        for r0, r1 in zip(rs[1:-2], rs[2:-1]):
            m.board([at(r0 + gap, SAIL_TRAIL + gap, 0.03), at(r1 - gap, SAIL_TRAIL + gap, 0.03),
                     at(r1 - gap, -0.14, 0.03), at(r0 + gap, -0.14, 0.03)], x, "sail")
    else:
        m.board([at(SAIL_R0 + 0.4, SAIL_TRAIL + gap, 0.03), at(SAIL_R1 - 1.6, SAIL_TRAIL + gap, 0.03),
                 at(SAIL_R1 - 1.6, -0.14, 0.03), at(SAIL_R0 + 0.4, -0.14, 0.03)], x, "sail")
    m.board([at(SAIL_R0 + 0.1, 0.14, 0.03), at(SAIL_R1 - 0.1, 0.14, 0.03),
             at(SAIL_R1 - 0.1, SAIL_LEAD - gap, 0.03), at(SAIL_R0 + 0.1, SAIL_LEAD - gap, 0.03)], x, "sail")


def rotor(i):
    """The turning part, in the rotor node's local frame: origin on the hub, windshaft along +X."""
    lod = LOD[i]
    m = Mesh()
    if lod["canister"]:
        rings = m.lathe([(0.2, -0.75), (0.3, -0.35), (0.3, 0.3)], lod["canister"], ["iron", "iron"], axis="x")
        m.face([Vector(v.co) for v in rings[-1]], "iron", Vector((1, 0, 0)))
    for k in range(2):
        psi = math.radians(45.0 + 90.0 * k)
        d = Vector((0.0, math.cos(psi), math.sin(psi)))
        dx = Vector((STOCK_DX[k], 0.0, 0.0))
        m.prism(dx - d * SAIL_R1, dx + d * SAIL_R1, STOCK_HALF, (1, 0, 0), "timber",
                caps=(i < 2, i < 2))
    for k in range(4):
        sail(m, k, lod)
    o = m.obj(f"{P}_rotor_lod{i}", location=HUB, rotation=(0.0, -THETA, 0.0))
    o["kind"] = "rotor"
    co = np.array([v.co[:] for v in o.data.vertices])
    centre = co[:, 1:].mean(axis=0)
    assert np.abs(centre).max() < 0.01, f"rotor lod{i} off its axle by {centre}"
    return o


def tower_collider():
    """Hull of the tower trunk alone (no cap, fantail or sails): a 12-gon frustum round the tower."""
    m = Mesh()
    k = 1.0 / math.cos(math.pi / 12)
    rings = m.lathe([(radius(DROP) * k + 0.02, DROP), (radius(H) * k + 0.02, H)], 12, ["tower"])
    m.face([Vector(v.co) for v in rings[0]], "tower", Vector((0, 0, -1)))
    m.face([Vector(v.co) for v in rings[1]], "tower", Vector((0, 0, 1)))
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
    print(f"{P}: tris per LOD {tris} (body, rotor) {split}")
    assert tris[0] <= PROP_LOD0_MAX and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris

    uv.atlas_unwrap(parts, margin=0.012)
    signature = bake.receiver_signature(parts)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(P)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{P}.ao", MASK_SIZE, non_color=True)
        # A 4 px margin stays inside the 12 px island spacing, so a later part's bake cannot spill
        # onto an earlier part's islands; each part bakes alone (bake_to hides the others).
        for i, o in enumerate(parts):
            bake.bake_to(o, "AO", ao, margin=4, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{P}.ao.tmp.png"))
        mats = [bpy.data.materials[n] for n in NAMES]
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for s, (_, _, hk) in SLOTS.items():
            bpy.data.materials[f"{P}_{s}"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*hk, 1.0)
        hooks = bake.new_image(f"{P}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(parts):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=4, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{P}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{P}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        stats = {"aoMean": float(a.mean()), "aoP05": float(np.percentile(a, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean())}
        print(f"{P}: masks {stats}")

    # LOD1 and LOD2 keep the fantail's iron hub (owner A2 Q4: every slot has a middle LOD; at 400 m the
    # 0.32 m hub is about one pixel): LOD0's 12-tri hub, copied after the bake with its baked UVs, so
    # the atlas and LOD0 are as before.
    bodies[1], hub = _lod_copy.copy_faces(bodies[0], bodies[1], lambda _c, m: m == f"{P}_iron")
    bodies[2], hub2 = _lod_copy.copy_faces(bodies[0], bodies[2], lambda _c, m: m == f"{P}_iron")
    parts = bodies + rotors
    tris[1] += hub
    tris[2] += hub2
    print(f"{P}: LOD1 + hub {hub}, LOD2 + hub {hub2} tris -> tris per LOD {tris}")
    assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris

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
                       "tiltDeg": math.degrees(THETA)}
    (out / "export-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in parts}
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=4,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{P}_lod0"], masksDensityPxPerM=density,
                           maskStats=stats, skipped=skip_bake)


main()
