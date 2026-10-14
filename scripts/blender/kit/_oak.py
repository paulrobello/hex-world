"""tree_hero_oak P2 polish: oak-only geometry on top of the shared `_tree` helpers (which other trees
hash as generatorDeps, so they stay byte-frozen). Blender frame, Z up, trunk base at the origin.

- Skeleton: `_tree`'s twisted three-strand trunk and crown layout, with limbs that leave the trunk
  lower and thicker so they show under the crown, and heavier, longer surface roots (ref-1).
- Domes: the baked master is the smooth R0 ellipsoid (no SECTORS-fold lumps, so the 120-degree bake
  repeat carries no low-frequency feature to spot), and every clump's shell gets its own seeded,
  non-symmetric lumps in geometry (`clump_lump`). Shell normals are the lumped surface's analytic
  normals per clump (`shell_normals`), so decimated LODs shade as smooth domes, not facets.
- Shell faces wholly inside a neighbouring clump are culled (`cull_shell_hidden`) to pay for a
  denser shell ring, which keeps dome silhouettes round at an 8 m close-up."""
from __future__ import annotations

import math
import random

import bmesh
import bpy
from mathutils import Matrix, Vector

import _leaves
import _tree

PIECE = _tree.PIECE
SEED = _tree.SEED
R0, DOME_WIDE, DOME_TOP, DOME_BASE, SECTORS = _tree.R0, _tree.DOME_WIDE, _tree.DOME_TOP, _tree.DOME_BASE, _tree.SECTORS
# Shell rings as (normal elevation psi in degrees, columns per sector), poles added. Rings sit at
# equal steps of the dome's NORMAL angle, not of latitude: the flattened ellipsoid turns 90 degrees
# within 0.8 m at its rim, so equal latitudes left 46-48 degree silhouette corners there. Columns
# taper toward the poles, where a column spans cos(psi) less normal angle, keeping every facet near
# a 15-20 degree normal step; the rings within +-36 degrees are the dome rims.
SHELL_RINGS = ((-67.5, 3), (-45, 4), (-22.5, 7), (0, 7), (22.5, 7), (45, 5), (67.5, 3))
SHELL_COLUMNS = tuple(c for _, c in SHELL_RINGS)
SHELL_PHIS = tuple(round(math.atan((DOME_TOP if p >= 0 else DOME_BASE) / DOME_WIDE * math.tan(math.radians(p))), 4)
                   for p, _ in SHELL_RINGS)  # ring latitudes (rad) on the master ellipsoid
CLUMP_ATTR = "oak_clump"  # point attribute on shell vertices: the clump index (float)


def skeleton(seed: str = SEED) -> list[dict]:
    """`_tree.skeleton` with oak P2 limbs and roots. Same layout, trunk strands and branch order."""
    rng = random.Random(seed)
    crown = _tree.layout(seed)
    out: list[dict] = []
    for i in range(3):  # trunk strands: as `_tree.skeleton` (owner-approved twist)
        pts, radii = [], []
        for k in range(41):
            t = k / 40
            a = 2 * math.pi * i / 3 + 3.6 * t
            r = 0.36 + 0.95 * (1 - t) ** 3
            lean = 0.2 * math.sin(math.pi * t * 0.9)
            pts.append(Vector((r * math.cos(a) + lean, r * math.sin(a) + 0.1 * lean, -0.3 + 4.0 * t)))
            radii.append(0.5 * (1 - 0.3 * t) + 0.32 * (1 - t) ** 4)
        out.append({"level": 0, "points": pts, "radii": radii, "sway": (0.0, _tree.SWAY_TRUNK)})
    # Surface roots, heavier than P1 (ref-1): they leave the strands higher, stay thick for longer,
    # reach further and sink below the meadow only at the tip.
    for i in range(8):
        a = 2 * math.pi * (i + rng.uniform(-0.25, 0.25)) / 8
        reach = rng.uniform(2.4, 3.2) * 1.15
        pts = []
        for k in range(21):
            t = k / 20
            r = 0.5 + (reach - 0.5) * t ** 1.05
            w = 0.18 * math.sin(3 * t + i)
            pts.append(Vector((r * math.cos(a + w), r * math.sin(a + w), 1.6 * (1 - t) ** 2.0 - 0.22 * t)))
        out.append({"level": 0, "points": _tree._smooth(pts), "radii": _tree._lerp_radius(0.7, 0.13, 21, 0.6),
                    "sway": (0.0, 0.0)})
    # Limbs leave the trunk at 2.6-3.0 m and run out nearly level before rising into their clump,
    # so their length shows under the crown; thicker than P1.
    limbs = []
    for cl in [c for c in crown if c["tier"] == 0]:
        c = cl["centre"]
        a = math.atan2(c.y, c.x)
        start = Vector((0.3 * math.cos(a), 0.3 * math.sin(a), 2.6 + rng.uniform(0, 0.4)))
        end = c - Vector((0, 0, 0.5))
        pts = _tree._arc(rng, start, end, 0.15, 0.14)
        # Ease the rise toward the tip: the first half stays low (ref-1's spreading limbs).
        z0, z1 = start.z, end.z
        for k, p in enumerate(pts):
            t = k / (len(pts) - 1)
            p.z = z0 + (z1 - z0) * t ** 1.8 + (p.z - (z0 + (z1 - z0) * t))
        limbs.append(pts)
        out.append({"level": 1, "points": pts, "radii": _tree._lerp_radius(0.5, 0.17, len(pts), 0.8),
                    "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    for cl in [c for c in crown if c["tier"] == 1]:
        c = cl["centre"]
        best = min(limbs, key=lambda pts: (pts[len(pts) // 2] - c).length)
        t = 0.45
        base = best[int(t * (len(best) - 1))]
        s2 = _tree.SWAY_TRUNK + (_tree.SWAY_LIMB - _tree.SWAY_TRUNK) * t
        out.append({"level": 2, "points": _tree._arc(rng, base, c - Vector((0, 0, 0.3)), 0.4, 0.12, 14),
                    "radii": _tree._lerp_radius(0.21, 0.08, 15), "sway": (s2, _tree.SWAY_TWIG)})
    for cl in [c for c in crown if c["tier"] == 2]:
        c = cl["centre"]
        start = Vector((0.1, 0.0, 3.4))
        out.append({"level": 1, "points": _tree._arc(rng, start, c - Vector((0, 0, 0.4)), 0.3, 0.12),
                    "radii": _tree._lerp_radius(0.36, 0.13, 21, 0.9), "sway": (_tree.SWAY_TRUNK, _tree.SWAY_LIMB)})
    for i, b in enumerate(out):
        b["id"] = i
    return out


def master_point(theta: float, phi: float) -> Vector:
    """Point on the smooth R0 master ellipsoid (no lumps: the bake has no low-frequency feature)."""
    sz = DOME_TOP if phi >= 0 else DOME_BASE
    return Vector((R0 * math.cos(phi) * math.cos(theta) * DOME_WIDE, R0 * math.cos(phi) * math.sin(theta) * DOME_WIDE,
                   R0 * math.sin(phi) * sz))


def _master_normal(theta: float, phi: float) -> Vector:
    p = master_point(theta, phi)
    sz = DOME_TOP if phi >= 0 else DOME_BASE
    return Vector((p.x / DOME_WIDE**2, p.y / DOME_WIDE**2, p.z / sz**2)).normalized()


def lump_terms(cl: dict) -> list[tuple[float, int, float, float]]:
    """Seeded relief terms (amplitude, theta frequency, phi frequency, phase) for one clump. The
    theta frequencies 1, 2, 4 and 5 share no factor with SECTORS, so no lump repeats per sector."""
    rng = random.Random(f"{SEED}:clump_lump:{cl['id']}")
    return [(amp, k, rng.uniform(-2.0, 2.0), rng.uniform(0, 2 * math.pi))
            for amp, k in ((0.05, 1), (0.06, 2), (0.05, 4), (0.035, 5))]


def clump_lump(terms: list, theta: float, phi: float) -> float:
    """Radial scale - 1 of a clump's shell at (theta, phi); fades to 0 at the poles."""
    fade = math.cos(phi) ** 0.5 if abs(phi) < math.pi / 2 else 0.0
    return fade * sum(a * math.cos(k * theta + f * phi + ph) for a, k, f, ph in terms)


def clump_surface(terms: list, theta: float, phi: float) -> Vector:
    return master_point(theta, phi) * (1.0 + clump_lump(terms, theta, phi))


def angles(q: Vector) -> tuple[float, float]:
    """(theta, phi) of a point on (a radially scaled copy of) the master ellipsoid, clump-local."""
    theta = math.atan2(q.y, q.x)
    rho = math.hypot(q.x, q.y) / DOME_WIDE
    sz = DOME_TOP if q.z >= 0 else DOME_BASE
    return theta, math.atan2(q.z / sz, rho)


def surface_normal(terms: list, theta: float, phi: float) -> Vector:
    e = 1e-3
    phi = max(-math.pi / 2 + 2 * e, min(math.pi / 2 - 2 * e, phi))
    dt = clump_surface(terms, theta + e, phi) - clump_surface(terms, theta - e, phi)
    dp = clump_surface(terms, theta, phi + e) - clump_surface(terms, theta, phi - e)
    n = dt.cross(dp)
    if n.length < 1e-9:
        return _master_normal(theta, phi)
    n.normalize()
    return n if n.dot(clump_surface(terms, theta, phi)) >= 0 else -n


def dome_sector(name: str) -> bpy.types.Object:
    """One 1/SECTORS sector of the closed low-poly R0 shell on the smooth master ellipsoid, poles
    included, outward winding. UV-unwrap it, then `_tree.replicate_sectors` to close the shell."""
    span = 2 * math.pi / SECTORS
    verts, faces, rings = [], [], []
    for phi, cols in zip(SHELL_PHIS, SHELL_COLUMNS):
        rings.append(list(range(len(verts), len(verts) + cols + 1)))
        verts += [tuple(master_point(span * j / cols, phi)) for j in range(cols + 1)]
    # Bands between rings of different column counts: zip by theta (equal counts give quad pairs).
    for lo, hi in zip(rings, rings[1:]):
        cl, ch, i, j = len(lo) - 1, len(hi) - 1, 0, 0
        while i < cl or j < ch:
            if j >= ch or (i < cl and (i + 1) * ch <= (j + 1) * cl):
                faces.append([lo[i], lo[i + 1], hi[j]])
                i += 1
            else:
                faces.append([lo[i], hi[j + 1], hi[j]])
                j += 1
    bottom, top = len(verts), len(verts) + 1
    verts += [tuple(master_point(0, -math.pi / 2)), tuple(master_point(0, math.pi / 2))]
    first, last = rings[0], rings[-1]
    for j in range(len(first) - 1):
        faces.append([first[j + 1], first[j], bottom])
    for j in range(len(last) - 1):
        faces.append([last[j], last[j + 1], top])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    for p in mesh.polygons:
        p.use_smooth = True
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def place_shell(shell: bpy.types.Object, cl: dict, name: str) -> bpy.types.Object:
    """A world-space copy of the R0 shell as clump `cl`: its own lumps, the clump transform, and the
    clump index in CLUMP_ATTR. UVs (shared bake) are untouched."""
    o = shell.copy()
    o.data = shell.data.copy()
    o.name = o.data.name = name
    terms = lump_terms(cl)
    m = _tree.clump_matrix(cl)
    for v in o.data.vertices:
        th, ph = angles(v.co)
        v.co = m @ (v.co * (1.0 + clump_lump(terms, th, ph)))
    o.data.attributes.new(CLUMP_ATTR, "FLOAT", "POINT").data.foreach_set("value", [float(cl["id"])] * len(o.data.vertices))
    o.matrix_world = Matrix.Identity(4)
    bpy.context.scene.collection.objects.link(o)
    return o


def _inside(crown: list[dict], shrink: float):
    """inside(co, skip) -> True when world point co lies inside any lumped clump dome other than
    index `skip`, scaled about its centre by `shrink`."""
    inv = [(_tree.clump_matrix(cl).inverted(), lump_terms(cl)) for cl in crown]

    def inside(co: Vector, skip: int) -> bool:
        for i, (mi, terms) in enumerate(inv):
            if i == skip:
                continue
            q = mi @ co
            th, ph = angles(q)
            if q.length < clump_surface(terms, th, ph).length * shrink:
                return True
        return False

    return inside


def cull_shell_hidden(obj: bpy.types.Object, crown: list[dict], shrink: float = 0.97) -> int:
    """Delete shell faces whose every vertex lies inside another clump's dome (never seen; frees
    triangles for a rounder shell). Needs CLUMP_ATTR. Returns the number of faces deleted."""
    inside = _inside(crown, shrink)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    lay = bm.verts.layers.float.get(CLUMP_ATTR)
    flags = {v.index: inside(v.co, int(round(v[lay]))) for v in bm.verts}
    drop = [f for f in bm.faces if all(flags[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=drop, context="FACES_ONLY")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()
    return len(drop)


def shell_normals(obj: bpy.types.Object, canopy_material: str, crown: list[dict]) -> None:
    """Custom normals on the canopy-material corners: the lumped clump surface's analytic normal at
    each vertex (CLUMP_ATTR picks the clump), so any LOD of the shell shades as a smooth dome. Other
    corners keep their current normals. Run before `_tree.set_canopy_normals`, which bends these."""
    mesh = obj.data
    cl_attr = mesh.attributes[CLUMP_ATTR].data
    idx = {i for i, m in enumerate(mesh.materials) if m and m.name == canopy_material}
    data = []
    for cl in crown:
        m = _tree.clump_matrix(cl)
        data.append((m.inverted(), m.to_3x3().inverted().transposed(), lump_terms(cl)))
    out = [tuple(c.vector) for c in mesh.corner_normals]
    for p in mesh.polygons:
        if p.material_index not in idx:
            continue
        for li in p.loop_indices:
            vi = mesh.loops[li].vertex_index
            inv, nmat, terms = data[int(round(cl_attr[vi].value))]
            th, ph = angles(inv @ mesh.vertices[vi].co)
            out[li] = tuple((nmat @ surface_normal(terms, th, ph)).normalized())
    mesh.normals_split_custom_set(out)
    mesh.update()


def cull_below(obj: bpy.types.Object, z: float) -> int:
    """Delete faces whose every vertex lies below z (buried under the meadow, whose relief never
    drops below -0.25 m). Returns the number of faces deleted."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    drop = [f for f in bm.faces if all(v.co.z < z for v in f.verts)]
    bmesh.ops.delete(bm, geom=drop, context="FACES_ONLY")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()
    return len(drop)


def hidden_texels(obj: bpy.types.Object, skel: list[dict], sides: dict[int, int], size: int):
    """Rasterize obj's UV0 at texel centres (size x size, Blender pixel order) and flag texels whose
    3D point lies inside ANOTHER branch's inscribed low tube (r cos(pi/sides)): hidden at runtime,
    they bake to near 0 and are excluded from the visible-AO statistic. Returns (island, hidden)
    bool arrays. Needs the tube_mesh face attribute tho_branch."""
    import numpy as np

    mesh = obj.data
    nf = len(mesh.polygons)
    fb = np.empty(nf, np.int64)
    mesh.attributes["tho_branch"].data.foreach_get("value", fb)
    uvd = mesh.uv_layers["UVMap"].data
    island = np.zeros((size, size), bool)
    pts, owner, pix = [], [], []
    for poly in mesh.polygons:
        li = list(poly.loop_indices)
        t = np.array([uvd[i].uv[:] for i in li]) * size
        q = np.array([mesh.vertices[mesh.loops[i].vertex_index].co[:] for i in li])
        lo_, hi_ = np.floor(t.min(0)).astype(int), np.ceil(t.max(0)).astype(int)
        xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
        (ax, ay), (bx, by), (cx, cy) = t[:3]
        d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(d) < 1e-12:
            continue
        l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
        l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
        inside = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
        if not inside.any():
            continue
        a1, a2 = l1[inside], l2[inside]
        pts.append(a1[:, None] * q[0] + a2[:, None] * q[1] + (1 - a1 - a2)[:, None] * q[2])
        owner.append(np.full(a1.size, fb[poly.index]))
        iy = ys[inside].astype(int).clip(0, size - 1)
        ix = xs[inside].astype(int).clip(0, size - 1)
        pix.append(np.stack([iy, ix], 1))
        island[iy, ix] = True
    pts, owner, pix = np.concatenate(pts), np.concatenate(owner), np.concatenate(pix)
    a, b, ra, rb, sb = [], [], [], [], []
    for br in skel:
        if br["id"] not in sides:
            continue
        k = math.cos(math.pi / sides[br["id"]])
        for i in range(len(br["points"]) - 1):
            a.append(tuple(br["points"][i]))
            b.append(tuple(br["points"][i + 1]))
            ra.append(br["radii"][i] * k)
            rb.append(br["radii"][i + 1] * k)
            sb.append(br["id"])
    a, b, ra, rb, sb = map(np.asarray, (a, b, ra, rb, sb))
    ab = b - a
    ll = np.maximum((ab * ab).sum(1), 1e-12)
    buried = np.zeros(len(pts), bool)
    for s in range(0, len(pts), 2048):
        p = pts[s:s + 2048, None, :]
        tt = np.clip(((p - a[None]) * ab[None]).sum(2) / ll[None], 0.0, 1.0)
        dd = np.linalg.norm(p - (a[None] + tt[..., None] * ab[None]), axis=2)
        rr = ra[None] + (rb - ra)[None] * tt
        buried[s:s + 2048] = ((dd < rr) & (sb[None] != owner[s:s + 2048, None])).any(1)
    hidden = np.zeros((size, size), bool)
    hidden[pix[buried, 0], pix[buried, 1]] = True
    visible = np.zeros((size, size), bool)
    visible[pix[~buried, 0], pix[~buried, 1]] = True
    return island, hidden & ~visible


def canopy_high(name: str, seed: str = SEED, per_sector: int = 480) -> bpy.types.Object:
    """The bake source, as `_tree.canopy_high` but on the smooth master ellipsoid: overlapping,
    cupped, lobed leaves over a dark solid dome at 0.97 R, SECTORS-fold symmetric like the shell.
    Leaf tone varies per leaf only (no latitude band), so no large-scale tone feature repeats per
    sector. Corner colour attribute tho_leaf_col carries per-leaf tone (linear)."""
    rng = random.Random(seed + ":canopy_high")
    outline = _leaves.outline(13)
    span = 2 * math.pi / SECTORS
    bm = bmesh.new()
    col = bm.loops.layers.float_color.new("tho_leaf_col")

    def lin(c):
        return [((x + 0.055) / 1.055) ** 2.4 if x > 0.04045 else x / 12.92 for x in c]

    leaves = []
    for _ in range(per_sector):
        theta = rng.uniform(0, span)
        phi = math.asin(rng.uniform(-1, 1))
        leaves.append((rng.random(), theta, phi))
    leaves.sort()
    under = lin([0x1f / 255, 0x38 / 255, 0x12 / 255]) + [1.0]
    cols, rows = 16, 20
    for k in range(SECTORS):
        rot = Matrix.Rotation(span * k, 3, "Z")
        grid = [[bm.verts.new(rot @ (master_point(span * j / cols, -math.pi / 2 + math.pi * i / rows) * 0.97))
                 for j in range(cols + 1)] for i in range(rows + 1)]
        for i in range(rows):
            for j in range(cols):
                f = bm.faces.new((grid[i][j], grid[i][j + 1], grid[i + 1][j + 1], grid[i + 1][j]))
                f.smooth = True
                for lp in f.loops:
                    lp[col] = under
        lr = random.Random(seed + ":canopy_leaf")
        for layer, theta, phi in leaves:
            p = master_point(theta, phi)
            n = _master_normal(theta, phi)
            down = (master_point(theta, phi - 0.05) - p).normalized()
            off = 0.02 + 0.16 * layer
            nl = (n + Vector((lr.uniform(-1, 1), lr.uniform(-1, 1), lr.uniform(-1, 1))) * 0.5).normalized()
            ax = (down - nl * down.dot(nl)).normalized()
            ax.rotate(Matrix.Rotation(lr.uniform(-math.pi, math.pi), 3, nl))
            ac = nl.cross(ax)
            length = lr.uniform(0.42, 0.55)
            centre = p + n * off + ax * length * 0.25
            tone = min(1.0, max(0.0, lr.uniform(0.2, 0.8) + 0.2 * layer))
            c = lin(list(_leaves.DARK + (_leaves.LIGHT - _leaves.DARK) * tone)) + [1.0]
            ring = []
            for s, q in outline:
                cup = 0.1 * (q / 0.33) ** 2 * length
                ring.append(bm.verts.new(rot @ (centre + ax * s * length / 2 + ac * q * 1.4 * length / 2 + nl * cup)))
            mid = bm.verts.new(rot @ (centre - nl * 0.01 * length))
            for i in range(len(ring)):
                f = bm.faces.new((mid, ring[(i + 1) % len(ring)], ring[i]))
                f.smooth = True
                for lp in f.loops:
                    lp[col] = c
    mesh = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def leaf_cards(name: str, crown: list[dict], seed: str = SEED,
               per_clump: int = _tree.FRINGE_PER_CLUMP) -> bpy.types.Object:
    """`_tree.leaf_cards` on each clump's lumped surface: lobed alpha cards half-sunk into the
    shell rims and undersides, each facing out from its clump centre. Point attribute tho_clump."""
    rng = random.Random(seed + ":fringe")
    bm = bmesh.new()
    uvl = bm.loops.layers.uv.new("UVMap")
    cl_layer = bm.verts.layers.float_vector.new("tho_clump")
    for cl in crown:
        m = _tree.clump_matrix(cl)
        terms = lump_terms(cl)
        c = cl["centre"]
        for k in range(per_clump):
            theta = 2 * math.pi * (k + rng.uniform(-0.3, 0.3)) / per_clump
            phi = rng.uniform(-1.0, 0.2)
            ctr = m @ (clump_surface(terms, theta, phi) * 0.97)
            out = (ctr - c).normalized()
            nrm = (out + Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-1, 1))) * 0.15).normalized()
            u = Vector((0, 0, 1)).cross(nrm)
            u = u.normalized() if u.length > 1e-6 else nrm.orthogonal().normalized()
            u.rotate(Matrix.Rotation(rng.uniform(-0.5, 0.5), 3, nrm))
            v = nrm.cross(u)
            h = rng.uniform(1.1, 1.5) / 2
            corners = [ctr - u * h - v * h, ctr + u * h - v * h, ctr + u * h + v * h, ctr - u * h + v * h]
            bv = [bm.verts.new(p) for p in corners]
            for x in bv:
                x[cl_layer] = c
            f = bm.faces.new(bv)
            tile = rng.randrange(_tree.LEAF_TILES)
            for loop, (s, t) in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
                s = _tree.TILE_INSET + s * (1 - 2 * _tree.TILE_INSET)
                t = _tree.TILE_INSET + t * (1 - 2 * _tree.TILE_INSET)
                loop[uvl].uv = ((tile + s) / _tree.LEAF_TILES, _tree.LEAF_V0 + t * (1 - _tree.LEAF_V0))
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj
