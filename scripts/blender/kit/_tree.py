"""Shared tree_hero_oak geometry: the seeded crown layout and skeleton, tube meshes at any resolution,
the canopy dome shells and their leaf-covered high-poly master, the fringe leaf cards, and the
materials. Used by the live authoring session (high poly, saved to the local .blend) and by the export
stage kit/tree_hero_oak.py (LOD receivers from the same layout and skeleton). Blender frame, Z up,
trunk base at the origin.

Look (owner, solid-dome canopy pass, ref-1): three strands twisting a full turn into a flared root
base with surface roots, low limbs, and a wide, flat crown of twelve sculpted dome clumps in three
tiers. Each clump is a closed low-poly shell carrying a bake of one high-poly, leaf-covered master
dome (three-fold symmetric, so the bake repeats per 120-degree sector and keeps >= 256 px/m); lobed
alpha cards only fringe the clump undersides. Bark is the tileable `bark` set on UV0 in metres."""
from __future__ import annotations

import hashlib
import json
import math
import pathlib
import random

import bmesh
import bpy
from mathutils import Matrix, Vector, noise

import _leaves

PIECE = "tree_hero_oak"
SEED = PIECE
LEAF_V0 = 0.75  # fringe-card leaf tiles live in the atlas strip v in [LEAF_V0, 1]; the shell bake below it
LEAF_TILES = _leaves.TILES  # cluster variants side by side in that strip
TILE_INSET = 0.01  # inside a tile, in tile units, so bilinear taps stay in the variant
# Canopy clump dome (the master is authored at radius R0; clumps scale it): horizontal stretch, dome
# height above the centre and the flattened underside below it, in radii; SECTORS-fold symmetry.
R0, DOME_WIDE, DOME_TOP, DOME_BASE, SECTORS = 2.0, 1.15, 0.9, 0.4, 3
# Crown tiers (count, ring radius m, centre z m, clump radius m), measured against ref-1's outline:
# crown width / tree height 1.15, crown height 0.65 of the tree, clear trunk 0.35 (task-10 report).
TIERS = ((5, 4.0, 5.1, 2.0), (4, 2.1, 7.0, 2.05), (2, 0.8, 9.0, 2.0))
FRINGE_PER_CLUMP = 7
# Wind (TEXCOORD_1, UV map "wind"): U = sway weight 0 at the trunk base to 1 at tips and outer cards,
# V = phase in [0, 1), constant per card, per branch and per clump shell. SHELL_SWAY scales a clump's
# sway on its shell (the solid domes move less than their fringe).
WIND_UV = "wind"
SWAY_TRUNK, SWAY_LIMB, SWAY_TWIG = 0.12, 0.65, 0.88
SHELL_SWAY = 0.35
REPO = pathlib.Path(__file__).resolve().parents[3]
BARK_SET = REPO / "assets/kit/p1/materials/bark"


def hash01(key: str) -> float:
    """Deterministic value in [0, 1) from a string (no RNG stream, so the skeleton never shifts)."""
    return int.from_bytes(hashlib.sha256(f"{SEED}:{key}".encode()).digest()[:4], "big") / 2**32


def _smooth(pts: list[Vector], passes: int = 2) -> list[Vector]:
    for _ in range(passes):
        pts = [pts[0]] + [(pts[i - 1] + 2 * pts[i] + pts[i + 1]) / 4 for i in range(1, len(pts) - 1)] + [pts[-1]]
    return pts


def _lerp_radius(r0: float, r1: float, n: int, power: float = 1.0) -> list[float]:
    return [r0 + (r1 - r0) * (i / (n - 1)) ** power for i in range(n)]


def _arc(rng: random.Random, start: Vector, end: Vector, bow: float, wobble: float, n: int = 20) -> list[Vector]:
    """A gnarled branch from start to end, bowed upward by `bow` metres mid-way, with seeded wobble."""
    pts = []
    for i in range(n + 1):
        t = i / n
        p = start.lerp(end, t) + Vector((0, 0, bow * 4 * t * (1 - t)))
        if 0 < i < n:
            p += Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-0.5, 0.5))) * wobble
        pts.append(p)
    return _smooth(pts)


def layout(seed: str = SEED) -> list[dict]:
    """The crown: TIERS of dome clumps as {id, tier, centre, radius, rot, stretch}."""
    rng = random.Random(seed + ":layout")
    out = []
    for tier, (count, ring, z, r) in enumerate(TIERS):
        a0 = rng.uniform(0, 2 * math.pi)
        for k in range(count):
            a = a0 + 2 * math.pi * (k + rng.uniform(-0.12, 0.12)) / count
            rr = ring * rng.uniform(0.92, 1.08)
            out.append({"id": len(out), "tier": tier,
                        "centre": Vector((rr * math.cos(a), rr * math.sin(a), z + rng.uniform(-0.5, 0.5))),
                        "radius": r * rng.uniform(0.92, 1.08), "rot": rng.uniform(0, 2 * math.pi),
                        "stretch": (rng.uniform(0.92, 1.08), rng.uniform(0.92, 1.08))})
    return out


def clump_matrix(cl: dict) -> Matrix:
    """Object matrix that places the R0 master dome (or shell) as this clump."""
    s = cl["radius"] / R0
    sx, sy = cl["stretch"]
    return (Matrix.Translation(cl["centre"]) @ Matrix.Rotation(cl["rot"], 4, "Z")
            @ Matrix.Diagonal((s * sx, s * sy, s, 1.0)))


def skeleton(seed: str = SEED) -> list[dict]:
    """Branches as {id, level, points, radii, sway}. Level 0 trunk strands and roots, 1 limbs and
    leaders (one per tier-0 clump, plus leaders to the top tier), 2 secondaries (to the middle tier).
    Branch ends sit inside their clump's dome. sway = (start, end) wind weight, inherited at forks."""
    rng = random.Random(seed)
    crown = layout(seed)
    out: list[dict] = []
    # Three strands twisting a full turn (ref-1). They overlap along the axis, so the grooves between
    # them are deep but never open, and they splay out at the base into the root flare.
    for i in range(3):
        pts, radii = [], []
        for k in range(41):
            t = k / 40
            a = 2 * math.pi * i / 3 + 3.6 * t
            r = 0.36 + 0.95 * (1 - t) ** 3
            lean = 0.2 * math.sin(math.pi * t * 0.9)
            pts.append(Vector((r * math.cos(a) + lean, r * math.sin(a) + 0.1 * lean, -0.3 + 4.0 * t)))
            radii.append(0.5 * (1 - 0.3 * t) + 0.32 * (1 - t) ** 4)
        out.append({"level": 0, "points": pts, "radii": radii, "sway": (0.0, SWAY_TRUNK)})
    # Surface roots: eight flares leave the strands about 1.3 m up and run out over the ground,
    # thinning and sinking below it at the tip, so no root end shows.
    for i in range(8):
        a = 2 * math.pi * (i + rng.uniform(-0.25, 0.25)) / 8
        reach = rng.uniform(2.4, 3.2)
        pts = []
        for k in range(21):
            t = k / 20
            r = 0.45 + (reach - 0.45) * t ** 1.05
            w = 0.18 * math.sin(3 * t + i)
            pts.append(Vector((r * math.cos(a + w), r * math.sin(a + w), 1.3 * (1 - t) ** 2.2 - 0.18 * t)))
        out.append({"level": 0, "points": _smooth(pts), "radii": _lerp_radius(0.52, 0.08, 21, 0.75),
                    "sway": (0.0, 0.0)})
    # Limbs to the low tier, secondaries from the nearest limb to the middle tier, leaders to the top.
    limbs = []
    for cl in [c for c in crown if c["tier"] == 0]:
        c = cl["centre"]
        a = math.atan2(c.y, c.x)
        start = Vector((0.25 * math.cos(a), 0.25 * math.sin(a), 3.3 + rng.uniform(0, 0.4)))
        pts = _arc(rng, start, c - Vector((0, 0, 0.4)), 0.5, 0.14)
        limbs.append(pts)
        out.append({"level": 1, "points": pts, "radii": _lerp_radius(0.4, 0.14, len(pts), 0.9),
                    "sway": (SWAY_TRUNK, SWAY_LIMB)})
    for cl in [c for c in crown if c["tier"] == 1]:
        c = cl["centre"]
        best = min(limbs, key=lambda pts: (pts[len(pts) // 2] - c).length)
        t = 0.45
        base = best[int(t * (len(best) - 1))]
        s2 = SWAY_TRUNK + (SWAY_LIMB - SWAY_TRUNK) * t
        out.append({"level": 2, "points": _arc(rng, base, c - Vector((0, 0, 0.3)), 0.4, 0.12, 14),
                    "radii": _lerp_radius(0.17, 0.07, 15), "sway": (s2, SWAY_TWIG)})
    for cl in [c for c in crown if c["tier"] == 2]:
        c = cl["centre"]
        start = Vector((0.1, 0.0, 3.6))
        out.append({"level": 1, "points": _arc(rng, start, c - Vector((0, 0, 0.4)), 0.3, 0.12),
                    "radii": _lerp_radius(0.32, 0.12, 21, 0.9), "sway": (SWAY_TRUNK, SWAY_LIMB)})
    for i, b in enumerate(out):
        b["id"] = i
    return out


def clumps(skel: list[dict] | None = None, seed: str = SEED) -> list[tuple[Vector, float]]:
    """(centre, radius) per crown clump (kept for callers that only need the spheres)."""
    return [(c["centre"], c["radius"]) for c in layout(seed)]


def _lump(theta: float, phi: float) -> float:
    """Dome surface relief, exactly SECTORS-fold symmetric in theta (only cos/sin of 3k*theta)."""
    return (0.06 * math.cos(6 * theta + 0.8 * phi) + 0.05 * math.cos(9 * theta - 1.7 * phi)
            + 0.05 * math.sin(3 * theta) * math.cos(2.5 * phi))


def dome_point(theta: float, phi: float) -> Vector:
    """Point on the R0 master dome: puffy top (DOME_TOP), flattened underside (DOME_BASE), lumpy."""
    rr = R0 * (1.0 + _lump(theta, phi))
    sz = DOME_TOP if phi >= 0 else DOME_BASE
    return Vector((rr * math.cos(phi) * math.cos(theta) * DOME_WIDE, rr * math.cos(phi) * math.sin(theta) * DOME_WIDE,
                   rr * math.sin(phi) * sz))


def _dome_normal(theta: float, phi: float) -> Vector:
    e = 1e-3
    dt = dome_point(theta + e, phi) - dome_point(theta - e, phi)
    dp = dome_point(theta, phi + e) - dome_point(theta, phi - e)
    n = dt.cross(dp).normalized()
    return n if n.dot(dome_point(theta, phi)) >= 0 else -n


SHELL_PHIS = (-1.0, -0.5, 0.0, 0.45, 0.85, 1.22)  # ring latitudes (rad) of the low shell, poles added


def dome_sector(name: str, columns: int = 5) -> bpy.types.Object:
    """One 1/SECTORS sector of the closed low-poly R0 shell (theta in [0, 2pi/SECTORS]), poles
    included, outward winding. UV-unwrap it, then `replicate_sectors` to close the shell."""
    span = 2 * math.pi / SECTORS
    verts, faces = [], []
    for phi in SHELL_PHIS:
        for j in range(columns + 1):
            verts.append(tuple(dome_point(span * j / columns, phi)))
    w = columns + 1
    for i in range(len(SHELL_PHIS) - 1):
        for j in range(columns):
            a = i * w + j
            faces.append([a, a + 1, a + 1 + w, a + w])
    bottom, top = len(verts), len(verts) + 1
    verts += [tuple(dome_point(0, -math.pi / 2)), tuple(dome_point(0, math.pi / 2))]
    last = (len(SHELL_PHIS) - 1) * w
    for j in range(columns):
        faces.append([j + 1, j, bottom])
        faces.append([last + j, last + j + 1, top])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    for p in mesh.polygons:
        p.use_smooth = True
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def replicate_sectors(obj: bpy.types.Object) -> None:
    """Close the shell: add SECTORS-1 copies of the sector rotated about Z, sharing its UVs, then
    weld the seams (positions only; UV seams stay)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    src = list(bm.faces)
    for k in range(1, SECTORS):
        rot = Matrix.Rotation(2 * math.pi * k / SECTORS, 4, "Z")
        dup = bmesh.ops.duplicate(bm, geom=src)
        verts = [g for g in dup["geom"] if isinstance(g, bmesh.types.BMVert)]
        bmesh.ops.transform(bm, matrix=rot, verts=verts)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def shell_smooth_normals(obj: bpy.types.Object) -> None:
    """Custom normals of the R0 shell = the smooth dome's own normals (ellipsoid gradient, top and
    base halves meet horizontally at the equator). Set before the bake, so the tangent-space map
    carries only the leaf relief, not the low-poly facets; the canopy bend added after the bake then
    stays smooth across faces."""
    mesh = obj.data
    out = []
    for lp in mesh.loops:
        p = mesh.vertices[lp.vertex_index].co
        sz = DOME_TOP if p.z >= 0 else DOME_BASE
        out.append(tuple(Vector((p.x / DOME_WIDE**2, p.y / DOME_WIDE**2, p.z / sz**2)).normalized()))
    mesh.normals_split_custom_set(out)
    mesh.update()


def canopy_high(name: str, seed: str = SEED, per_sector: int = 480) -> bpy.types.Object:
    """The bake source: the R0 master dome covered in overlapping, cupped, lobed oak leaves (real
    geometry, layered like shingles with tips pointing down the dome), SECTORS-fold symmetric like
    the shell. Corner colour attribute tho_leaf_col carries per-leaf tone (linear)."""
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
        sphi = rng.uniform(-1, 1)
        phi = math.asin(sphi)
        leaves.append((rng.random(), theta, phi))
    leaves.sort()  # draw order: low layers first, so later (outer) leaves overlap them
    under = lin([0x1f / 255, 0x38 / 255, 0x12 / 255]) + [1.0]  # deep leaf-shadow green
    cols, rows = 16, 20
    for k in range(SECTORS):
        rot = Matrix.Rotation(span * k, 3, "Z")
        # A dark, solid dome just under the leaves: the gaps between leaves bake to shadowed depth
        # instead of missing the high poly (ref-1's dark crevices between leaf clusters).
        grid = [[bm.verts.new(rot @ (dome_point(span * j / cols, -math.pi / 2 + math.pi * i / rows) * 0.97))
                 for j in range(cols + 1)] for i in range(rows + 1)]
        for i in range(rows):
            for j in range(cols):
                f = bm.faces.new((grid[i][j], grid[i][j + 1], grid[i + 1][j + 1], grid[i + 1][j]))
                f.smooth = True
                for lp in f.loops:
                    lp[col] = under
        lr = random.Random(seed + ":canopy_leaf")  # reseeded per sector: identical leaves in each
        for layer, theta, phi in leaves:
            p = dome_point(theta, phi)
            n = _dome_normal(theta, phi)
            down = (dome_point(theta, phi - 0.05) - p).normalized()
            off = 0.02 + 0.16 * layer
            nl = (n + Vector((lr.uniform(-1, 1), lr.uniform(-1, 1), lr.uniform(-1, 1))) * 0.5).normalized()
            ax = (down - nl * down.dot(nl)).normalized()
            ax.rotate(Matrix.Rotation(lr.uniform(-math.pi, math.pi), 3, nl))  # rosettes, not shingles
            ac = nl.cross(ax)
            length = lr.uniform(0.42, 0.55)
            centre = p + n * off + ax * length * 0.25
            tone = min(1.0, max(0.0, lr.uniform(0.15, 0.75) + 0.25 * math.sin(phi) + 0.2 * layer))
            c = lin(list(_leaves.DARK + (_leaves.LIGHT - _leaves.DARK) * tone)) + [1.0]
            ring = []
            for s, q in outline:
                cup = 0.1 * (q / 0.33) ** 2 * length
                ring.append(bm.verts.new(rot @ (centre + ax * s * length / 2 + ac * q * 1.4 * length / 2 + nl * cup)))
            mid = bm.verts.new(rot @ (centre - nl * 0.01 * length))
            for i in range(len(ring)):
                f = bm.faces.new((mid, ring[(i + 1) % len(ring)], ring[i]))  # winds along +nl
                f.smooth = True  # cupped leaves bake as curved, not as fan facets
                for lp in f.loops:
                    lp[col] = c
    mesh = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def canopy_high_material(name: str = f"{PIECE}_canopy_high") -> bpy.types.Material:
    """The master dome's leaves: base colour from tho_leaf_col, roughness 0.6 (bake source)."""
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    nt, p = _ensure_principled(mat)
    _clear(nt)
    at = nt.nodes.new("ShaderNodeAttribute")
    at.attribute_name = "tho_leaf_col"
    nt.links.new(at.outputs["Color"], p.inputs["Base Color"])
    p.inputs["Roughness"].default_value = 0.6
    p.inputs["Metallic"].default_value = 0.0
    return mat


def skeleton_sha(skel: list[dict]) -> str:
    data = [[b["level"], [[round(v, 5) for v in p] for p in b["points"]], [round(r, 5) for r in b["radii"]]]
            for b in skel]
    return hashlib.sha256(json.dumps(data).encode()).hexdigest()


def _resample(pts: list[Vector], radii: list[float], step: float) -> tuple[list[Vector], list[float], float]:
    cum = [0.0]
    for i in range(1, len(pts)):
        cum.append(cum[-1] + (pts[i] - pts[i - 1]).length)
    n = max(2, math.ceil(cum[-1] / step) + 1)
    out_p, out_r, j = [], [], 0
    for k in range(n):
        s = cum[-1] * k / (n - 1)
        while j < len(cum) - 2 and cum[j + 1] < s:
            j += 1
        f = 0.0 if cum[j + 1] == cum[j] else (s - cum[j]) / (cum[j + 1] - cum[j])
        out_p.append(pts[j].lerp(pts[j + 1], f))
        out_r.append(radii[j] + (radii[j + 1] - radii[j]) * f)
    return out_p, out_r, cum[-1]


def tube_mesh(name: str, skel: list[dict], sides: dict[int, int], step: dict[int, float],
              displace: float = 0.0) -> bpy.types.Object:
    """One mesh of closed tubes along every branch: a cap at the start and a cone at the tip, so no
    tube end is ever open. sides/step per level. displace > 0 adds the bark relief along the ring
    normal (high poly only): long vertical ridges plus finer streaks.
    UV map `UVMap` is in metres: U around the branch (the ring's own circumference, centred on the
    seam's opposite side), V along it, for the tileable bark set. Point attribute tho_wind = (sway,
    phase): sway runs from the branch's sway[0] to sway[1], phase is per branch. Face attribute
    tho_branch = the branch id."""
    verts: list[tuple] = []
    faces: list[list[int]] = []
    uvs: list[list[tuple[float, float]]] = []
    wind: list[tuple[float, float]] = []
    face_branch: list[int] = []
    for b in skel:
        lv = b["level"]
        pts, radii, length = _resample(b["points"], b["radii"], step[lv])
        ns = sides[lv]
        prev_n = None
        base = len(verts)
        rings = len(pts)
        s0, s1 = b["sway"]
        phase = hash01(f"branch:{b['id']}")
        wind += [(s0 + (s1 - s0) * i / (rings - 1), phase) for i in range(rings) for _ in range(ns)]
        vpos = [length * i / (rings - 1) for i in range(rings)]

        def uv(i: int, k: float) -> tuple[float, float]:
            return ((k / ns - 0.5) * 2 * math.pi * radii[i], vpos[i])

        for i, p in enumerate(pts):
            t = (pts[min(i + 1, rings - 1)] - pts[max(i - 1, 0)]).normalized()
            if prev_n is None:
                prev_n = t.orthogonal().normalized()
            nrm = (prev_n - t * prev_n.dot(t)).normalized()
            prev_n = nrm
            bi = t.cross(nrm)
            for k in range(ns):
                a = 2 * math.pi * k / ns
                d = nrm * math.cos(a) + bi * math.sin(a)
                q = p + d * radii[i]
                if displace:
                    ridge = 1.0 - abs(noise.noise(Vector((q.x * 5.0, q.y * 5.0, q.z * 0.35))))
                    h = 0.9 * ridge ** 3 - 0.35 + 0.35 * noise.noise(Vector((q.x * 16.0, q.y * 16.0, q.z * 2.0)))
                    q = q + d * (displace * h * min(1.0, radii[i] / 0.3))
                verts.append(tuple(q))
        for i in range(rings - 1):
            for k in range(ns):
                a0, a1 = base + i * ns + k, base + i * ns + (k + 1) % ns
                faces.append([a0, a1, a1 + ns, a0 + ns])
                uvs.append([uv(i, k), uv(i, k + 1), uv(i + 1, k + 1), uv(i + 1, k)])
        tip = len(verts)
        verts.append(tuple(pts[-1] + (pts[-1] - pts[-2]).normalized() * radii[-1] * 0.6))
        wind.append((s1, phase))
        last = base + (rings - 1) * ns
        for k in range(ns):
            faces.append([last + k, last + (k + 1) % ns, tip])
            uvs.append([uv(rings - 1, k), uv(rings - 1, k + 1),
                        (uv(rings - 1, k + 0.5)[0], length + radii[-1] * 0.6)])
        start = len(verts)
        verts.append(tuple(pts[0] - (pts[1] - pts[0]).normalized() * radii[0] * 0.3))
        wind.append((s0, phase))
        for k in range(ns):
            faces.append([base + (k + 1) % ns, base + k, start])
            uvs.append([uv(0, k + 1), uv(0, k), (uv(0, k + 0.5)[0], -radii[0] * 0.3)])
        face_branch += [b["id"]] * (len(faces) - len(face_branch))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    assert len(mesh.vertices) == len(wind) and len(mesh.polygons) == len(face_branch) == len(uvs), name
    layer = mesh.uv_layers.new(name="UVMap")
    layer.data.foreach_set("uv", [c for f in uvs for co in f for c in co])
    for p in mesh.polygons:
        p.use_smooth = True
    mesh.attributes.new("tho_wind", "FLOAT2", "POINT").data.foreach_set("vector", [c for w in wind for c in w])
    mesh.attributes.new("tho_branch", "INT", "FACE").data.foreach_set("value", face_branch)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def leaf_cards(name: str, crown: list[dict], seed: str = SEED, per_clump: int = FRINGE_PER_CLUMP) -> bpy.types.Object:
    """The fringe: lobed alpha leaf cards half-sunk into each clump shell around its rim and
    underside, every card facing straight out from the clump centre (so none is seen edge-on from
    outside its clump) with a small seeded tilt; cards wind outward. Each card maps one of the
    LEAF_TILES cluster variants in the atlas strip v >= LEAF_V0. Point attribute tho_clump holds the
    clump centre (canopy normals, wind)."""
    rng = random.Random(seed + ":fringe")
    bm = bmesh.new()
    uvl = bm.loops.layers.uv.new("UVMap")
    cl_layer = bm.verts.layers.float_vector.new("tho_clump")
    for cl in crown:
        m = clump_matrix(cl)
        c = cl["centre"]
        for k in range(per_clump):
            theta = 2 * math.pi * (k + rng.uniform(-0.3, 0.3)) / per_clump
            phi = rng.uniform(-1.0, 0.2)
            ctr = m @ (dome_point(theta, phi) * 0.97)
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
            tile = rng.randrange(LEAF_TILES)
            for loop, (s, t) in zip(f.loops, ((0, 0), (1, 0), (1, 1), (0, 1))):
                s = TILE_INSET + s * (1 - 2 * TILE_INSET)
                t = TILE_INSET + t * (1 - 2 * TILE_INSET)
                loop[uvl].uv = ((tile + s) / LEAF_TILES, LEAF_V0 + t * (1 - LEAF_V0))
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def cull_inside_crown(obj: bpy.types.Object, crown: list[dict], shrink: float = 0.85) -> int:
    """Delete faces whose every vertex lies inside a clump's dome (scaled by `shrink`): the branch
    ends that the solid shells hide. Returns the number of faces deleted."""
    inv = [(clump_matrix(cl).inverted(), cl) for cl in crown]

    def inside(co: Vector) -> bool:
        for mi, _ in inv:
            q = mi @ co
            r = math.hypot(q.x / DOME_WIDE, q.y / DOME_WIDE)
            z = q.z / (DOME_TOP if q.z >= 0 else DOME_BASE)
            if math.hypot(r, z) < R0 * shrink:
                return True
        return False

    bm = bmesh.new()
    bm.from_mesh(obj.data)
    flags = {v.index: inside(v.co) for v in bm.verts}
    drop = [f for f in bm.faces if all(flags[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=drop, context="FACES_ONLY")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()
    return len(drop)


def cull_buried(obj: bpy.types.Object, skel: list[dict], sides: dict[int, int], margin: float = 0.02) -> int:
    """Delete receiver faces whose every vertex lies inside ANOTHER branch's low tube (the intertwined
    trunk strands, root and limb starts, buried start caps). They are never visible, would bake to
    AO 0 and take atlas space. Inside = closer to that branch's axis than its inscribed radius,
    r*cos(pi/sides), minus margin. sides maps branch id -> ring sides in this receiver. Needs the
    tube_mesh face attribute tho_branch. Returns the number of faces deleted."""
    import numpy as np

    mesh = obj.data
    nv, nf = len(mesh.vertices), len(mesh.polygons)
    co = np.empty(nv * 3)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    fb = np.empty(nf, np.int64)
    mesh.attributes["tho_branch"].data.foreach_get("value", fb)
    vb = np.full(nv, -1, np.int64)
    for p in mesh.polygons:
        vb[list(p.vertices)] = fb[p.index]
    a, b, ra, rb, sb = [], [], [], [], []
    for br in skel:
        if br["id"] not in sides:  # not part of this receiver (e.g. tertiary twigs): buries nothing
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
    buried = np.zeros(nv, bool)
    for s in range(0, nv, 512):
        p = co[s:s + 512, None, :]
        t = np.clip(((p - a[None]) * ab[None]).sum(2) / ll[None], 0.0, 1.0)
        d = np.linalg.norm(p - (a[None] + t[..., None] * ab[None]), axis=2)
        r = ra[None] + (rb - ra)[None] * t
        other = sb[None] != vb[s:s + 512, None]
        buried[s:s + 512] = ((d < r - margin) & other).any(1)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    drop = [f for f in bm.faces if all(buried[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=drop, context="FACES_ONLY")
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context="VERTS")
    bm.to_mesh(mesh)
    bm.free()
    mesh.update()
    return len(drop)


def _activate_atlas_uv(mesh: bpy.types.Mesh) -> None:
    mesh.uv_layers.active = mesh.uv_layers["UVMap"]
    mesh.uv_layers["UVMap"].active_render = True
    assert mesh.uv_layers[0].name == "UVMap", [u.name for u in mesh.uv_layers]


def bark_wind_uv(obj: bpy.types.Object) -> None:
    """Copy tube_mesh's tho_wind point attribute into the second UV map `wind` (TEXCOORD_1), after
    the atlas pack so UVMap stays first and active. Drops the source attribute."""
    mesh = obj.data
    src = mesh.attributes["tho_wind"].data
    w = [tuple(src[lp.vertex_index].vector) for lp in mesh.loops]
    layer = mesh.uv_layers.new(name=WIND_UV)
    layer.data.foreach_set("uv", [c for uv_ in w for c in uv_])
    mesh.attributes.remove(mesh.attributes["tho_wind"])
    _activate_atlas_uv(mesh)


def leaf_wind_uv(obj: bpy.types.Object, skel: list[dict]) -> None:
    """Wind UV `wind` for the leaf cards: U = the sway of the skeleton point nearest the card's clump
    centre, raised toward 1 for cards on the clump's outer shell; V = a per-card phase, one value on
    all four corners so the card moves as a unit. Drops nothing (tho_clump stays for the normals)."""
    pts = [(p, b["sway"][0] + (b["sway"][1] - b["sway"][0]) * i / (len(b["points"]) - 1))
           for b in skel if b["level"] >= 1 for i, p in enumerate(b["points"])]
    mesh = obj.data
    cl = mesh.attributes["tho_clump"].data
    clump_sway: dict[tuple, float] = {}
    shell: dict[tuple, float] = {}
    per_face = []
    for poly in mesh.polygons:
        c = Vector(cl[poly.vertices[0]].vector)
        key = tuple(round(x, 4) for x in c)
        if key not in clump_sway:
            clump_sway[key] = min(pts, key=lambda ps: (ps[0] - c).length)[1]
        d = (poly.center - c).length
        shell[key] = max(shell.get(key, 0.0), d)
        per_face.append((poly, key, clump_sway[key], d))
    layer = mesh.uv_layers.new(name=WIND_UV)
    for poly, key, sway, d in per_face:
        radial = d / max(shell[key], 1e-6)
        u = min(1.0, sway * (0.5 + 0.5 * radial) + (1.0 - sway) * radial)  # inner cards half, shell 1
        v = hash01(f"card:{poly.index}")
        for li in poly.loop_indices:
            layer.data[li].uv = (u, v)
    _activate_atlas_uv(mesh)


def shell_wind_uv(obj: bpy.types.Object, cl: dict, skel: list[dict]) -> None:
    """Wind UV `wind` for one clump shell: U = SHELL_SWAY x the sway of the skeleton point nearest
    the clump centre (the solid dome moves less than its fringe), V = one phase per clump."""
    pts = [(p, b["sway"][0] + (b["sway"][1] - b["sway"][0]) * i / (len(b["points"]) - 1))
           for b in skel if b["level"] >= 1 for i, p in enumerate(b["points"])]
    sway = min(pts, key=lambda ps: (ps[0] - cl["centre"]).length)[1]
    layer = obj.data.uv_layers.get(WIND_UV) or obj.data.uv_layers.new(name=WIND_UV)
    u, v = SHELL_SWAY * sway, hash01(f"clump:{cl['id']}")
    layer.data.foreach_set("uv", [u, v] * len(obj.data.loops))
    _activate_atlas_uv(obj.data)


def clamp_wind_uv(obj: bpy.types.Object) -> None:
    """Collapse decimation interpolates corner UVs: it can overshoot by ~1e-4 and blends a little
    trunk sway into collapsed root tips. Keep wind in [0, 1] and snap sway below 0.01 to 0, so the
    rooted base stays still in every LOD (the gate wants U = 0 on the lowest vertices)."""
    import numpy as np

    layer = obj.data.uv_layers[WIND_UV]
    a = np.empty(len(layer.data) * 2)
    layer.data.foreach_get("uv", a)
    a = np.clip(a, 0.0, 1.0)
    u = a[0::2]
    u[u < 0.01] = 0.0
    a[0::2] = u
    layer.data.foreach_set("uv", a)


def set_canopy_normals(obj: bpy.types.Object, leaf_material: str, canopy_material: str | None = None) -> None:
    """Custom canopy normals (spherical-normal transfer). Fringe-card corners point along a blend of
    the direction from their clump centre, the direction from the canopy centre and world up, with
    half the card's face normal kept (a normal in the card plane gives MikkTSpace a zero tangent).
    Shell corners (canopy_material) keep their smooth normal bent toward up and out of the canopy, so
    the flat clump undersides lean into the sky light; set after the bake, so the baked normal map
    adds the leaf relief on top of the bent normal. Bark corners keep their smooth normals."""
    mesh = obj.data
    cl = mesh.attributes["tho_clump"].data
    leaf_idx = {i for i, m in enumerate(mesh.materials) if m and m.name == leaf_material}
    shell_idx = {i for i, m in enumerate(mesh.materials) if m and canopy_material and m.name == canopy_material}
    leaf_verts = {vi for p in mesh.polygons if p.material_index in leaf_idx for vi in p.vertices}
    centres = {tuple(cl[vi].vector) for vi in leaf_verts}
    canopy = sum((Vector(c) for c in centres), Vector()) / max(len(centres), 1)
    cur = [Vector(c.vector) for c in mesh.corner_normals]
    out = [tuple(c) for c in cur]
    up = Vector((0.0, 0.0, 1.0))
    for p in mesh.polygons:
        if p.material_index in leaf_idx:
            for li in p.loop_indices:
                vi = mesh.loops[li].vertex_index
                co = mesh.vertices[vi].co
                n = ((co - Vector(cl[vi].vector)).normalized() * 0.45 + (co - canopy).normalized() * 0.35
                     + up * 0.75).normalized()
                face = p.normal if n.dot(p.normal) >= 0 else -p.normal
                out[li] = tuple((n + face * 0.5).normalized())
        elif p.material_index in shell_idx:
            for li in p.loop_indices:
                co = mesh.vertices[mesh.loops[li].vertex_index].co
                n = (cur[li] + up * 0.6 + (co - canopy).normalized() * 0.3).normalized()
                out[li] = tuple(n)
    mesh.normals_split_custom_set(out)
    mesh.update()


def _ensure_principled(mat: bpy.types.Material):
    nt = mat.node_tree
    p = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    return nt, p


def _clear(nt) -> None:
    for n in [n for n in nt.nodes if n.type not in ("BSDF_PRINCIPLED", "OUTPUT_MATERIAL")]:
        nt.nodes.remove(n)


def bark_material(name: str = f"{PIECE}_bark", tile_m: float = 2.0) -> bpy.types.Material:
    """The tileable bark set (assets/kit/p1/materials/bark) on the tubes' metre UVs, as the engine
    maps it (UV0 x tileUvScale), so the live showcase shows the shipped bark."""
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    nt, p = _ensure_principled(mat)
    _clear(nt)
    uvn = nt.nodes.new("ShaderNodeUVMap")
    uvn.uv_map = "UVMap"
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (1 / tile_m, 1 / tile_m, 1.0)
    nt.links.new(uvn.outputs["UV"], mp.inputs["Vector"])

    def tex(ch: str, non_color: bool):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(str(BARK_SET / f"bark.{ch}.png"), check_existing=True)
        n.image.name = f"{PIECE}_bark_{ch}"  # owner rule O1: every datablock carries the piece prefix
        if non_color:
            n.image.colorspace_settings.name = "Non-Color"
        nt.links.new(mp.outputs["Vector"], n.inputs["Vector"])
        return n

    nt.links.new(tex("albedo", False).outputs["Color"], p.inputs["Base Color"])
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(tex("orm", True).outputs["Color"], sep.inputs[0])
    nt.links.new(sep.outputs["Green"], p.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nm.uv_map = "UVMap"
    nt.links.new(tex("normal", True).outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], p.inputs["Normal"])
    p.inputs["Metallic"].default_value = 0.0
    return mat


def leaf_images(px: int = 512) -> dict[str, bpy.types.Image]:
    """The `_leaves` strip as packed Blender images (albedo sRGB with alpha, normal and roughness
    Non-Color), width LEAF_TILES * px, height px."""
    import numpy as np

    alb, nrm, rough = _leaves.strip(px, SEED)
    out = {}
    for key, arr, srgb in (("albedo", alb, True), ("normal", nrm, False),
                           ("rough", np.repeat(rough[..., None], 3, -1), False)):
        name = f"{PIECE}_leaf_{key}"
        if name in bpy.data.images:
            bpy.data.images.remove(bpy.data.images[name])
        img = bpy.data.images.new(name, arr.shape[1], arr.shape[0], alpha=True)
        img.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
        rgba = arr if arr.shape[-1] == 4 else np.concatenate([arr, np.ones(arr.shape[:2] + (1,), np.float32)], -1)
        img.pixels.foreach_set(np.ascontiguousarray(rgba, np.float32).ravel())
        img.pack()
        out[key] = img
    return out


def leaf_material(name: str = f"{PIECE}_leaves", px: int = 512) -> bpy.types.Material:
    """The `_leaves` oak-leaf cluster strip on the cards' atlas UVs (v in [LEAF_V0, 1] -> image v)."""
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    nt, p = _ensure_principled(mat)
    _clear(nt)
    imgs = leaf_images(px)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["UV"], sep.inputs[0])
    mv = nt.nodes.new("ShaderNodeMath")
    mv.operation = "MULTIPLY_ADD"
    mv.inputs[1].default_value = 1 / (1 - LEAF_V0)
    mv.inputs[2].default_value = -LEAF_V0 / (1 - LEAF_V0)
    nt.links.new(sep.outputs["Y"], mv.inputs[0])
    comb = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(sep.outputs["X"], comb.inputs["X"])
    nt.links.new(mv.outputs[0], comb.inputs["Y"])

    def tex(key: str):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = imgs[key]
        nt.links.new(comb.outputs[0], n.inputs["Vector"])
        return n

    a = tex("albedo")
    nt.links.new(a.outputs["Color"], p.inputs["Base Color"])
    nt.links.new(a.outputs["Alpha"], p.inputs["Alpha"])
    nt.links.new(tex("rough").outputs["Color"], p.inputs["Roughness"])
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(tex("normal").outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], p.inputs["Normal"])
    p.inputs["Metallic"].default_value = 0.0
    return mat
