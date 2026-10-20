"""Hay bales for the farm (refs: assets-source/refs/farm, the round and rectangular bales in the yard).
Scripted route (PRD §11.1), prop kind. Shared by hay_bale_round.py and hay_bale_stack.py.

Both pieces bake one unique straw atlas (albedo through EMIT, tangent normal, AO with a ground plane
as occluder into ORM.R, roughness into ORM.G) from an object-space procedural straw on a high poly.
Straw is not foliage: no wind, no alpha cut, no season tint. masks: R snow 1, G porosity 0.7,
B 0, A 1 (the unique ORM carries the AO).

round: a net-wrapped round bale lying on its side, a lathe about Blender X built at three
resolutions from one analytic profile (flat-domed caps, rounded shoulders, a slight barrel bulge),
squashed a little where it rests and sunk 5 cm. Every LOD's UVs are analytic: the barrel is four
90-degree sectors that share one island, and both caps share one island (mirrored), so LOD1/LOD2
sample LOD0's atlas exactly and the 1024² albedo holds about 400 px/m. The bake receiver is the top
sector and one cap only, so the overlapping islands take one bake (contact AO never lands on top).
Shared islands put a seam at every sector boundary and round each cap, so (Q55, 2026-10-20):
- the straw and the high's relief are SECTORS-fold periodic about the axis (`periodic_point`, the
  shader's circle embedding of SECTORS * theta, and a net with a whole number of lines per sector),
  so a sector island's u = 0 and u = 1 edges carry the same straw;
- the receiver keeps the full LOD0's vertex normals on its open edges (`receiver_of`); an open copy's
  edge normals lean half a column into the kept faces, and the bake wrote the opposite lean into the
  normal map's island edges (a 10 degree shading step at every sector seam);
- round bakes lifted past the AO distance (BAKE_LIFT), as round_b does, so the cap island takes no
  ground-contact AO that the shared barrel texels cannot match at the rim.

stack: five small square bales (three below, two on top), each a bevelled box with a bulge, built
low (one chamfer) and high (rounded, lumpy). Faces hidden inside the stack are dropped before one
smart-project atlas; LOD1/LOD2 decimate LOD0 and keep its UVs (rock_small's recipe)."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector, noise
from mathutils.bvhtree import BVHTree

from lib import bake, budgets, collider, export, geom, lod, materials, scene, uv

ATLAS, SMALL, MASK_SIZE = 1024, 512, 128
CAGE, MAX_RAY, MARGIN, SAMPLES, AO_DIST = 0.04, 0.1, 8, 32, 0.3
SNOW, POROSITY = 1.0, 0.7

# Round bale profile (m): radius, half width, shoulder radius, cap dome, barrel bulge, rest squash, sink.
R, W, B, DOME, BULGE, SQUASH, SINK = 0.75, 0.6, 0.15, 0.025, 0.02, 0.9, 0.05
RC = R - B  # cap boundary radius (shoulder start)
ZC = -SINK + R * SQUASH  # axis height after the squash and sink
SECTORS = 4
SECTOR_LEN = 2 * math.pi * R / SECTORS
PROFILE_LEN = B * math.pi + 2 * (W - B)
GUTTER = 0.012
UV_K = (1 - 3 * GUTTER) / (SECTOR_LEN + 2 * RC)  # UV units per metre (barrel and cap islands side by side)
NET_LINES = 11  # net lines per sector and family: a whole number, so the net meets itself at the seams
NET_SPACING = SECTOR_LEN / NET_LINES  # 0.107 m (0.11 m gave 10.7 lines and a zig-zag at every seam)
FIBRE_K = 3.5 * R / SECTORS  # fibre circle radius: 3.5 noise units per metre of arc, as before
CAP_BLEND = 0.15  # the cap's free (non-periodic) straw fades in this far inside the cap rim
BAKE_LIFT = 0.5  # round's bake lift over bake_maps' ground occluder, past AO_DIST
assert BAKE_LIFT > AO_DIST


def periodic_point(x: float, y: float, z: float) -> Vector:
    """The point the round straw's noises read at object point (x, y, z): the lathe's angle theta about
    the axis (y, z - ZC) is embedded as a circle of SECTORS * theta at radius r / SECTORS (true arc
    length), so every field repeats each sector and a sector island's two edges match. Inside the
    caps it fades to the plain point (axis-relative) over CAP_BLEND, so the cap faces keep free,
    non-repeating straw. Mirrors the node graph in `straw_material` (ZC is read at call time)."""
    zr = z - ZC
    r = math.hypot(y, zr)
    phi = SECTORS * math.atan2(y, zr)
    e = Vector((x, r / SECTORS * math.cos(phi), r / SECTORS * math.sin(phi)))
    t = min(max((RC - r) / CAP_BLEND, 0.0), 1.0)
    w = t * t * (3 - 2 * t)
    return e.lerp(Vector((x, y, zr)), w)


# ---------------------------------------------------------------------------------------------- shader

class _Nodes:
    def __init__(self, mat: bpy.types.Material):
        self.nt = materials.ensure_nodes(mat)
        self.N, self.L = self.nt.nodes, self.nt.links
        self.bsdf = next(n for n in self.N if n.type == "BSDF_PRINCIPLED")
        self.tc = self.N.new("ShaderNodeTexCoord")

    def link(self, a, b):
        self.L.new(a, b)

    def tex(self, kind, vec, scale, detail=2.0, **kw):
        t = self.N.new(kind)
        t.inputs["Scale"].default_value = scale
        if "Detail" in t.inputs:
            t.inputs["Detail"].default_value = detail
        for k, v in kw.items():
            if k in t.inputs:
                t.inputs[k].default_value = v
            else:
                setattr(t, k, v)
        self.link(vec, t.inputs["Vector"])
        return t

    def rng(self, src, a, b, lo=0.0, hi=1.0, interp="LINEAR"):
        r = self.N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.inputs["To Min"].default_value, r.inputs["To Max"].default_value = lo, hi
        r.clamp = True
        r.interpolation_type = interp
        self.link(src, r.inputs["Value"])
        return r.outputs["Result"]

    def mix_vec(self, fac, a, b):
        n = self.N.new("ShaderNodeMix")
        n.data_type = "VECTOR"
        ins = {s.identifier: s for s in n.inputs}
        self.link(fac, ins["Factor_Float"])
        self.link(a, ins["A_Vector"])
        self.link(b, ins["B_Vector"])
        return next(s for s in n.outputs if s.identifier == "Result_Vector")

    def math(self, op, a, b=None):
        n = self.N.new("ShaderNodeMath")
        n.operation = op
        for i, x in enumerate((a, b)):
            if x is None:
                continue
            if isinstance(x, float):
                n.inputs[i].default_value = x
            else:
                self.link(x, n.inputs[i])
        return n.outputs[0]

    def mix(self, fac, a, b):
        n = self.N.new("ShaderNodeMix")
        n.data_type = "RGBA"
        ins = {s.identifier: s for s in n.inputs}
        if isinstance(fac, float):
            ins["Factor_Float"].default_value = fac
        else:
            self.link(fac, ins["Factor_Float"])
        for key, x in (("A_Color", a), ("B_Color", b)):
            if isinstance(x, tuple):
                ins[key].default_value = x
            else:
                self.link(x, ins[key])
        return next(s for s in n.outputs if s.identifier == "Result_Color")

    def xyz(self, vec):
        s = self.N.new("ShaderNodeSeparateXYZ")
        self.link(vec, s.inputs[0])
        return s.outputs["X"], s.outputs["Y"], s.outputs["Z"]

    def combine(self, x, y, z):
        c = self.N.new("ShaderNodeCombineXYZ")
        for sock, v in zip(c.inputs, (x, y, z)):
            if isinstance(v, float):
                sock.default_value = v
            else:
                self.link(v, sock)
        return c.outputs[0]


def straw_material(name: str, kind: str, tone: float = 0.0, coords: str | None = None) -> bpy.types.Material:
    """Object-space procedural straw. kind 'round' (axis along object X, centre at z = ZC: circumferential
    fibres and a diamond net on the barrel, rolled rings on the caps) or 'square' (bale-local frame,
    long axis X, bottom at z = 0: fibres across, flake gaps along X, fibre-end speckle on the ends, two
    orange twines over the top and ends). tone in [0, 1] weathers the straw toward grey-brown.
    `coords` names a point vector attribute read instead of the object coordinates (round_b's lathe
    frame before its slump, so the periodic fields stay periodic on the leaning bale)."""
    m = bpy.data.materials.new(name)
    n = _Nodes(m)
    c = materials.srgb
    obj = n.tc.outputs["Object"]
    if coords:
        attr = n.N.new("ShaderNodeAttribute")
        attr.attribute_name = coords
        obj = attr.outputs["Vector"]
    px, py, pz = n.xyz(obj)
    nx, ny, nz = n.xyz(n.tc.outputs["Normal"])
    nvec = obj
    if kind == "round":
        # The SECTORS-fold periodic point of `periodic_point`: (x, r/S cos S.theta, r/S sin S.theta),
        # fading to the plain axis-relative point inside the caps.
        zr = n.math("SUBTRACT", pz, ZC)
        theta = n.math("ARCTAN2", py, zr)
        phi = n.math("MULTIPLY", theta, float(SECTORS))
        cphi, sphi = n.math("COSINE", phi), n.math("SINE", phi)
        rad = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", py, py), n.math("MULTIPLY", zr, zr)))
        rq = n.math("DIVIDE", rad, float(SECTORS))
        wcap = n.rng(rad, RC, RC - CAP_BLEND, interp="SMOOTHSTEP")
        nvec = n.mix_vec(wcap, n.combine(px, n.math("MULTIPLY", rq, cphi), n.math("MULTIPLY", rq, sphi)),
                         n.combine(px, py, zr))
    mottle = n.tex("ShaderNodeTexNoise", nvec, 4.0, 3.0)
    if kind == "round":
        lo, hi, dark, light = c("#b98f45"), c("#d3b064"), c("#8c6a2e"), c("#e6cf8c")
        arc = n.math("MULTIPLY", theta, R)
        fibre_vec = n.combine(n.math("MULTIPLY", px, 55.0), n.math("MULTIPLY", cphi, FIBRE_K),
                              n.math("MULTIPLY", sphi, FIBRE_K))
        fibre = n.tex("ShaderNodeTexNoise", fibre_vec, 1.0, 6.0, Roughness=0.65)
        cap = n.rng(n.math("ABSOLUTE", nx), 0.55, 0.85)
        barrel = n.math("SUBTRACT", 1.0, cap)
    else:
        lo, hi, dark, light = c("#c3a055"), c("#dcc27a"), c("#9a7a38"), c("#eedc9e")
        sq = n.N.new("ShaderNodeVectorMath")
        sq.operation = "MULTIPLY"
        sq.inputs[1].default_value = (70.0, 5.0, 5.0)
        n.link(obj, sq.inputs[0])
        fibre_vec = sq.outputs[0]
        fibre = n.tex("ShaderNodeTexNoise", fibre_vec, 1.0, 6.0, Roughness=0.65)
        cap = n.rng(n.math("ABSOLUTE", nx), 0.6, 0.8)  # the bale's cut ends
        barrel = n.math("SUBTRACT", 1.0, cap)
    col = n.mix(n.rng(mottle.outputs["Fac"], 0.35, 0.65), lo, hi)
    fibre_dark = 0.75 if kind == "round" else 0.4
    col = n.mix(n.math("MULTIPLY", n.rng(fibre.outputs["Fac"], 0.5, 0.32), fibre_dark), col, dark)
    col = n.mix(n.math("MULTIPLY", n.rng(fibre.outputs["Fac"], 0.58, 0.78), 0.55), col, light)
    height = n.math("MULTIPLY", fibre.outputs["Fac"], 1.0)
    rough = n.math("ADD", n.math("MULTIPLY", fibre.outputs["Fac"], 0.08), 0.82)
    speck = n.tex("ShaderNodeTexNoise", nvec, 140.0, 1.0)
    ends = n.math("MULTIPLY", cap, n.rng(speck.outputs["Fac"], 0.42, 0.34))
    if kind == "round":
        # Caps: the rolled layers as distorted rings about the axis. Blender's Wave RINGS at scale 9,
        # distortion 3, detail 3 (0.5 - 0.5 cos(180 r + 3 (2 N - 1)), N the fBm at 9 p), built from
        # nodes so the distortion noise reads the periodic point near the rim.
        ring_vec = n.mix_vec(wcap, n.combine(0.0, n.math("MULTIPLY", rq, cphi), n.math("MULTIPLY", rq, sphi)),
                             n.combine(0.0, py, zr))
        dist = n.tex("ShaderNodeTexNoise", ring_vec, 9.0, 3.0, Roughness=0.5)
        ring_n = n.math("ADD", n.math("MULTIPLY", rad, 180.0),
                        n.math("MULTIPLY", n.math("SUBTRACT", n.math("MULTIPLY", dist.outputs["Fac"], 2.0), 1.0), 3.0))
        rings = n.math("SUBTRACT", 0.5, n.math("MULTIPLY", n.math("COSINE", ring_n), 0.5))
        ring_dark = n.math("MULTIPLY", cap, n.rng(rings, 0.25, 0.0))
        col = n.mix(n.math("MULTIPLY", ring_dark, 0.7), col, c("#7d5f28"))
        height = n.math("ADD", height, n.math("MULTIPLY", ring_dark, -0.6))
        # Barrel: a pale diamond net, two families of helical lines NET_SPACING apart (a whole number
        # per sector, so they meet across the sector seams).
        lines = []
        for sgn in (1.0, -1.0):
            t = n.math("MULTIPLY", n.math("ADD", px, n.math("MULTIPLY", arc, sgn)), 1.0 / NET_SPACING)
            d = n.math("ABSOLUTE", n.math("SUBTRACT", n.math("FRACT", t), 0.5))
            lines.append(n.rng(d, 0.455, 0.485))
        net = n.math("MULTIPLY", n.math("MAXIMUM", *lines), barrel)
        col = n.mix(n.math("MULTIPLY", net, 0.6), col, c("#ece6d2"))
        rough = n.math("SUBTRACT", rough, n.math("MULTIPLY", net, 0.2))
        height = n.math("ADD", height, n.math("MULTIPLY", net, 0.5))
        dirt_z = n.rng(pz, 0.10, -0.02)
    else:
        # Flakes: soft gaps across the bale every ~9 cm along X.
        flakes = n.tex("ShaderNodeTexWave", obj, 3.5, 2.0, wave_type="BANDS", bands_direction="X",
                       Distortion=1.5)
        gap = n.math("MULTIPLY", barrel, n.rng(flakes.outputs["Fac"], 0.08, 0.0))
        col = n.mix(n.math("MULTIPLY", gap, 0.35), col, c("#8a6a2c"))
        height = n.math("ADD", height, n.math("MULTIPLY", gap, -0.8))
        # Twine: two strings at y = +-0.13 over the top, the bottom and the ends (not the long sides).
        off = n.math("ABSOLUTE", n.math("SUBTRACT", n.math("ABSOLUTE", py), 0.13))
        twine = n.math("MULTIPLY", n.rng(off, 0.009, 0.005), n.rng(n.math("ABSOLUTE", ny), 0.75, 0.55))
        col = n.mix(twine, col, c("#b4602c"))
        rough = n.math("SUBTRACT", rough, n.math("MULTIPLY", twine, 0.25))
        height = n.math("ADD", height, n.math("MULTIPLY", twine, 0.4))
        dirt_z = n.rng(pz, 0.06, -0.01)
    col = n.mix(n.math("MULTIPLY", ends, 0.8), col, c("#6e5424"))
    col = n.mix(n.math("MULTIPLY", dirt_z, 0.55), col, c("#6b5532"))
    if tone > 0:
        col = n.mix(tone * 0.35, col, c("#a99a74"))
    n.link(col, n.bsdf.inputs["Base Color"])
    n.link(n.math("MINIMUM", rough, 0.93), n.bsdf.inputs["Roughness"])
    bump = n.N.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.35
    bump.inputs["Distance"].default_value = 0.006
    n.link(height, bump.inputs["Height"])
    n.link(bump.outputs["Normal"], n.bsdf.inputs["Normal"])
    return m


# ------------------------------------------------------------------------------------------ round bale

def _ring(spec):
    """(x, r, s) of a profile ring. spec: ('c', side) cap centre, ('cap', side, r), ('sh', side, phi
    degrees, 0 = cap boundary), ('bar', x). s is the barrel island's profile coordinate (None on caps)."""
    kind = spec[0]
    if kind == "c":
        return spec[1] * (W + DOME), 0.0, None
    if kind == "cap":
        side, r = spec[1], spec[2]
        return side * (W + DOME * (1 - (r / RC) ** 2)), r, None
    if kind == "sh":
        side, phi = spec[1], math.radians(spec[2])
        x, r = side * (W - B + B * math.cos(phi)), RC + B * math.sin(phi)
        s = B * phi if side < 0 else B * math.pi / 2 + 2 * (W - B) + B * (math.pi / 2 - phi)
        return x, r, s
    x = spec[1]
    return x, R + BULGE * (1 - (x / (W - B)) ** 2), B * math.pi / 2 + x + (W - B)


def round_rings(level: str) -> list[tuple]:
    """Ring specs from the -x cap centre to the +x cap centre."""
    if level == "high":
        caps = [("cap", -1, r) for r in np.linspace(0, RC, 14)[1:-1]]
        sh = [("sh", -1, p) for p in np.linspace(0, 90, 12)[:-1]]
        bar = [("bar", x) for x in np.linspace(-(W - B), W - B, 26)]
    elif level == "lod0":
        caps, sh, bar = [("cap", -1, 0.3)], [("sh", -1, 0.0), ("sh", -1, 45.0)], [("bar", -(W - B)), ("bar", 0.0),
                                                                                      ("bar", W - B)]
    elif level == "lod1":
        caps, sh, bar = [], [("sh", -1, 0.0), ("sh", -1, 45.0)], [("bar", -(W - B)), ("bar", 0.0), ("bar", W - B)]
    else:
        caps, sh, bar = [], [("sh", -1, 0.0)], [("bar", -(W - B)), ("bar", W - B)]
    a = [("c", -1)] + caps + sh
    return a + bar + [(s[0], 1, *s[2:]) for s in reversed(a)]


def _rest(y: float, z: float) -> tuple[float, float]:
    """Squash the lower half where the bale rests, then sink it (z from the axis)."""
    return y, ZC + (z * SQUASH if z < 0 else z)


def round_bale(name: str, level: str, cols: int, relief: float = 0.0, seed: int = 0) -> bpy.types.Object:
    """The lathe at one resolution, with analytic UVs (module docstring). Faces carry an int layer
    'island': 0..3 barrel sector, 4 cap -x, 5 cap +x."""
    specs = round_rings(level)
    rings = [_ring(s) for s in specs]
    ca = next(i for i, s in enumerate(specs) if s[0] == "sh" and s[1] < 0 and s[2] == 0.0)
    cb = next(i for i, s in enumerate(specs) if s[0] == "sh" and s[1] > 0 and s[2] == 0.0)
    step = 360.0 / cols
    bm = bmesh.new()
    isl = bm.faces.layers.int.new("island")
    uvl = bm.loops.layers.uv.new("UVMap")
    grid = []
    for i, (x, r, _s) in enumerate(rings):
        if r == 0.0:
            y, z = _rest(0.0, 0.0)
            grid.append([bm.verts.new((x, y, z))])
            continue
        row = []
        for j in range(cols):
            t = math.radians(45.0 + j * step)
            y, z = _rest(r * math.sin(t), r * math.cos(t))
            row.append(bm.verts.new((x, y, z)))
        grid.append(row)
    centre = Vector((0.0, 0.0, ZC))

    def uv_of(i, theta_deg, island):
        x, r, s = rings[i]
        if island < SECTORS:
            u = (theta_deg - 45.0 - 90.0 * island) / 90.0
            return GUTTER + u * SECTOR_LEN * UV_K, GUTTER + s * UV_K
        side = -1.0 if island == SECTORS else 1.0
        t = math.radians(theta_deg)
        y, z = side * r * math.sin(t), r * math.cos(t)
        return 2 * GUTTER + SECTOR_LEN * UV_K + (y + RC) * UV_K, GUTTER + (z + RC) * UV_K

    for i in range(len(rings) - 1):
        for j in range(cols):
            th = [45.0 + j * step, 45.0 + (j + 1) * step]
            jj = [j, (j + 1) % cols]
            if i >= cb:
                island = SECTORS + 1
            elif i + 1 <= ca:
                island = SECTORS
            else:
                island = int(((th[0] + th[1]) / 2 - 45.0) // 90.0)
            corners = []
            for ri, k in ((i, 0), (i, 1), (i + 1, 1), (i + 1, 0)):
                row = grid[ri]
                if len(row) == 1:
                    if not corners or corners[-1][0] is not row[0]:
                        corners.append((row[0], ri, th[k]))
                    continue
                corners.append((row[jj[k]], ri, th[k]))
            f = bm.faces.new([c[0] for c in corners])
            f[isl] = island
            f.normal_update()
            if f.normal.dot(f.calc_center_median() - centre) < 0:
                f.normal_flip()
            info = {c[0]: c for c in corners}
            for loop in f.loops:
                _v, ri, t = info[loop.vert]
                loop[uvl].uv = uv_of(ri, t, island)
    if relief:
        bm.normal_update()
        noise.seed_set(seed)
        off = Vector((11.3, -4.1, 7.7))
        moved = [(v, v.normal * relief * noise.noise(periodic_point(*v.co) * 3.5 + off)) for v in bm.verts]
        for v, d in moved:
            v.co += d
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return scene.link(bpy.data.objects.new(name, me))


# -------------------------------------------------------------------------------------- square bales

def bale(name: str, size: tuple[float, float, float], bevel: float, segments: int, cuts: tuple[int, int, int],
         relief: float = 0.0, seed: int = 0) -> bpy.types.Object:
    """One square bale in its local frame (long axis X, bottom centre at the origin): a box cut
    `cuts` times along each axis, its box edges bevelled, its sides and top bulged."""
    L, Wd, H = size
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((v.co.x * L, v.co.y * Wd, (v.co.z + 0.5) * H))
    for axis, n in enumerate(cuts):
        if n:
            ext = size[axis]
            es = [e for e in bm.edges if abs(abs(e.verts[0].co[axis] - e.verts[1].co[axis]) - ext) < 1e-6]
            bmesh.ops.subdivide_edges(bm, edges=es, cuts=n, use_grid_fill=True)
    bm.normal_update()
    sharp = [e for e in bm.edges if len(e.link_faces) == 2 and e.calc_face_angle(0.0) > 0.5]
    bmesh.ops.bevel(bm, geom=sharp + list({v for e in sharp for v in e.verts}), offset=bevel,
                    offset_type="OFFSET", segments=segments, profile=0.5, affect="EDGES", clamp_overlap=True)
    for v in bm.verts:
        fx = max(0.0, 1 - (2 * v.co.x / L) ** 2)
        v.co.y *= 1 + 0.05 * fx
        v.co.z += 0.018 * fx * (v.co.z / H)
    if relief:
        bm.normal_update()
        noise.seed_set(seed)
        off = Vector((seed * 3.1, 2.7, -5.3))
        moved = [(v, v.normal * relief * noise.noise(v.co * 6.0 + off)) for v in bm.verts if v.co.z > 0.01]
        for v, d in moved:
            v.co += d
    bmesh.ops.triangulate(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = False
    return scene.link(bpy.data.objects.new(name, me))


def stack_layout(seed: str) -> list[tuple[tuple[float, float, float], Matrix]]:
    """Five bales: three side by side below, two on top over the gaps. (size, local-to-world)."""
    rng = random.Random(seed)
    H = 0.37
    out = []
    for row, ys, z in ((0, (-0.47, 0.0, 0.47), -0.03), (1, (-0.235, 0.235), H - 0.06)):
        for y in ys:
            size = (rng.uniform(0.93, 0.99), rng.uniform(0.47, 0.49), H)
            yaw = math.radians(rng.uniform(-3.5, 3.5) if row == 0 else rng.uniform(-6, 6))
            loc = Vector((rng.uniform(-0.05, 0.05), y + rng.uniform(-0.01, 0.01), z))
            out.append((size, Matrix.Translation(loc) @ Matrix.Rotation(yaw, 4, "Z")))
    return out


def world_bvhs(objs: list) -> list:
    """BVH trees of the (closed) objects in world space."""
    out = []
    for o in objs:
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bm.transform(o.matrix_world)
        out.append(BVHTree.FromBMesh(bm))
        bm.free()
    return out


def drop_hidden(obj: bpy.types.Object, others: list, depth: float = 0.004) -> None:
    """Deletes the faces of `obj` that nobody sees: ground faces (facing down at the ground) and faces
    whose every vertex lies more than `depth` inside another closed bale (BVH trees in world space).
    A face straddling a neighbour's surface stays, so the grooves between bales stay closed."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.transform(obj.matrix_world)
    bm.normal_update()

    def inside(p):
        for t in others:
            hit, nrm, _i, _d = t.find_nearest(p)
            if hit is not None and (p - hit).dot(nrm) < -depth:
                return True
        return False

    drop = [f for f in bm.faces if (f.normal.z < -0.9 and f.calc_center_median().z < 0.05)
            or all(inside(v.co) for v in f.verts)]
    bmesh.ops.delete(bm, geom=drop, context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.transform(obj.matrix_world.inverted())
    bm.to_mesh(obj.data)
    bm.free()


# ------------------------------------------------------------------------------------------ bake + io

def _island_mask(obj: bpy.types.Object, size: int) -> np.ndarray:
    """Texels (Blender pixel order) inside obj's UV triangles."""
    isl = np.zeros((size, size), bool)
    me = obj.data
    uvd = me.uv_layers["UVMap"].data
    for poly in me.polygons:
        pts = np.array([uvd[li].uv[:] for li in poly.loop_indices]) * size
        for k in range(1, len(pts) - 1):
            tri = pts[[0, k, k + 1]]
            lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
            xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = tri
            d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(d) < 1e-12:
                continue
            l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
            l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
            inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
            isl[ys[inside].astype(int).clip(0, size - 1), xs[inside].astype(int).clip(0, size - 1)] = True
    return isl


def _dilate(m: np.ndarray, n: int) -> np.ndarray:
    out = m.copy()
    for _ in range(n):
        g = out.copy()
        g[1:] |= out[:-1]
        g[:-1] |= out[1:]
        g[:, 1:] |= out[:, :-1]
        g[:, :-1] |= out[:, 1:]
        out = g
    return out


def _down(px: np.ndarray, size: int, out: int, renormal: bool = False) -> np.ndarray:
    f = size // out
    a = px.reshape(out, f, out, f, 4).mean(axis=(1, 3))
    if renormal:
        nv = a[..., :3] * 2.0 - 1.0
        nv /= np.maximum(np.linalg.norm(nv, axis=-1, keepdims=True), 1e-6)
        a[..., :3] = nv * 0.5 + 0.5
    a[..., 3] = 1.0
    return a.reshape(-1, 4).astype(np.float32)


def bake_maps(piece: str, out: pathlib.Path, highs: list, receiver: bpy.types.Object, skip: bool) -> dict:
    """Bakes the straw onto `receiver` and writes <piece>.{albedo (1024), normal, orm (512), masks (128)}.png.
    Texels outside the islands and their bake margin take the island mean (normal: flat), so deep
    mips keep the straw's colour. Returns bake stats."""
    n_px = ATLAS * ATLAS
    albedo = np.tile(np.array([*materials.srgb("#c8a65c")[:3], 1.0], np.float32), (n_px, 1))
    normal = np.tile(np.array([0.5, 0.5, 1.0, 1.0], np.float32), (n_px, 1))
    orm = np.tile(np.array([1.0, 0.86, 0.0, 1.0], np.float32), (n_px, 1))
    stats = {}
    if not skip:
        bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new(f"{piece}_world")
        bake.setup_cycles(samples=SAMPLES)
        kw = dict(cage_extrusion=CAGE, max_ray=MAX_RAY, margin=MARGIN)
        imgs = {k: bake.new_image(f"{piece}.{k}", ATLAS, non_color=k != "albedo") for k in ("albedo", "normal", "ao", "rough")}
        bake.bake_emit_input(highs, receiver, "Base Color", imgs["albedo"], **kw)
        bake.bake_to(receiver, "NORMAL", imgs["normal"], highs=highs, **kw)
        bake.bake_to(receiver, "ROUGHNESS", imgs["rough"], highs=highs, **kw)
        gme = bpy.data.meshes.new("ground_occluder")
        gme.from_pydata([(-4, -4, 0), (4, -4, 0), (4, 4, 0), (-4, 4, 0)], [], [[0, 1, 2, 3]])
        ground = scene.link(bpy.data.objects.new("ground_occluder", gme))
        ground.data.materials.append(highs[0].data.materials[0])
        bpy.context.scene.world.light_settings.distance = AO_DIST
        bake.bake_to(receiver, "AO", imgs["ao"], highs=[*highs, ground], **kw)
        bpy.data.objects.remove(ground, do_unlink=True)
        bpy.context.view_layer.update()
        px = {}
        for k, im in imgs.items():
            p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
            px[k] = bake.load_png(p)[1]
            os.remove(p)
        isl = _island_mask(receiver, ATLAS)
        flat = isl.ravel()
        cov = _dilate(isl, MARGIN - 1).ravel()
        albedo[:] = px["albedo"]
        normal[:] = px["normal"]
        orm[:, 0] = px["ao"][:, 0]
        orm[:, 1] = px["rough"][:, 0]
        for a in (albedo, normal, orm):
            a[:, 3] = 1.0
        albedo[~cov, :3] = albedo[flat, :3].mean(0)
        orm[~cov, :2] = orm[flat, :2].mean(0)
        normal[~cov] = (0.5, 0.5, 1.0, 1.0)

        def st(a):
            return {"texels": int(a.size), "min": float(a.min()), "mean": float(a.mean()),
                    "p01": float(np.percentile(a, 1)), "p05": float(np.percentile(a, 5))}

        stats = {"ao": st(orm[flat, 0]), "roughness": st(orm[flat, 1]), "albedoLuma": st(albedo[flat, :3].mean(1)),
                 "normalMissFraction": float((normal[flat, 2] < 0.05).mean()),
                 "islandFraction": float(flat.mean())}
    bake.write_png(ATLAS, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(SMALL, _down(normal, ATLAS, SMALL, renormal=True), str(out / f"{piece}.normal.png"))
    bake.write_png(SMALL, _down(orm, ATLAS, SMALL), str(out / f"{piece}.orm.png"))
    bake.make_masks(MASK_SIZE, SNOW, POROSITY, 0.0, None, str(out / f"{piece}.masks.png"))
    return stats


def finish(piece: str, out: pathlib.Path, generator: str, lods: list, texel: dict, stats: dict,
           receiver: bpy.types.Object, skip: bool, collider_kind: str = "hull", **extra) -> None:
    for o in lods:
        lod.triangulate(o)
    tris = [geom.tri_count(o) for o in lods]
    dens = [uv.measure_texel_density(o, ATLAS) for o in lods]
    zs = [v.co.z for v in lods[0].data.vertices]
    print(f"{piece}: tris per LOD {tris}, texel per LOD {dens}, z {min(zs):.3f}..{max(zs):.3f}, bake {stats}")
    budgets.check(tuple(tris), "prop", piece)
    assert texel["median"] >= 256, texel
    col = collider.collider_from(lods[-1], collider_kind, f"{piece}_collider")
    # Hull faces carry no UVs, which export zero tangents: flat-shade and map them obliquely.
    cuv = col.data.uv_layers.get("UVMap") or col.data.uv_layers.new(name="UVMap")
    for p in col.data.polygons:
        p.use_smooth = False
    for loop in col.data.loops:
        q = col.data.vertices[loop.vertex_index].co
        cuv.data[loop.index].uv = (0.31 * q.x + 0.17 * q.y + 0.07 * q.z + 0.5, 0.11 * q.x - 0.23 * q.y + 0.29 * q.z + 0.5)
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([receiver]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=MARGIN,
                           cageExtrusionM=CAGE, maxRayDistanceM=MAX_RAY, aoDistanceM=AO_DIST,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": SMALL, "colorspace": "linear"},
                                     "orm": {"size": SMALL, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, texelDensityPerLod=dens, bakeStats=stats,
                           extentZ=[min(zs), max(zs)], colliderReachM=reach, skipped=skip, **extra)


def receiver_of(src: bpy.types.Object, name: str, keep: set, lift: float = 0.0) -> bpy.types.Object:
    """A bake receiver: a copy of `src` keeping only the faces whose int layer 'island' is in `keep`,
    lifted `lift` m, with every kept corner's normal set to src's own vertex normal. An open copy's
    edge vertices would otherwise average only the kept faces and lean into them, and the tangent
    normal bake writes the opposite lean into the island's edges, which shows as a shading step
    where the full mesh's other faces sample the same island."""
    normals = [v.normal.copy() for v in src.data.vertices]
    rec = src.copy()
    rec.data = src.data.copy()
    rec.name = rec.data.name = name
    scene.link(rec)
    bm = bmesh.new()
    bm.from_mesh(rec.data)
    src_index = bm.verts.layers.int.new("src_index")
    for i, v in enumerate(bm.verts):
        v[src_index] = i
    isl = bm.faces.layers.int["island"]
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[isl] not in keep], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    orig = [v[src_index] for v in bm.verts]
    bm.verts.layers.int.remove(src_index)
    bm.transform(Matrix.Translation((0.0, 0.0, lift)))
    bm.to_mesh(rec.data)
    bm.free()
    rec.data.normals_split_custom_set_from_vertices([normals[i] for i in orig])
    for p in rec.data.polygons:
        p.use_smooth = True
    return rec


# --------------------------------------------------------------------------------------------- pieces

def build_round(piece: str, out: pathlib.Path, generator: str) -> None:
    skip = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    mat = materials.pbr_material(f"{piece}_straw", materials.srgb("#c8a65c"), 0.86)
    high = round_bale(f"{piece}_high", "high", 160, relief=0.012, seed=3)
    high.data.materials.append(straw_material(f"{piece}_high", "round"))
    lods = []
    for i, (level, cols) in enumerate((("lod0", 32), ("lod1", 12), ("lod2", 8))):
        o = round_bale(f"{piece}_lod{i}", level, cols)
        o.data.materials.append(mat)
        lod.triangulate(o)
        lods.append(o)
    texel = uv.measure_texel_density(lods[0], ATLAS)
    # Receiver: the top barrel sector and the +x cap of LOD0 (all other faces share their texels),
    # lifted with the high past the AO distance of the bake's ground occluder.
    receiver = receiver_of(lods[0], f"{piece}_receiver", {SECTORS - 1, SECTORS + 1}, BAKE_LIFT)
    high.matrix_world = Matrix.Translation((0.0, 0.0, BAKE_LIFT))
    stats = bake_maps(piece, out, [high], receiver, skip)
    sig_obj = receiver
    bpy.data.objects.remove(high, do_unlink=True)
    bpy.context.view_layer.update()
    for o in lods:
        if "island" in o.data.attributes:
            o.data.attributes.remove(o.data.attributes["island"])
    finish(piece, out, generator, lods, texel, stats, sig_obj, skip,
           profile={"radiusM": R, "halfWidthM": W, "shoulderM": B, "sinkM": SINK, "squash": SQUASH})
    bpy.data.objects.remove(sig_obj, do_unlink=True)


def build_stack(piece: str, out: pathlib.Path, generator: str, lod0_tris: int = 520) -> None:
    skip = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    layout = stack_layout(piece)
    rng = random.Random(piece + "tone")
    highs, lows = [], []
    for k, (size, m) in enumerate(layout):
        h = bale(f"{piece}_high{k}", size, 0.05, 3, (14, 7, 6), relief=0.006, seed=k + 1)
        h.matrix_world = m
        h.data.materials.append(straw_material(f"{piece}_high{k}", "square", tone=rng.random()))
        highs.append(h)
        lo = bale(f"{piece}_part{k}", size, 0.04, 2, (3, 1, 1))
        lo.matrix_world = m
        lows.append(lo)
    bpy.context.view_layer.update()
    trees = world_bvhs(lows)
    for k, lo in enumerate(lows):
        drop_hidden(lo, [t for j, t in enumerate(trees) if j != k])
    lod0 = geom.join(lows, f"{piece}_lod0")
    lod0.data.materials.clear()
    lod0.data.materials.append(materials.pbr_material(f"{piece}_straw", materials.srgb("#c8a65c"), 0.86))
    for p in lod0.data.polygons:
        p.use_smooth = False
    lod.decimate_to(lod0, lod0_tris)
    uv.atlas_unwrap([lod0], margin=0.008)
    texel = uv.measure_texel_density(lod0, ATLAS)
    stats = bake_maps(piece, out, highs, lod0, skip)
    for h in highs:
        bpy.data.objects.remove(h, do_unlink=True)
    bpy.context.view_layer.update()
    lods = [lod0]
    t0 = geom.tri_count(lod0)
    t1, t2 = budgets.lod_targets(t0, "prop", piece)
    for i, target in ((1, int(t1 * 0.92)), (2, int(t2 * 0.92))):
        o = lod0.copy()
        o.data = lod0.data.copy()
        o.name = o.data.name = f"{piece}_lod{i}"
        scene.link(o)
        lod.decimate_to(o, target)
        lods.append(o)
    finish(piece, out, generator, lods, texel, stats, lod0, skip,
           bales=[{"size": list(s), "at": list(m.translation)} for s, m in layout])
