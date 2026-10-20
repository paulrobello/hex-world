"""house_twostorey_b: a jettied, hip-roofed timber-framed house, the house_twostorey variant for
house_cluster's house 2 slot (PRD §6 Variety; owner-approved style, assets-source/refs/house_cluster
ref-1 and ref-2, "including the timber-framed English cottage style"): cream lime render between dark
oak framing, the upper storey close-studded and jettied 0.35 m over the ground floor on the front, a
brown clay-tile hipped roof with its courses running round all four slopes, a red-brick stack with two
pots on the short ridge over the door, small-paned oak casements and a plank door under the jetty.
Scripted route; the builder and its slot contract are in _house_jettied.py (on _house.py's and
_house_longhouse.py's parts).

Plan (Blender x deep, y wide): the upper storey 5.0 x 7.0 m (x ±2.5, y ±3.5), the ground floor's front
set back to x 2.15 under the jetty; jetty at 2.55 m, eaves 4.9 m, 47° hips, ridge 2 m long. The door
faces +x (yaw 0) at y 0, so its step reaches house_pad's two-storey doorstep."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_jettied  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "house_twostorey_b"
WALLS, ROOF, STONE, BRICK, TIMBER, FRAME, DOOR, GLASS = (
    f"{P}_{n}" for n in ("walls", "roof", "stone", "brick", "timber", "frame", "door", "glass"))
# Framing heights (m): sole band on the plinth, the first-floor girt (the bressumer on the front, under
# the jetty's upper wall), the wall plate under the eaves, and the ground-floor front's window-head rail.
# Posts and studs run to the bands' centrelines, inside the prouder bands, so their ends never show.
SOLE = SOLE_TOP = 0.41
JET = 2.55
GIRT = GIRT_TOP = 2.66
PLATE = PLATE_BOT = 4.54
RAIL = 2.15
BAND, POST, STUD, BRACE = 0.06, 0.05, 0.04, 0.035  # how far each kind of member stands proud
SPEC = {
    "piece": P,
    "front_lo": 2.15, "front_hi": 2.5, "back": -2.5, "half_w": 3.5, "jetty": JET, "eave": 4.9,
    "pitch": 47.0, "overhang": 0.25, "courses": 10, "kick": 0.03, "roof_thick": 0.14,
    "chimney": (0.0, 0.0), "chimney_size": (0.6, 0.95), "chimney_above": 0.9,
    "apron_top": 0.07,  # flagstones under the jetty, 2 cm over house_pad's gravel (YARD_Z 0.05)
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((1.0 * L, 0.95 * L, 0.83 * L), 0.9),
        ROOF: ((0.42 * L, 0.19 * L, 0.11 * L), 0.85),
        STONE: ((0.45 * L, 0.42 * L, 0.37 * L), 0.9),
        BRICK: ((0.46 * L, 0.16 * L, 0.09 * L), 0.9),
        TIMBER: ((0.07, 0.048, 0.033), 0.8),
        FRAME: ((0.12, 0.075, 0.042), 0.75),
        DOOR: ((0.16, 0.1, 0.05), 0.7),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), ROOF: (1.0, 0.35, 0.0), STONE: (1.0, 0.4, 0.0), BRICK: (1.0, 0.45, 0.0),
              TIMBER: (1.0, 0.6, 0.0), FRAME: (1.0, 0.5, 0.0), DOOR: (1.0, 0.5, 0.0), GLASS: (0.0, 0.05, 1.0)},
    "slots": {"walls": WALLS, "roof": ROOF, "stone": STONE, "brick": BRICK, "timber": TIMBER, "frame": FRAME,
              "door": DOOR, "glass": GLASS, "sill": STONE, "pots": ROOF},
    # (u, width, height) on the ground-floor front, whose u runs along +y.
    "door": (0.0, 1.0, 2.05),
    # Casements: (wall, u, sill z, width, height, major). Walls: FL ground-floor front and FU upper front
    # (u = y), B back (u = -y), SL left hip end (u = x), SR right hip end (u = -x). Major windows keep
    # their frame at LOD1 and their sill and bars at LOD0.
    "windows": [("FL", -2.0, 0.85, 1.2, 1.0, True), ("FL", 2.0, 0.85, 1.2, 1.0, True),
                ("FU", -2.0, 3.15, 1.1, 1.0, True), ("FU", 2.0, 3.15, 1.1, 1.0, True),
                ("FU", 0.0, 3.3, 0.6, 0.8, False),
                ("B", -1.6, 0.9, 0.9, 0.9, False), ("B", 1.6, 0.9, 0.9, 0.9, False),
                ("B", -1.6, 3.2, 0.9, 0.9, False), ("B", 1.6, 3.2, 0.9, 0.9, False),
                ("SL", 1.0, 0.9, 0.8, 0.9, False), ("SR", -1.0, 0.9, 0.8, 0.9, False),
                ("SL", 1.1, 3.2, 0.8, 0.9, False), ("SR", -1.1, 3.2, 0.8, 0.9, False)],
    # Framing: (wall, u0, z0, u1, z1, kept at LOD1, width, proud); wall "S" is both hip ends (u = x).
    # Bands run past each corner by their proud so the two walls' bands close it; corner posts reach
    # out to the corner the same way.
    "timbers": [
        # Ground-floor front: the sole either side of the door, corner posts, posts beside the door and
        # window-head rails between them (the posts end on the jetty's underside).
        ("FL", -3.56, SOLE, -0.6, SOLE, True, 0.16, BAND), ("FL", 0.6, SOLE, 3.56, SOLE, True, 0.16, BAND),
        ("FL", -3.46, SOLE_TOP, -3.46, JET, True, 0.18, POST), ("FL", 3.46, SOLE_TOP, 3.46, JET, True, 0.18, POST),
        ("FL", -1.0, SOLE_TOP, -1.0, JET, True, 0.18, POST), ("FL", 1.0, SOLE_TOP, 1.0, JET, True, 0.18, POST),
        ("FL", -3.37, RAIL, -1.09, RAIL, False, 0.14, 0.045), ("FL", 1.09, RAIL, 3.37, RAIL, False, 0.14, 0.045),
        # Upper front: the bressumer on the jetty, the wall plate and corner posts (studs below).
        ("FU", -3.56, GIRT, 3.56, GIRT, True, 0.22, BAND), ("FU", -3.56, PLATE, 3.56, PLATE, True, 0.18, BAND),
        ("FU", -3.46, GIRT_TOP, -3.46, PLATE_BOT, True, 0.18, POST),
        ("FU", 3.46, GIRT_TOP, 3.46, PLATE_BOT, True, 0.18, POST),
        # Back: sole, girt and plate, corner and middle posts on both storeys, upper braces.
        ("B", -3.56, SOLE, 3.56, SOLE, True, 0.16, BAND), ("B", -3.56, GIRT, 3.56, GIRT, True, 0.22, BAND),
        ("B", -3.56, PLATE, 3.56, PLATE, True, 0.18, BAND),
        ("B", -3.46, SOLE_TOP, -3.46, JET, True, 0.18, POST), ("B", 3.46, SOLE_TOP, 3.46, JET, True, 0.18, POST),
        ("B", -3.46, GIRT_TOP, -3.46, PLATE_BOT, True, 0.18, POST),
        ("B", 3.46, GIRT_TOP, 3.46, PLATE_BOT, True, 0.18, POST),
        ("B", 0.0, SOLE_TOP, 0.0, JET, True, 0.18, POST), ("B", 0.0, GIRT_TOP, 0.0, PLATE_BOT, True, 0.18, POST),
        ("B", -3.44, 2.95, -2.5, 4.52, False, 0.12, BRACE), ("B", 3.44, 2.95, 2.5, 4.52, False, 0.12, BRACE),
        ("B", -0.03, 2.95, -0.9, 4.52, False, 0.12, BRACE), ("B", 0.03, 2.95, 0.9, 4.52, False, 0.12, BRACE),
        # Hip ends: the sole to the ground floor's front, girt and plate to the jetty's front (as wide as
        # the front and back bands, so each band's open end is capped by the other's face), corner posts
        # abutting the front, upper front and back corner posts, a middle post, braces.
        ("S", -2.56, SOLE, 2.21, SOLE, True, 0.16, BAND), ("S", -2.56, GIRT, 2.56, GIRT, True, 0.22, BAND),
        ("S", -2.56, PLATE, 2.56, PLATE, True, 0.18, BAND),
        ("S", -2.41, SOLE_TOP, -2.41, JET, True, 0.18, POST), ("S", 2.06, SOLE_TOP, 2.06, JET, True, 0.18, POST),
        ("S", -2.41, GIRT_TOP, -2.41, PLATE_BOT, True, 0.18, POST),
        ("S", 2.41, GIRT_TOP, 2.41, PLATE_BOT, True, 0.18, POST),
        ("S", -0.2, SOLE_TOP, -0.2, JET, True, 0.16, POST), ("S", -0.2, GIRT_TOP, -0.2, PLATE_BOT, True, 0.16, POST),
        ("S", -2.38, 2.95, -1.45, 4.52, False, 0.12, BRACE), ("S", -0.21, 2.95, -1.15, 4.52, False, 0.12, BRACE),
    ],
    # Studs: (wall, u positions, z0, z1, width, proud, kept at LOD1 when unclipped), clipped round every
    # opening on the wall. The upper front is close-studded, as the refs' timber-framed fronts.
    "studs": [
        ("FL", (-3.0, 3.0), SOLE_TOP, RAIL, 0.12, STUD, ()),
        ("FU", (-2.86, -2.42, -1.98, -1.54, -1.1, -0.66, -0.22, 0.22, 0.66, 1.1, 1.54, 1.98, 2.42, 2.86),
         GIRT_TOP, PLATE_BOT, 0.12, STUD, (-2.86, -1.1, -0.66, 0.66, 1.1, 2.86)),
    ],
}

_house_jettied.build_piece(SPEC, __file__)
