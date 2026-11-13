"""outcrop_granite_b: a second whaleback granite outcrop (ref-2), about 8 x 4.8 m and 1.3 m high, so
that two outcrops in one view no longer show the same fracture pattern (outcrop_granite redo, open
question Q2: "every outcrop is the same mesh, rotated and scaled"). Built ahead of owner review in
outcrop_granite's material family, scale class, pipeline (`_boulder.build`) and budgets (refs:
assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a runtime
scatter prop drawn as outcrop_granite's variant: make kit-build PIECE=outcrop_granite_b.

What differs from outcrop_granite:
- Silhouette and mass. Its own seed (the piece id) gives other cleave planes, and `warp` bends the
  plan into a kidney, tapers a low -x tail and lifts a blunt +x end with a saddle behind it, so the
  high point sits off centre instead of on a centred crest.
- Fractures. A curved transverse joint cuts a raised slab off the high end (ref-2's stepped front
  slab), a long joint runs from it down the tail, a short branch leaves that toward the -y rim, and a
  hairline lies near the +y rim, all from their own `SEED`.

The rest is outcrop_granite's redo code, copied because it lives in that generator rather than in a
shared `_*.py` helper and that generator must stay byte-identical while it is under review; once both
are approved the shared code can move into a `_outcrop.py` helper. As there, this file replaces
`_boulder.shape_high` and wraps `_boulder.sparser_moss` (module globals `_boulder.build` looks up), so
`_boulder.py`, `_rock.py` and the other pieces are unchanged:
- A `Fracture` wanders from `a` toward `b` (a seeded heading walk), its width, depth and slab step
  vary along it, each end fades by its own length, and its lips are rounded and ragged. A branch
  leaves its parent obliquely. No two run parallel and none crosses the stone.
- Only edges near a fracture are subdivided further, so a narrow groove keeps a clean edge without
  a sawtooth, and the rest of the high poly is unchanged.
- Nothing below z = 0.3 m moves (`Z_FLOOR`), so the stone's ground slice keeps its fit (grass
  `ROCK_BASES`, scatter `FOOTPRINTS`).
- A low swell (`SWELL`) breaks the flat cleave facets of the top.
- Lichen varies: pale sage rosettes and ochre crusts in clusters under low-frequency density fields,
  over `granite_material`'s small rosettes, whose density now varies too; fracture floors darken."""
import math
import pathlib
import random
import sys
from dataclasses import dataclass

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import bmesh  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector, noise  # noqa: E402

import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401
from lib import materials  # noqa: E402

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
SEED = "outcrop_granite_b-fractures"
Z_FLOOR = (0.30, 0.60)  # carving weight rises from 0 to 1 over this z band (m)
REFINE = (0.30, 0.10)  # extra subdivision passes, each for edges within this distance (m) of a groove
SWELL = 0.04  # top swell amplitude (m)
SEMI_X = 3.95  # the stone's long semi-axis (m)
# `warp`, with xn = x / SEMI_X: y -> y (1 + TAPER xn) + BEND (xn^2 - 0.35) (a kidney plan, wide at +x),
# and height above ground times 1 + RISE xn - DIP exp(-((xn + 0.15) / 0.35)^2) (a high +x end, a saddle).
BEND, TAPER, RISE, DIP = 0.55, 0.22, 0.28, 0.08
CRACK = "fracture"  # high-poly point attribute: 1 on a fracture floor (darkens the albedo bake)


@dataclass
class Fracture(Joint):
    """A wandering sheet joint (`Joint` keeps a, b, depth, width, step and the step's lateral `fade`,
    so the bake report records it). `parent` >= 0 starts it on that earlier fracture's path at
    fraction `at` (`a` is then where it lands). `ends` are the fade lengths at a and b as fractions
    of the length; `wobble` the heading noise per 0.12 m step (rad). `bevel` is where the high side's
    step reaches full height, in groove half-widths from the path: wider rolls the slab edge down into
    the joint instead of standing a lip beside it. The step holds its full height out to `hold` m from
    the path before fading over `fade`, so a slab stays level instead of peaking at its edge."""
    wobble: float = 0.18
    ends: tuple = (0.15, 0.25)
    parent: int = -1
    at: float = 0.5
    bevel: float = 1.5
    hold: float = 0.0


FRACTURES = [
    # The main joint: a curved cut across the high +x end, ending inside both rims. Its +x side (left
    # of a->b) is ref-2's raised front slab: the step holds level across the slab (`hold`, `fade`), and
    # its edge rolls down into the joint over 3.2 half-widths (`bevel`), so it reads as a drop off a
    # fractured slab, not a ridge beside the groove (pass 2, coordinator 2026-11-13).
    Fracture(a=(1.45, 2.10), b=(2.10, -1.95), depth=0.13, width=0.12, step=0.09, fade=4.0,
             wobble=0.22, ends=(0.16, 0.12), bevel=3.2, hold=1.8),
    # A long joint from the main one down the low tail, ending inside the -x rim.
    Fracture(a=(0.0, 0.0), b=(-3.05, 0.55), depth=0.10, width=0.10, step=0.045, fade=1.8,
             wobble=0.17, ends=(0.0, 0.26), parent=0, at=0.46),
    # A short branch off the long joint toward the -y rim.
    Fracture(a=(0.0, 0.0), b=(-0.95, -1.70), depth=0.065, width=0.07, step=0.03, fade=1.0,
             wobble=0.20, ends=(0.0, 0.38), parent=1, at=0.40),
    # A hairline near the +y rim, dying out at both ends.
    Fracture(a=(-1.70, 1.25), b=(0.15, 1.75), depth=0.035, width=0.04, step=0.0, fade=1.0,
             wobble=0.22, ends=(0.30, 0.30)),
]


def smoothstep(e0, e1, x):
    """Hermite step from e0 to e1 (scalars or arrays); 1 everywhere where e0 == e1."""
    e0, e1, x = (np.asarray(v, float) for v in (e0, e1, x))
    flat = e1 == e0
    t = np.where(flat, 1.0, np.clip((x - e0) / np.where(flat, 1.0, e1 - e0), 0.0, 1.0))
    return t * t * (3.0 - 2.0 * t)


@dataclass
class Path:
    pts: np.ndarray  # (m, 2) Blender x, y
    t: np.ndarray  # (m,) arc-length fraction of each point
    width: np.ndarray  # (m,) multipliers of the fracture's peak values
    depth: np.ndarray
    step: np.ndarray


def wander(a: np.ndarray, b: np.ndarray, rng: random.Random, wobble: float, seg: float = 0.12) -> np.ndarray:
    """A heading walk from a that wanders freely in its middle and is steered home to b near its
    end; about every twelfth step kinks."""
    n = max(8, int(round(np.linalg.norm(b - a) / seg)))
    step = np.linalg.norm(b - a) / n
    p = a.copy()
    h = math.atan2(b[1] - a[1], b[0] - a[0]) + rng.gauss(0.0, 0.12)
    pts = [p.copy()]
    for i in range(n):
        want = math.atan2(b[1] - p[1], b[0] - p[0])
        miss = (want - h + math.pi) % (2.0 * math.pi) - math.pi
        pull = 0.16 + 0.5 * (i / n) ** 2
        h += pull * miss + rng.gauss(0.0, wobble * (3.0 if rng.random() < 0.08 else 1.0))
        p = p + step * np.array((math.cos(h), math.sin(h)))
        pts.append(p.copy())
    return np.array(pts)


def plan(fractures: list) -> list:
    """Each fracture's path and its along-length profiles (seeded, so the same every build); a
    branch's `a` is set to its start on the parent."""
    rng = random.Random(SEED)
    paths = []
    for f in fractures:
        if f.parent >= 0:
            par = paths[f.parent]
            f.a = tuple(round(float(c), 3) for c in (
                np.interp(f.at, par.t, par.pts[:, 0]), np.interp(f.at, par.t, par.pts[:, 1])))
        pts = wander(np.array(f.a, float), np.array(f.b, float), rng, f.wobble)
        seg = np.linalg.norm(np.diff(pts, axis=0), axis=1)
        t = np.concatenate(([0.0], np.cumsum(seg))) / seg.sum()

        def prof(lo: float, k: int = 7) -> np.ndarray:
            return np.interp(t, np.linspace(0.0, 1.0, k), [rng.uniform(lo, 1.0) for _ in range(k)])

        paths.append(Path(pts, t, prof(0.45), prof(0.40), prof(0.25)))
    return paths


PATHS = plan(FRACTURES)


def poly_dist(p: np.ndarray, path: Path) -> tuple:
    """(distance, arc fraction t, side: +1 left of the path's direction) from each of p (n, 2)."""
    a, b = path.pts[:-1], path.pts[1:]
    ab = b - a
    l2 = (ab ** 2).sum(1)
    seg = np.sqrt(l2)
    cum = np.concatenate(([0.0], np.cumsum(seg)))
    d = np.empty(len(p))
    t = np.empty(len(p))
    side = np.empty(len(p))
    for s in range(0, len(p), 4096):
        q = p[s:s + 4096, None, :]
        u = np.clip(((q - a) * ab).sum(-1) / l2, 0.0, 1.0)
        d2 = ((q - (a + u[..., None] * ab)) ** 2).sum(-1)
        k = d2.argmin(1)
        i = np.arange(len(k))
        d[s:s + 4096] = np.sqrt(d2[i, k])
        t[s:s + 4096] = (cum[k] + u[i, k] * seg[k]) / cum[-1]
        r = q[:, 0, :] - a[k]
        side[s:s + 4096] = np.sign(ab[k, 0] * r[:, 1] - ab[k, 1] * r[:, 0])
    return d, t, side


def _refine(bm: bmesh.types.BMesh, reach: float) -> None:
    """Subdivides every edge above the carving floor whose midpoint lies within a groove's width
    plus `reach` of its path."""
    bm.verts.index_update()
    co = np.array([v.co[:] for v in bm.verts])
    edges = list(bm.edges)
    ev = np.array([(e.verts[0].index, e.verts[1].index) for e in edges])
    mid = 0.5 * (co[ev[:, 0]] + co[ev[:, 1]])
    up = np.minimum(co[ev[:, 0], 2], co[ev[:, 1], 2]) > Z_FLOOR[0] - 0.05
    near = np.zeros(len(edges), bool)
    for f, path in zip(FRACTURES, PATHS):
        d, _, _ = poly_dist(mid[:, :2], path)
        near |= d < f.width * 1.5 + reach
    sel = [e for e, k in zip(edges, near & up) if k]
    bmesh.ops.subdivide_edges(bm, edges=sel, cuts=1, use_grid_fill=True)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])


def warp(bm: bmesh.types.BMesh) -> None:
    """Bends, tapers and lifts the stone (`BEND`, `TAPER`, `RISE`, `DIP`); heights scale only above
    ground, so the z = 0 plane stays where it is."""
    for v in bm.verts:
        x, y, z = v.co
        xn = max(-1.2, min(1.2, x / SEMI_X))
        y = y * (1.0 + TAPER * xn) + BEND * (xn * xn - 0.35)
        if z > 0.0:
            z *= 1.0 + RISE * xn - DIP * math.exp(-((xn + 0.15) / 0.35) ** 2)
        v.co = Vector((x, y, z))


def shape_high(high, lump: float, size: float, joints: list) -> None:
    """Replaces `_boulder.shape_high` for this piece: the plan and mass warp, then outcrop_granite's
    subdivision, lump, near-fracture refinement, top swell and fractures (module docstring)."""
    assert list(joints) == FRACTURES, "outcrop_granite_b carves its own FRACTURES"
    bm = bmesh.new()
    bm.from_mesh(high.data)
    warp(bm)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=1, use_grid_fill=True)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    for reach in REFINE:
        _refine(bm, reach)
    bm.normal_update()
    off = Vector((13.7, -4.1, 8.3))
    for v in bm.verts:
        v.co += v.normal * lump * noise.noise(v.co * (1.6 / size) + off)
    bm.normal_update()
    verts = list(bm.verts)
    co = np.array([v.co[:] for v in verts])
    nrm = np.array([v.normal[:] for v in verts])
    up = smoothstep(Z_FLOOR[0], Z_FLOOR[1], co[:, 2])
    swell = np.array([noise.noise(Vector((x * 0.8 - 5.3, y * 0.8 + 2.9, 4.4))) for x, y, _ in co])
    rag = np.array([noise.noise(Vector((x * 4.5 + 1.7, y * 4.5 - 3.2, 2.7))) for x, y, _ in co])
    groove = np.zeros(len(co))
    lift = np.zeros(len(co))
    crack = np.zeros(len(co))
    for f, path in zip(FRACTURES, PATHS):
        d, t, side = poly_dist(co[:, :2], path)
        w = f.width * np.interp(t, path.t, path.width)
        fade = smoothstep(0.0, f.ends[0], t) * smoothstep(0.0, f.ends[1], 1.0 - t)
        # The slab step always tapers at both ends, so it never runs on past a branch's start.
        lift_fade = smoothstep(0.0, max(f.ends[0], 0.12), t) * smoothstep(0.0, f.ends[1], 1.0 - t)
        dd = np.maximum(0.0, d + 0.45 * w * rag)
        g = f.depth * np.interp(t, path.t, path.depth) * fade * np.clip(1.0 - (dd / w) ** 2, 0.0, 1.0) ** 2
        groove = np.maximum(groove, g)
        lift += (f.step * np.interp(t, path.t, path.step) * lift_fade * (side > 0)
                 * smoothstep(0.25 * w, f.bevel * w, dd) * (1.0 - smoothstep(f.hold, f.fade, d)))
        crack = np.maximum(crack, fade * (1.0 - smoothstep(0.55 * w, 1.05 * w, dd)))
    co -= nrm * (groove * up)[:, None]
    co[:, 2] += (lift + SWELL * swell) * up
    for v, c in zip(verts, co):
        v.co = Vector(c)
    bm.to_mesh(high.data)
    bm.free()
    attr = high.data.attributes.new(CRACK, "FLOAT", "POINT")
    attr.data.foreach_set("value", (crack * up).astype(np.float32))
    for p in high.data.polygons:
        p.use_smooth = True


def vary_lichen(mat) -> None:
    """Adds the varied lichen and the fracture dirt to `_rock.granite_material`'s base colour."""
    nt = mat.node_tree
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    tc = next(n for n in N if n.type == "TEX_COORD")
    geo = next(n for n in N if n.type == "NEW_GEOMETRY")
    base = bsdf.inputs["Base Color"].links[0].from_socket
    moss = bsdf.inputs["Emission Color"].links[0].from_socket
    c = materials.srgb

    def tex(kind, scale, vec=None, detail=2.0):
        t = N.new(kind)
        t.inputs["Scale"].default_value = scale
        if "Detail" in t.inputs:
            t.inputs["Detail"].default_value = detail
        L.new(vec if vec is not None else tc.outputs["Object"], t.inputs["Vector"])
        return t

    def rng_(src, a, b, lo=0.0, hi=1.0):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.inputs["To Min"].default_value, r.inputs["To Max"].default_value = lo, hi
        r.clamp = True
        L.new(src, r.inputs["Value"])
        return r.outputs["Result"]

    def math_(op, a, b):
        n = N.new("ShaderNodeMath")
        n.operation = op
        for i, x in enumerate((a, b)):
            if isinstance(x, float):
                n.inputs[i].default_value = x
            else:
                L.new(x, n.inputs[i])
        return n.outputs[0]

    def mix(fac, a, b):
        n = N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        L.new(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                L.new(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    def sep(vec, axis):
        s = N.new("ShaderNodeSeparateXYZ")
        L.new(vec, s.inputs[0])
        return s.outputs[axis]

    # granite_material's small rosettes: their density follows a low-frequency field.
    lich_n = [n for n in N if n.type == "TEX_NOISE" and abs(n.inputs["Scale"].default_value - 2.5) < 1e-6]
    assert len(lich_n) == 1, len(lich_n)
    lr = [lk.to_node for lk in lich_n[0].outputs["Fac"].links if lk.to_node.type == "MAP_RANGE"]
    assert len(lr) == 1, len(lr)
    dens = tex("ShaderNodeTexNoise", 0.45)
    lo = rng_(dens.outputs["Fac"], 0.38, 0.62, 0.30, 0.50)
    L.new(lo, lr[0].inputs["From Min"])
    L.new(math_("ADD", lo, 0.14), lr[0].inputs["From Max"])

    z = sep(tc.outputs["Object"], "Z")
    above_soil = rng_(z, 0.04, 0.14)
    floor = N.new("ShaderNodeAttribute")
    floor.attribute_name = CRACK
    crack = floor.outputs["Fac"]
    open_rock = math_("MULTIPLY", math_("MULTIPLY", math_("SUBTRACT", 1.0, moss), above_soil),
                      math_("SUBTRACT", 1.0, crack))
    # The moss of granite_material (sparser_moss's cut) thins and thickens across the stone too.
    mr = [n for n in N if n.type == "MAP_RANGE"
          and abs(n.inputs["From Min"].default_value - _boulder.MOSS_CUT[0]) < 1e-6
          and abs(n.inputs["From Max"].default_value - _boulder.MOSS_CUT[1]) < 1e-6]
    assert len(mr) == 1, len(mr)
    mlo = rng_(tex("ShaderNodeTexNoise", 0.38).outputs["Fac"], 0.38, 0.62, 0.47, 0.62)
    L.new(mlo, mr[0].inputs["From Min"])
    L.new(math_("ADD", mlo, 0.13), mr[0].inputs["From Max"])

    def warp(src, scale, amount):
        wn = tex("ShaderNodeTexNoise", scale, src)
        wv = N.new("ShaderNodeVectorMath")
        wv.operation = "MULTIPLY_ADD"
        L.new(wn.outputs["Color"], wv.inputs[0])
        wv.inputs[1].default_value = (amount, amount, amount)
        L.new(src, wv.inputs[2])
        return wv.outputs[0]

    # Warped lookups: a broad warp moves the colonies, a fine one lobes every edge.
    warped = warp(warp(tc.outputs["Object"], 2.2, 0.22), 9.0, 0.14)

    # Pale sage foliose rosettes, 0.12-0.25 m across, in colonies (ref-2).
    vor = tex("ShaderNodeTexVoronoi", 2.6, warped, detail=0.0)
    cell = N.new("ShaderNodeSeparateColor")
    L.new(vor.outputs["Color"], cell.inputs["Color"])
    radius = math_("ADD", math_("MULTIPLY", cell.outputs["Red"], 0.17), 0.16)
    rosette = rng_(math_("SUBTRACT", vor.outputs["Distance"], radius), 0.03, -0.03)
    keep = rng_(cell.outputs["Green"], 0.35, 0.41)
    cluster = rng_(tex("ShaderNodeTexNoise", 0.55).outputs["Fac"], 0.45, 0.56)
    sage_m = math_("MULTIPLY", math_("MULTIPLY", rosette, keep), math_("MULTIPLY", cluster, open_rock))
    sage = mix(cell.outputs["Blue"], c("#92a083"), c("#aab498"))

    # Ochre crustose patches on the flanks, irregular, sparse.
    crust = rng_(tex("ShaderNodeTexNoise", 6.0, warped, detail=4.0).outputs["Fac"], 0.57, 0.63)
    nz = sep(geo.outputs["Normal"], "Z")
    flank = math_("MULTIPLY", rng_(nz, 0.90, 0.62), rng_(nz, 0.02, 0.18))
    crust_cl = rng_(tex("ShaderNodeTexNoise", 0.8).outputs["Fac"], 0.48, 0.58)
    ochre_m = math_("MULTIPLY", math_("MULTIPLY", crust, flank), math_("MULTIPLY", crust_cl, open_rock))
    ochre = mix(rng_(tex("ShaderNodeTexNoise", 3.0).outputs["Fac"], 0.4, 0.6), c("#8e6c34"), c("#a8843e"))

    col = mix(math_("MULTIPLY", crack, 0.30), base, c("#4f4a42"))
    col = mix(math_("MULTIPLY", sage_m, 0.92), col, sage)
    col = mix(math_("MULTIPLY", ochre_m, 0.85), col, ochre)
    L.new(col, bsdf.inputs["Base Color"])


_sparser_moss = _boulder.sparser_moss


def sparser_moss(mat, cut) -> None:
    _sparser_moss(mat, cut)
    vary_lichen(mat)


_boulder.shape_high = shape_high
_boulder.sparser_moss = sparser_moss

_boulder.build("outcrop_granite_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(SEMI_X, 2.75, 1.36), centre=(0.0, 0.0, -0.15), planes=13, keep=(0.85, 0.95),
                  top_el=(1.25, 1.5), tilt=(0.07, -0.05))],
    bury=-0.32, lod_tris=(1398, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.10, joints=tuple(FRACTURES))
