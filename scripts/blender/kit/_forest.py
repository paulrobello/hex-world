"""Shared tree_forest geometry: a stylized-real Norway spruce (ref forest/ref-1, ref-2), about 16 m.

Parts (Blender frame, Z up, trunk base at the origin):
- trunk: one tapered tube (`_tree.tube_mesh`, bark UVs in metres for the tileable `bark` set);
- core: a closed dark cone inside the crown, so gaps between fronds read as dense needle shadow
  instead of sky (a colour + masks slot, no texture of its own);
- needle cards: one drooping frond card per whorl branch plus a shorter filler frond above it, and a
  crossed leader at the top, all mapping FROND_TILES numpy-drawn frond variants (`frond_strip`) in the
  atlas rows v >= NEEDLE_V0. LOD0 cards are folded (a ridge along the branch, sides hanging), LOD1 and
  LOD2 are regenerated from the same branch list with fewer segments, never decimated.
Wind (TEXCOORD_1, UV map "wind", project.md): U = sway, 0 at the trunk base rising to TRUNK_SWAY at
the top, and along every card from the trunk's value at its root to 1 at its tip; V = a phase per
card (all its vertices) and one for trunk and core."""
from __future__ import annotations

import hashlib
import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import _tree

PIECE = "tree_forest"
SEED = PIECE
HEIGHT = 16.2  # trunk tip (m); the leader cards reach about 16.4
Z0, ZTOP = 1.95, 15.2  # lowest and highest whorl
WHORLS = 26
RMAX = 3.3  # crown radius at the lowest whorl (m): crown width / height about 0.38
TRUNK_R0, TRUNK_R1 = 0.27, 0.035
TRUNK_SWAY = 0.3
CARD_W = 0.55  # card width / length
FOLD = math.radians(34)
CURL = 0.2  # upturned branch tips (ref-1): the sag is mid-branch  # LOD0 card ridge: each side drops by sin(FOLD) of its half width
# Atlas rows (1024 px): bark island and core masks below NEEDLE_V0, then FROND_TILES frond tiles of
# TILE_ROWS rows each, full atlas width (u along the branch, trunk end at u = 0).
ATLAS = 1024
NEEDLE_ROWS0, TILE_ROWS, FROND_TILES = 288, 368, 2
NEEDLE_V0 = NEEDLE_ROWS0 / ATLAS
TILE_INSET = 0.01
WIND_UV = _tree.WIND_UV


def hash01(key: str) -> float:
    return int.from_bytes(hashlib.sha256(f"{SEED}:{key}".encode()).digest()[:4], "big") / 2**32


def trunk_sway(z: float) -> float:
    return TRUNK_SWAY * min(max(z / HEIGHT, 0.0), 1.0)


def crown_r(z: float) -> float:
    """Crown radius at height z: a slightly convex cone from RMAX at Z0 to a narrow spire."""
    f = min(max((z - Z0) / (ZTOP - Z0), 0.0), 1.0)
    return RMAX * (1 - f) ** 0.85 + 0.32


def trunk_skeleton() -> list[dict]:
    """The trunk as one `_tree.tube_mesh` branch: a slight base flare, tapering to the tip."""
    pts, radii = [], []
    n = 33
    for i in range(n):
        t = i / (n - 1)
        z = -0.15 + (HEIGHT + 0.15) * t
        pts.append(Vector((0.03 * math.sin(2.1 * t), 0.02 * math.sin(3.3 * t + 1), z)))
        radii.append(TRUNK_R1 + (TRUNK_R0 - TRUNK_R1) * (1 - t) ** 1.15 + 0.16 * max(0.0, 1 - z / 0.9) ** 2)
    return [{"id": 0, "level": 0, "points": pts, "radii": radii, "sway": (0.0, TRUNK_SWAY)}]


def branches() -> list[dict]:
    """Every frond card: {kind: main|filler, z, a (azimuth), L, rise, droop, roll, tile, t0, u0}.
    t0 is where the card starts along its parent branch (0 for main cards); u0 the card-root sway."""
    rng = random.Random(SEED + ":whorls")
    out = []
    phase = 0.0
    for k in range(WHORLS):
        f = k / (WHORLS - 1)
        z = Z0 + (ZTOP - Z0) * f ** 1.08 + rng.uniform(-0.06, 0.06)
        n = int(round(7.5 - 3.5 * f))
        phase += 2.39996  # golden angle between whorls
        for j in range(n):
            a = phase + 2 * math.pi * (j + rng.uniform(-0.18, 0.18)) / n
            length = crown_r(z) * rng.uniform(0.86, 1.1)
            droop = 0.4 - 0.22 * f + rng.uniform(-0.04, 0.04)
            main = {"kind": "main", "z": z, "a": a, "L": length, "rise": 0.1 + 0.06 * f, "droop": droop,
                    "roll": rng.uniform(-0.15, 0.15), "tile": rng.randrange(FROND_TILES), "t0": 0.0}
            out.append(main)
            side = 1 if rng.random() < 0.5 else -1
            out.append({"kind": "filler", "z": z + rng.uniform(0.12, 0.22),
                        "a": a + side * rng.uniform(0.38, 0.58), "L": length * rng.uniform(0.5, 0.62),
                        "rise": main["rise"], "droop": droop * 0.9, "roll": rng.uniform(-0.15, 0.15),
                        "tile": rng.randrange(FROND_TILES), "t0": 0.38})
    for i, b in enumerate(out):
        b["id"] = i
    return out


def _path(b: dict, t: float) -> Vector:
    """Centre line of a card at parameter t in [0, 1] (0 at the trunk end)."""
    h = Vector((math.cos(b["a"]), math.sin(b["a"]), 0.0))
    r0 = 0.06 + b["t0"] * b["L"] * 1.4  # a filler starts out along its parent
    L = b["L"]
    return (h * (r0 + L * t) + Vector((0, 0, b["z"] + L * (b["rise"] * t - b["droop"] * t * t + CURL * t ** 3))))


def card_mesh(name: str, cards: list[dict], segs: int, folded: bool, width_scale: float = 1.0,
              leader: int = 2) -> bpy.types.Object:
    """Frond cards (segs segments along each card; folded: a centre ridge, else one flat strip) plus
    `leader` crossed vertical cards at the top. UVMap on the frond tiles, then UV map `wind`."""
    bm = bmesh.new()
    uvl = bm.loops.layers.uv.new("UVMap")
    wnd = bm.loops.layers.uv.new(WIND_UV)
    cols = (-1.0, 0.0, 1.0) if folded else (-1.0, 1.0)

    def tile_uv(tile: int, t: float, c: float) -> tuple[float, float]:
        s = TILE_INSET + t * (1 - 2 * TILE_INSET)
        q = TILE_INSET + (0.5 + 0.5 * c) * (1 - 2 * TILE_INSET)
        return s, (NEEDLE_ROWS0 + TILE_ROWS * (tile + q)) / ATLAS

    def emit(grid, uvs, winds):
        for i in range(len(grid) - 1):
            for j in range(len(grid[0]) - 1):
                quad = (grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1])
                f = bm.faces.new(quad)
                f.smooth = True
                for lp, (ii, jj) in zip(f.loops, ((i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1))):
                    lp[uvl].uv = uvs[ii][jj]
                    lp[wnd].uv = winds[ii]

    for b in cards:
        L = b["L"]
        h = Vector((math.cos(b["a"]), math.sin(b["a"]), 0.0))
        side = Vector((-h.y, h.x, 0.0))
        half = 0.5 * CARD_W * L * width_scale
        u_root = trunk_sway(b["z"])
        v_phase = hash01(f"card:{b['id']}")
        grid, uvs, winds = [], [], []
        for i in range(segs + 1):
            t = i / segs
            p = _path(b, t)
            row, urow = [], []
            for c in cols:
                drop = abs(c) * half * math.sin(FOLD) if folded else half * math.sin(FOLD) * 0.5
                q = p + side * (c * half * math.cos(FOLD)) + Vector((0, 0, -drop + c * b["roll"] * half))
                row.append(bm.verts.new(q))
                urow.append(tile_uv(b["tile"], t, c))
            grid.append(row)
            uvs.append(urow)
            tt = b["t0"] + (1 - b["t0"]) * t
            winds.append((min(1.0, u_root + (1 - u_root) * tt ** 0.8), v_phase))
        emit(grid, uvs, winds)
    # Leader: crossed vertical fronds around the trunk tip (u along the card points up).
    for k in range(leader):
        a = math.pi / 2 * k + 0.4
        side = Vector((math.cos(a), math.sin(a), 0.0))
        z0, z1, half = ZTOP - 0.1, HEIGHT + 0.25, 0.17
        grid, uvs, winds = [], [], []
        for i in range(3):
            t = i / 2
            p = Vector((0, 0, z0 + (z1 - z0) * t))
            grid.append([bm.verts.new(p - side * half), bm.verts.new(p + side * half)])
            uvs.append([tile_uv(k % FROND_TILES, t, -1.0), tile_uv(k % FROND_TILES, t, 1.0)])
            winds.append((min(1.0, trunk_sway(p.z) + 0.6 * t + 0.1), hash01(f"leader:{k}")))
        emit(grid, uvs, winds)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.uv_layers.active = mesh.uv_layers["UVMap"]
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def core_mesh(name: str, sides: int, rings: int, uv_box: tuple[float, float, float, float]) -> bpy.types.Object:
    """The closed dark inner cone: from just above the lowest whorl to near the top, at 0.5 of the
    crown radius. UV0 maps it into uv_box (u0, v0, u1, v1) of the masks map (its AO gradient), with a
    duplicated seam column. Wind U follows the trunk."""
    za, zb = Z0 + 0.25, ZTOP - 0.3
    verts, uvs, faces = [], [], []
    for i in range(rings + 1):
        z = za + (zb - za) * i / rings
        r = 0.36 * crown_r(z)
        for j in range(sides + 1):
            a = 2 * math.pi * j / sides
            verts.append((r * math.cos(a), r * math.sin(a), z))
            uvs.append((uv_box[0] + (uv_box[2] - uv_box[0]) * j / sides, uv_box[1] + (uv_box[3] - uv_box[1]) * i / rings))
    w = sides + 1
    for i in range(rings):
        for j in range(sides):
            faces.append([i * w + j, i * w + j + 1, (i + 1) * w + j + 1, (i + 1) * w + j])
    top = len(verts)
    verts.append((0.0, 0.0, zb + 0.6))
    uvs.append((0.5 * (uv_box[0] + uv_box[2]), uv_box[3]))
    bottom = len(verts)
    verts.append((0.0, 0.0, za - 0.25))
    uvs.append((0.5 * (uv_box[0] + uv_box[2]), uv_box[1]))
    last = rings * w
    for j in range(sides):
        faces.append([last + j, last + j + 1, top])
        faces.append([j + 1, j, bottom])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    uvl = mesh.uv_layers.new(name="UVMap")
    wl = mesh.uv_layers.new(name=WIND_UV)
    for poly in mesh.polygons:
        poly.use_smooth = True
        for li in poly.loop_indices:
            vi = mesh.loops[li].vertex_index
            uvl.data[li].uv = uvs[vi]
            wl.data[li].uv = (trunk_sway(verts[vi][2]), hash01("core"))
    mesh.uv_layers.active = uvl
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def set_spruce_normals(obj: bpy.types.Object, needle_material: str, core_material: str) -> None:
    """Custom normals for the crown (spherical-normal transfer, cone form): needle-card and core
    corners point radially out of the trunk axis, tilted up by 40 degrees, so the cone shades as one
    volume. Cards keep half their face normal (a normal in the card plane gives MikkTSpace a zero
    tangent). Bark corners keep their smooth normals."""
    mesh = obj.data
    nidx = {i for i, m in enumerate(mesh.materials) if m and m.name == needle_material}
    cidx = {i for i, m in enumerate(mesh.materials) if m and m.name == core_material}
    out = [tuple(c.vector) for c in mesh.corner_normals]
    up = Vector((0.0, 0.0, 1.0))
    ce, se = math.cos(math.radians(40)), math.sin(math.radians(40))
    for p in mesh.polygons:
        if p.material_index not in nidx and p.material_index not in cidx:
            continue
        for li in p.loop_indices:
            co = mesh.vertices[mesh.loops[li].vertex_index].co
            radial = Vector((co.x, co.y, 0.0))
            radial = radial.normalized() if radial.length > 1e-4 else Vector((1.0, 0.0, 0.0))
            n = (radial * ce + up * se).normalized()
            if p.material_index in nidx:
                face = p.normal if n.dot(p.normal) >= 0 else -p.normal
                n = (n + face * 0.5).normalized()
            out[li] = tuple(n)
    mesh.normals_split_custom_set(out)
    mesh.update()


# --- Frond tiles (numpy, Blender pixel order: row 0 = v 0; u along the branch, trunk end at x = 0) ---
DARK, MID, LIGHT = (np.array(c, np.float32) / 255 for c in ((0x1a, 0x30, 0x18), (0x2e, 0x52, 0x26), (0x6c, 0x93, 0x45)))
TWIG = np.array((0x4a, 0x36, 0x26), np.float32) / 255


def frond_tile(w: int, h: int, k: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """One spruce frond: a main twig along the tile centre with alternating side shoots, every twig
    covered in needles angled forward, older needles dark inside and fresh light-green tips. Returns
    albedo RGBA (sRGB RGB, alpha mask), normal RGB and roughness, shape (h, w, n)."""
    rng = np.random.default_rng(int.from_bytes(hashlib.sha256(f"{SEED}:frond:{k}".encode()).digest()[:8], "big"))
    rgb = np.empty((h, w, 3), np.float32)
    rgb[:] = MID  # transparent texels carry needle green, so mips stay green
    alpha = np.zeros((h, w), np.float32)
    nrm = np.empty((h, w, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((h, w), 0.8, np.float32)
    cy = h / 2

    def env(s: float) -> float:
        """Frond half-width (fraction of h/2) at s along it: bare near the trunk, broad, pointed tip."""
        return 0.96 * math.sin(math.pi * min(1.0, 0.12 + 0.9 * s)) ** 0.55 * (1 - 0.25 * s) * min(1.0, s / 0.12)

    def line(x0, y0, x1, y1, half_w, col_a, col_b, depth):
        """Anti-aliased segment, colour from col_a at its start to col_b at its end, drawn over."""
        bx0, bx1 = int(max(min(x0, x1) - half_w - 1, 0)), int(min(max(x0, x1) + half_w + 2, w))
        by0, by1 = int(max(min(y0, y1) - half_w - 1, 0)), int(min(max(y0, y1) + half_w + 2, h))
        if bx0 >= bx1 or by0 >= by1:
            return
        ys, xs = np.mgrid[by0:by1, bx0:bx1].astype(np.float32) + 0.5
        dx, dy = x1 - x0, y1 - y0
        ll = max(dx * dx + dy * dy, 1e-6)
        t = np.clip(((xs - x0) * dx + (ys - y0) * dy) / ll, 0, 1)
        px, py = x0 + t * dx - xs, y0 + t * dy - ys
        d = np.sqrt(px * px + py * py)
        cov = np.clip(half_w + 0.5 - d, 0, 1)
        m = cov > 0
        if not m.any():
            return
        across = np.clip(d / max(half_w, 0.5), 0, 1)
        col = (col_a[None, None] + (col_b - col_a)[None, None] * t[..., None]) * (1.05 - 0.3 * across[..., None] ** 2)
        sub = (slice(by0, by1), slice(bx0, bx1))
        a = cov[..., None]
        rgb[sub] = rgb[sub] * (1 - a) + np.clip(col, 0, 1) * a
        alpha[sub] = np.maximum(alpha[sub], cov)
        # Needle normal: rounded across the needle, plus the frond's side droop (depth).
        nl = math.sqrt(ll)
        sx, sy = -dy / nl, dx / nl
        sgn = np.sign(px * sx + py * sy)
        tilt = -sgn * across * 0.35
        nx, ny = sx * tilt, sy * tilt + depth
        nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.1, 1))
        n3 = np.stack([nx, ny, nz], -1) * 0.5 + 0.5
        nrm[sub] = np.where(m[..., None], n3, nrm[sub])
        rough[sub] = np.where(m, 0.72 + 0.12 * across, rough[sub])

    def needles(x0, y0, x1, y1, spacing, nlen, young):
        """Needles along a twig from (x0, y0) to (x1, y1), both sides, angled toward its tip."""
        dx, dy = x1 - x0, y1 - y0
        length = math.hypot(dx, dy)
        if length < 2:
            return
        ux, uy = dx / length, dy / length
        steps = int(length / spacing)
        for i in range(steps):
            s = (i + rng.uniform(0, 1)) / max(steps, 1)
            bx, by = x0 + dx * s, y0 + dy * s
            for side in (-1, 1):
                ang = math.radians(rng.uniform(38, 75)) * side
                ca, sa = math.cos(ang), math.sin(ang)
                vx, vy = ux * ca - uy * sa, ux * sa + uy * ca
                ln = nlen * rng.uniform(0.75, 1.15) * (0.75 + 0.25 * (1 - s))
                fresh = s > young
                tip = LIGHT * rng.uniform(0.85, 1.05) if fresh else MID * rng.uniform(0.85, 1.15)
                base = MID * 0.9 if fresh else DARK * rng.uniform(0.85, 1.2)
                depth = ((by - cy) / cy) * 0.2
                line(bx, by, bx + vx * ln, by + vy * ln, 1.25, base, tip, depth)

    x_end = w * 0.975
    # Shadowed inner mass: the frond's body under the needles, so the card holds its coverage down
    # the mip chain (thin needles alone gave 0.27) and gaps read as depth, not sky.
    xs_ = (np.arange(w, dtype=np.float32) + 0.5) / w
    envs = np.array([env(s) for s in xs_], np.float32) * cy * 0.74
    yy = np.abs(np.arange(h, dtype=np.float32)[:, None] + 0.5 - cy)
    body = (yy < envs[None, :]) & (xs_[None, :] > 0.1) & (xs_[None, :] < 0.95)
    shade = 0.75 + 0.25 * rng.random((h, w)).astype(np.float32)
    rgb[body] = (DARK[None, :] * shade[body][:, None])
    alpha[body] = 1.0
    nrm[body] = (0.5, 0.5 + 0.0, 1.0)
    rough[body] = 0.88
    # Side shoots, back to front: alternating, angled forward, length from the frond envelope.
    shoots = []
    x = w * 0.05
    side = 1
    while x < x_end - 12:
        s = x / w
        half = env(s) * cy
        ang = math.radians(rng.uniform(42, 56))
        ln = max(half - 12, 0) / math.sin(ang)
        if ln > 6:
            shoots.append((x, side, ang, ln))
        x += rng.uniform(15, 21) * (1.0 - 0.3 * s)
        side = -side
    young = 0.55 + 0.1 * rng.random()
    for x, side, ang, ln in shoots:
        ex, ey = x + math.cos(ang) * ln, cy + side * math.sin(ang) * ln
        line(x, cy, ex, ey, 1.4, TWIG, TWIG * 1.2, 0.0)
        needles(x, cy, ex, ey, 2.6, 21.0, young)
    line(0, cy, x_end, cy, 2.6, TWIG * 0.9, TWIG * 1.15, 0.0)
    needles(w * 0.1, cy, x_end, cy, 2.6, 22.0, 0.8)
    albedo = np.concatenate([rgb, alpha[..., None]], -1)
    return albedo, nrm, rough


def frond_strip(w: int = ATLAS, h: int = TILE_ROWS) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """FROND_TILES tiles stacked in v (tile 0 lowest): arrays (h * FROND_TILES, w, n)."""
    tiles = [frond_tile(w, h, k) for k in range(FROND_TILES)]
    return tuple(np.concatenate([t[i] for t in tiles], axis=0) for i in range(3))


def coverage_by_mip(alpha: np.ndarray) -> list[float]:
    """Box-filtered alpha > 0.5 coverage per mip level (as the oak reports it)."""
    cov, m = [], alpha
    while True:
        cov.append(round(float((m > 0.5).mean()), 4))
        if min(m.shape) < 2:
            break
        h2, w2 = m.shape[0] // 2 * 2, m.shape[1] // 2 * 2
        m = m[:h2, :w2].reshape(h2 // 2, 2, w2 // 2, 2).mean(axis=(1, 3))
    return cov
