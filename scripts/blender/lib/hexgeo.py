"""Hex frame shared by every kit generator. Constants are frozen (D8) and tested against axial.rs.

Blender is Z-up; glTF/runtime is Y-up: runtime (x, y, z) = Blender (x, z, -y).
Runtime: pointy-top, edge 0 faces +x, edges clockwise viewed from +Y, +z is south.
In Blender's top view edge k's outward normal is at -60°·k.

Edge contracts are edge-index generic: edge k is edge 0 rotated by rotate_xy(·, k), so pieces that
turn (road bends) reuse the same border ring slice and lane frame at every edge.
"""
import math

HEX_R = 16.0
LEVEL_STEP = 4.0
APOTHEM = HEX_R * math.sqrt(3.0) / 2.0
BORDER_SUBDIV = 16  # segments per hex edge on the border ring, identical in every LOD


def blender_to_rt(x, y, z):
    return (x, z, -y)


def rt_to_blender(x, y, z):
    return (x, -z, y)


def corner(i, r=HEX_R):
    """Runtime corner i sits at 30°+60°·i in the XZ plane (crates/hex-render/src/scene.rs corner())."""
    a = math.radians(30.0 + 60.0 * i)
    return (r * math.cos(a), -r * math.sin(a))


def edge_normal(k):
    a = math.radians(60.0 * k)
    return (math.cos(a), -math.sin(a))


def edge_midpoint(k, r=HEX_R):
    d = r * math.sqrt(3.0) / 2.0
    n = edge_normal(k)
    return (d * n[0], d * n[1])


def border_ring(subdiv=BORDER_SUBDIV, r=HEX_R):
    """Points around the border, edge 0 first; edge k runs from corner k-1 to corner k."""
    pts = []
    for k in range(6):
        a, b = corner((k - 1) % 6, r), corner(k % 6, r)
        for s in range(subdiv):
            t = s / subdiv
            pts.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return pts


def dist_to_border(x, y):
    """Distance to the hex border, positive inside."""
    return APOTHEM - max(x * n[0] + y * n[1] for n in (edge_normal(k) for k in range(6)))


def lattice_points(spacing, margin):
    """Triangular lattice inside the hex, at least `margin` from the border. Deterministic order."""
    out = []
    row_h = spacing * math.sqrt(3.0) / 2.0
    n = int(HEX_R / row_h) + 2
    for j in range(-n, n + 1):
        y = j * row_h
        off = (spacing / 2.0) if j % 2 else 0.0
        m = int(HEX_R / spacing) + 2
        for i in range(-m, m + 1):
            x = i * spacing + off
            if dist_to_border(x, y) >= margin:
                out.append((x, y))
    return out


def rotate_xy(p, k):
    """Rotate by k edge steps (edge e -> edge e+k), i.e. -60°·k in Blender's top view."""
    a = math.radians(-60.0 * k)
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c)


def lane_frame(edge=0, offset=0.0, width=6.0):
    """Lane frame on edge `edge`: center is the edge midpoint moved `offset` along the tangent
    (-n_y, n_x); identical for every edge up to rotate_xy, so bends reuse the edge-0 cross-section."""
    n = edge_normal(edge)
    t = (-n[1], n[0])
    m = edge_midpoint(edge)
    return {"center": (m[0] + t[0] * offset, m[1] + t[1] * offset), "tangent": t, "normal": n, "width": width}
