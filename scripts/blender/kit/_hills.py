"""_hills: the family file of the single-tile landforms (P4, owner A40 O1 2026-10-27: "single-tile hills
and peaks that rise inside the hex and meet neighbours at their edge level"), hill_meadow and
peak_granite. Spec: docs/superpowers/specs/2026-11-13-single-tile-hills-design.md (pending owner
approval, built ahead under the no-idle rule).

The pipeline is massif_granite's (`_massif.build`): a heightfield grass cap with scree lenses, a granite
rock planted in it (a signed-distance volume of the mass, slab facets and joints carved in, OpenVDB
meshed), a unique bake of the rock and scree, the soil skirt. `_massif` reads its landform and
geometry through module globals at call time, so `configure` points them at one hex and at this
file's landforms. `_massif.py` itself is never edited.

Geometry contract (a `base` piece, so the base gate's ring checks apply): the cap border is
`hexgeo.border_ring(BORDER_SUBDIV)` at z = 0, meadow_base's own points, identical in every LOD, normals
+Z; the skirt goes to z = -LEVEL_STEP; the landform rises inside the hex and stays at least RIM m
inside the border, flat there, with meadow's hollows. Kind base (LOD0 <= 4000 tris), KTX2 class
piece (1.5 MB a family): a 1024 atlas, normal, ORM and masks at 512. Pack hero (spec D6)."""
import json
import math
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import hexgeo  # noqa: E402

import _massif as M  # noqa: E402

fbm, sstep, smax = M.fbm, M.sstep, M.smax


def hex_dist(x, y):
    """Signed distance to the hex border (positive inside), numpy or scalar: the distance to the
    nearest edge line, which inside a convex hex is the distance to the outline."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    return hexgeo.APOTHEM - np.max([x * n[0] + y * n[1] for n in (hexgeo.edge_normal(k) for k in range(6))], axis=0)


# --- the forms (Blender XY, metres; the crown or summit at (SX, SY)) -----------------------------
# Common to both: the atlas split of outcrop_granite (1.17 MB a family), the bake reach scaled to the
# smaller rock, the ground spacing per LOD (steep, mid, gentle, flat; _massif.interior_points
# rounds each class to a multiple of the steep lattice), and the rock tris per LOD, sized so that
# LOD0 lands near the base budget (4000) and LOD2's 15% holds the fixed ring and skirt (286 tris,
# gates.md floor LOD0 >= 1907) plus a readable landform.
COMMON = dict(KIND="base", ATLAS=1024, NORMAL_SIZE=512, ORM_SIZE=512, MASK_SIZE=512, SAMPLES=32, MARGIN=6,
              APRON_R=17.0)

FORMS = {
    # A rounded grassy hill, crown about 8.5 m, a few low granite ledges near the crown (ref-1; ref-4
    # from the low camera). The grass is a smooth dome on a slightly wandering foot circle, faded flat
    # inside the rim; the outcrops are low flat-topped slabs on the slope; two small gravel aprons
    # run downhill from the larger ones (the pipeline needs scree in every LOD; granite weathers so).
    "hill_meadow": dict(
        # The crown a little behind the centre and the profile power 0.6, a broad rounded dome as
        # ref-1's (power 1 read as a cone); the slabs spread over the upper dome as ref-1's three.
        SX=0.0, SY=1.5, HILL_H=8.5, HILL_P=0.6, HILL_WOB=0.25, RIM=(0.6, 3.6),
        # (x, y from the crown, half length, half width, yaw deg, height over the grass)
        OUTCROPS=((2.8, -1.5, 3.2, 1.7, 15.0, 1.1), (-4.0, 3.5, 2.2, 1.4, -30.0, 0.9),
                  (2.0, 6.0, 1.8, 1.2, 40.0, 0.75), (-2.0, -4.5, 1.0, 0.8, 10.0, 0.45)),
        # (outcrop index, half angle deg, length m) of each gravel apron, downhill from its outcrop
        APRONS=((0, 22.0, 4.0), (1, 24.0, 3.5)),
        FAN_T=0.45, SPACING=((0.66, 0.66, 0.66, 1.32), (1.05, 1.05, 1.05, 2.1), (2.8, 2.8, 2.8, None)),
        FAN_STEP=(0.8, 1.2, 3.0), ROCK_TRIS=(560, 240, 48), VOX=0.15, VX=(-9.5, 10.0, 1.0, 11.5), BURY=1.5,
        SLAB=(1.5, 0.5, 0.25, 9.0), DIL_CARVE=(0.45, 0.6), JOINT=((0.05, 0.12), 0.28), SMOOTH_ITER=6,
        GRAIN=0.45, MOSS=(8.5, 6.0), CAGE=(0.12, 0.25, 0.6), SCREE_CAGE=(0.6, 1.2), ROCK2_CAGE=(0.6, 1.4)),
    # A craggy granite peak, summit about 18 m near the centre, buttresses and cliffs split by
    # vertical joints, scree fans in the gullies, grassy lower slopes easing out flat at the rim
    # (ref-1; ref-4 the silhouette). The massif's form at about half scale on one hex.
    "peak_granite": dict(
        SX=-0.6, SY=0.2, SUMMIT_H=17.6, RIM=(0.5, 4.0),
        # (x, y, height, foot radius, profile power, shoulder power, x stretch): ref-1's twin summit
        # pinnacles, a lower tower to the right, a shoulder to the front left and one behind, and a low
        # block to the front right. Full shoulders (power 1.5-1.8) read as columns, not a cone.
        TOPS=((-0.6, 0.2, 17.6, 4.6, 0.9, 1.6, 1.0), (1.9, -0.9, 15.6, 3.4, 0.9, 1.7, 1.0),
              (5.6, 1.0, 13.0, 3.2, 0.85, 2.2, 1.0), (-5.2, -1.6, 9.5, 3.8, 0.85, 1.8, 1.0),
              (-3.2, 4.0, 12.5, 3.4, 0.85, 2.0, 1.0), (6.0, -3.6, 6.0, 2.6, 0.9, 1.8, 1.0)),
        CLEFT=(0.8, 0.30), SPINE_E0=6.0,
        SPINES=((-146.0, 6.5, 2.0, 1.3), (-86.0, 6.0, 2.0, 1.3), (-3.0, 6.5, 1.9, 1.3), (29.0, 7.0, 1.9, 1.2),
                (72.0, 6.0, 1.9, 1.2), (127.0, 7.0, 2.0, 1.3)),
        GRASS=(7.0, 2.5, 16.0, 1.6),  # A_G, R_IN, R_OUT, P_G: massif's slopes at about half scale
        # (bearing deg, half angle deg, length m): narrow gravel chutes from the clefts, as ref-1's
        FANS=((-116.0, 9.0, 6.0), (-46.0, 9.0, 5.5), (168.0, 9.0, 5.5), (12.0, 7.0, 4.0), (100.0, 9.0, 5.0),
              (50.0, 8.0, 4.5)),
        FAN_T=0.6, SPACING=((0.8, 0.8, 0.8, 1.6), (1.15, 1.15, 1.15, 2.3), (2.8, 2.8, 5.6, None)),
        FAN_STEP=(0.9, 1.4, 3.2), ROCK_TRIS=(1900, 680, 120), VOX=0.2, VX=(-12.0, 12.0, -2.0, 20.0), BURY=2.0,
        SLAB=(1.6, 0.36, 0.45, 7.0), DIL_CARVE=(0.65, 0.8), JOINT=((0.08, 0.18), 0.38), SMOOTH_ITER=6,
        GRAIN=0.6, MOSS=(7.5, 3.5), CAGE=(0.12, 0.25, 0.8), SCREE_CAGE=(0.7, 1.4), ROCK2_CAGE=(0.7, 1.6)),
}
F = {}  # the configured form


def rim_fade(x, y):
    """0 at the border and on the RIM band's outer edge, 1 inside it: the landform's flat Meadow rim."""
    return sstep(F["RIM"][0], F["RIM"][1], hex_dist(x, y))


def no_flank(x, y):
    """`_massif.flank` is the massif's rim ramp; these forms build theirs into the grass, so the cap's
    point classes follow the ground's own slope."""
    return np.zeros(np.shape(np.asarray(x, float)))


# --- hill_meadow ---------------------------------------------------------------------------------

def hill_grass(x, y):
    """The dome: H sstep(t) with t = d / (d + r), d the rounded distance to the hex border less the
    flat rim and r the (bearing-wandered) distance from the crown, so the contours run round near the
    crown and with the hex near the rim, the crown is rounded (t = 1) and the foot eases flat into the
    rim (t = 0) along every bearing, corners included. Soft ripples run down the flanks (zero at the
    crown and the foot)."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    dx, dy = x - M.SX, y - M.SY
    r = np.hypot(dx, dy)
    th = np.arctan2(dy, dx)
    wob = fbm(np.cos(th) * 2.5, np.sin(th) * 2.5, 1.0, 3, 11) - 0.5
    d = np.maximum(M.soft_outline_dist(x, y) - F["RIM"][0], 0.0)
    t = d / np.maximum(d + r * (1.0 + F["HILL_WOB"] * wob), 1e-6)
    dome = F["HILL_H"] * sstep(0.0, 1.0, t) ** F["HILL_P"]
    env = 4.0 * t * (1.0 - t)
    ripple = env * (0.28 * np.sin(5.0 * th + 1.3) + 0.6 * (fbm(x, y, 6.0, 3, 23) - 0.5))
    return dome + ripple


def hill_mass(x, y):
    """The outcrops' tops over the grass (absolute): low slabs, flat-topped with steep rounded sides,
    smooth-maxed over a mass 4 m under the grass, so the rock is only the outcrops."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    g = M.slope_h(x, y)
    m = g - 4.0
    for k, (ox, oy, a, b, yaw, h) in enumerate(F["OUTCROPS"]):
        c, s_ = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
        u = (x - M.SX - ox) * c + (y - M.SY - oy) * s_
        v = -(x - M.SX - ox) * s_ + (y - M.SY - oy) * c
        e = np.sqrt((u / a) ** 2 + (v / b) ** 2)
        ang = np.arctan2(v, u)
        e = e * (1.0 + 0.14 * (fbm(np.cos(ang) * 1.8 + k * 7.0, np.sin(ang) * 1.8, 1.0, 3, 57 + k) - 0.5))
        top = g + h * np.clip(1.0 - e ** 3, 0.0, 1.0) ** 0.6 - 2.5 * np.maximum(e - 1.0, 0.0)
        m = smax(m, top, 0.35)
    return m


def hill_apexes():
    """Each apron's apex: from its outcrop's centre outward along the bearing from the crown, the
    first point where the outcrop has sunk to within 0.2 m of the grass, less 0.6 m."""
    out = []
    for k, half, length in F["APRONS"]:
        ox, oy = F["OUTCROPS"][k][:2]
        a = math.atan2(oy, ox)
        r0 = math.hypot(ox, oy)
        while r0 < 12.0:
            p = (np.array([M.SX + r0 * math.cos(a)]), np.array([M.SY + r0 * math.sin(a)]))
            if M.mass(*p)[0] < M.grass_h(*p)[0] + 0.2:
                break
            r0 += 0.1
        r0 -= 0.6
        out.append((a, M.SX + r0 * math.cos(a), M.SY + r0 * math.sin(a), math.radians(half), length))
    return out


# --- peak_granite --------------------------------------------------------------------------------

def peak_grass(x, y):
    """The grass slopes (the massif's `slope_h` at half scale): monotonic along every ray from the
    summit, a rib under each spine and lower ground under the gullies, faded flat inside the rim."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    a_g, r_in, r_out0, p_g = F["GRASS"]
    r = np.hypot(x - M.SX, y - M.SY)
    th = np.arctan2(y - M.SY, x - M.SX)
    rib = np.max(M._bearing_field(x, y, 14.0), axis=0)
    wob = fbm(np.cos(th) * 2.5, np.sin(th) * 2.5, 1.0, 3, 11) - 0.5
    r_out = r_out0 * (1.0 + 0.08 * wob) + 2.0 * rib
    t = np.clip((r_out - r) / (r_out - r_in), 0.0, 1.0)
    return a_g * (0.82 + 0.3 * rib + 0.2 * wob) * t ** p_g * rim_fade(x, y)


def peak_mass(x, y):
    """The rock mass's top (absolute): the tops and the spines smooth-maxed over a mass 4 m under the
    grass, cleft along the fans' bearings (the massif's `mass` at half scale)."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    g = M.slope_h(x, y)
    m = g - 4.0
    th = np.arctan2(y - M.SY, x - M.SX)
    rs = np.hypot(x - M.SX, y - M.SY)
    cleft = np.zeros(np.shape(x))
    for ang, _half, _length in M.FANS:
        dth = np.angle(np.exp(1j * (th - math.radians(ang))))
        cleft = np.maximum(cleft, np.exp(-(dth / M.CLEFT_SIGMA) ** 2))
    keep = 1.0 - M.CLEFT_DEPTH * cleft * sstep(1.5, 5.0, rs)
    for tx, ty, h, rad, p, q, ex in M.TOPS:
        dx, dy = (x - tx) / ex, y - ty
        d = np.sqrt(dx * dx + dy * dy + 1.0) - 1.0
        a = np.arctan2(dy, dx)
        d = d * (1.0 + 0.18 * (fbm(np.cos(a) * 2.0 + tx, np.sin(a) * 2.0 + ty, 1.0, 3, 5) - 0.5))
        # Past its radius a top sinks 1.5 m per m (the massif's 4 m per radius is 0.6 m per m on the
        # 6.8 m summit, which kept its buried skirt within 2 m of the grass out to the border).
        top = h * np.clip(1.0 - (d / rad) ** q, 0.0, 1.0) ** p - 1.5 * np.maximum(d - rad, 0.0)
        # The cleft lowers only the rock above the grass: applied below it too (the massif's form),
        # it lifts a top's buried skirt to the grass in each cleft, out to the hex border.
        m = smax(m, np.where(top > g, g + (top - g) * keep, top), 0.9)
    for ang, length, w0, w1 in M.SPINES:
        a = math.radians(ang)
        ux, uy = math.cos(a), math.sin(a)
        u = (x - M.SX) * ux + (y - M.SY) * uy
        v = -(x - M.SX) * uy + (y - M.SY) * ux
        v = v - 0.6 * (fbm(u * 0.3 + ang, 0.0 * u, 1.0, 2, 41) - 0.5) * np.clip(u / length, 0.0, 1.0)
        uc = np.clip(u / length, 0.0, 1.0)
        w = w0 + (w1 - w0) * uc
        e = M.SPINE_E0 * (1.0 - uc) ** 1.25 + 0.5
        # Off the spine and past its tip the crest drops 4 m under the grass, as the base does: the
        # massif's -e side (0.5 m at a tip) left buried rock out to the hex border on one hex.
        side = np.clip(1.0 - (np.abs(v) / w) ** 1.7, -1.0, 1.0)
        crest = (g + np.where(side >= 0.0, e * side, 4.0 * side) - 3.0 * np.maximum(-u - 0.5, 0.0)
                 - 3.0 * np.maximum(u - length, 0.0))
        m = smax(m, crest, 0.7)
    return m


def peak_apexes():
    """Each fan's apex: the rock foot along its ray from the summit (the massif's `_fan_apexes` with
    the walk scaled to one hex)."""
    out = []
    for ang, half, length in M.FANS:
        a = math.radians(ang)
        r0 = 1.5
        while r0 < 12.0:
            p = (np.array([M.SX + r0 * math.cos(a)]), np.array([M.SY + r0 * math.sin(a)]))
            if M.mass(*p)[0] < M.grass_h(*p)[0] + 0.5:
                break
            r0 += 0.25
        out.append((a, M.SX + (r0 - 0.6) * math.cos(a), M.SY + (r0 - 0.6) * math.sin(a), math.radians(half), length))
    return out


# --- wiring --------------------------------------------------------------------------------------

_ORIG = {"classify": M.classify, "granite_shader": M.granite_shader, "tri_noise": M.tri_noise}


def classify(x, y):
    """The massif's classes, and at least the face nearest each fan's axis 1 m below its apex as
    scree: `_massif.build` needs scree in every LOD, and LOD2's coarse cap can straddle a small fan."""
    cls = _ORIG["classify"](x, y)
    if not (cls == "scree").any():
        x, y = np.asarray(x, float), np.asarray(y, float)
        for a, ax, ay, _half, _length in M.FAN_APEX:
            k = int(np.argmin(np.hypot(x - ax - math.cos(a), y - ay - math.sin(a))))
            cls[k] = "scree"
    return cls


def granite_shader(name):
    """The massif's granite with its low moss band moved to this form's lower faces (the massif's is
    7-13 m up a 34 m peak)."""
    m = _ORIG["granite_shader"](name)
    hi, lo = F["MOSS"]
    hit = 0
    for n in m.node_tree.nodes:
        if n.type == "MAP_RANGE" and n.inputs["From Min"].default_value == 13.0 \
                and n.inputs["From Max"].default_value == 7.0:
            n.inputs["From Min"].default_value, n.inputs["From Max"].default_value = hi, lo
            hit += 1
    assert hit == 1, f"{M.PIECE}: the granite moss band node was not found once ({hit})"
    return m


def tri_noise(p, scale, seed):
    """The high poly's triplanar grain, its amplitude scaled to this form's smaller rock."""
    return 0.5 + F["GRAIN"] * (_ORIG["tri_noise"](p, scale, seed) - 0.5)


def configure(piece):
    """Point `_massif`'s globals at one hex and at `piece`'s form."""
    F.clear()
    F.update(COMMON)
    F.update(FORMS[piece])
    M.PIECE = piece
    M.SCREE = "unique"
    for k in ("KIND", "ATLAS", "NORMAL_SIZE", "ORM_SIZE", "MASK_SIZE", "SAMPLES", "MARGIN", "APRON_R", "SX", "SY",
              "FAN_T", "SPACING", "FAN_STEP", "ROCK_TRIS", "VOX", "BURY", "SMOOTH_ITER"):
        setattr(M, k, F[k])
    M.CAGE, M.MAX_RAY, M.AO_DIST = F["CAGE"]
    M.SCREE_CAGE, M.ROCK2_CAGE = F["SCREE_CAGE"], F["ROCK2_CAGE"]
    M.VX0, M.VX1, M.VZ0, M.VZ1 = F["VX"]
    M.SLAB_S, M.SLAB_ALPHA, M.SLAB_LIFT, M.SLAB_TILT = F["SLAB"]
    M.DIL, M.CARVE = F["DIL_CARVE"]
    M.JOINT_W, M.JOINT_D = F["JOINT"]
    # One hex: its six edges, meadow_base's border points, and anchors along the six corner bearings
    # 10 m out (`build` puts anchor n at CENTRES[1 + n] pushed 6 m further out from the origin).
    M.SEGS = [(hexgeo.corner((k - 1) % 6), hexgeo.corner(k)) for k in range(6)]
    M.RING = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
    M.SPAN = 2.0 * max(max(abs(x), abs(y)) for x, y in M.RING) + 0.2
    M.CENTRES = [(0.0, 0.0)] + [(4.0 * math.cos(math.radians(30.0 + 60.0 * n)),
                                 4.0 * math.sin(math.radians(30.0 + 60.0 * n))) for n in range(6)]
    M.outline_dist = hex_dist
    M.flank = no_flank
    M.classify = classify
    M.granite_shader = granite_shader
    M.tri_noise = tri_noise
    if piece == "hill_meadow":
        M.slope_h = M.grass_h = hill_grass
        M.mass = hill_mass
        # Each apron's bearing and half angle, for the massif's `mass` clefts and `fan_lift` wedges.
        M.FANS = tuple((math.degrees(math.atan2(F["OUTCROPS"][k][1], F["OUTCROPS"][k][0])), half, length)
                       for k, half, length in F["APRONS"])
        M.FAN_APEX = hill_apexes()
    else:
        M.TOPS, M.SPINES, M.SPINE_E0, M.FANS = F["TOPS"], F["SPINES"], F["SPINE_E0"], F["FANS"]
        M.SUMMIT_H = F["SUMMIT_H"]
        M.CLEFT_DEPTH, M.CLEFT_SIGMA = F["CLEFT"]
        M.slope_h = M.grass_h = peak_grass
        M.mass = peak_mass
        M.FAN_APEX = peak_apexes()
    M._VOLUME.clear()


def build(piece, out, generator):
    """`_massif.build` for `piece` into the stage dir `out`, then the export report's KTX2 budget class
    set to `piece` (the massif's build writes `hero`; a base piece is a piece-class family, spec D7)."""
    configure(piece)
    M.build(out, generator)
    rep_path = pathlib.Path(out) / "export-report.json"
    rep = json.loads(rep_path.read_text())
    rep["budgetClass"] = "piece"
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
