"""Measure the meadow/forest border hump: relief mean by distance from the border, and the masks
profile (A = AO, G = porosity) by distance from the border. Usage: measure.py <repo> [relief-module]."""
import math
import sys

import numpy as np
from PIL import Image

REPO = sys.argv[1]
R = 16.0
AP = R * math.sqrt(3) / 2


def dist(x, y):
    return AP - np.max([x * math.cos(math.radians(-60 * k)) + y * math.sin(math.radians(-60 * k))
                        for k in range(6)], axis=0)


def edge_of(x, y):
    return np.argmax([x * math.cos(math.radians(-60 * k)) + y * math.sin(math.radians(-60 * k))
                      for k in range(6)], axis=0)


def smooth(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def und(x, y):
    s = 0.6 * np.sin(0.31 * x + 1.7) * np.cos(0.27 * y - 0.4) + 0.4 * np.sin(0.53 * x - 0.45 * y + 0.9)
    return 0.5 + 0.5 * s


def mounds(x, y):
    s = (0.5 * np.sin(0.9 * x + 0.3) * np.sin(0.8 * y + 1.1)
         + 0.5 * np.sin(1.3 * x - 1.1 * y + 2.0) * np.cos(0.7 * x + 1.4 * y))
    return 0.5 + 0.5 * s


def meadow(x, y):
    return -0.25 * smooth(dist(x, y) / 2.0) * und(x, y)


def forest(x, y):
    return -smooth(dist(x, y) / 2.0) * (0.15 * und(x, y) + 0.1 * mounds(x, y))


def river_straight(x, y):
    """_river.relief on river_straight_base (channel along x, d = |y|), meadow part only."""
    fc = smooth((np.abs(y) - 6.83) / 2.0) * (np.abs(y) >= 6.83)
    return -0.10 * smooth(dist(x, y) / 2.0) * fc * und(x, y)


g = np.linspace(-R, R, 1601)
X, Y = np.meshgrid(g, g)
D = dist(X, Y)
inside = D >= 0
E = edge_of(X, Y)
bands = [(0, 0.5), (0.5, 1), (1, 1.5), (1.5, 2), (2, 3), (3, 4), (4, 6), (6, 9), (9, 14)]
for name, f in (("meadow", meadow), ("forest", forest), ("river_straight (meadow part, |y| > 8.83)", river_straight)):
    Z = f(X, Y)
    if name.startswith("river"):
        inside = (D >= 0) & (np.abs(Y) > 8.83)
    else:
        inside = D >= 0
    print(f"{name}: relief mean by distance from border (m)")
    print("  " + "  ".join(f"[{a},{b}) {Z[inside & (D >= a) & (D < b)].mean():+.3f}" for a, b in bands))
    # Slope of the mean profile in the last 2 m (grade), and per-edge mean in d 2-3 m.
    print("  per-edge mean z in d [2,3): " + " ".join(
        f"e{k} {Z[inside & (D >= 2) & (D < 3) & (E == k)].mean():+.3f}" for k in range(6)))
    print(f"  interior mean {Z[inside].mean():+.4f}, min {Z[inside].min():+.3f}, max {Z[inside].max():+.3f}")

for piece in ("meadow_base", "forest_base"):
    im = np.asarray(Image.open(f"{REPO}/assets/kit/p1/{piece}/{piece}.masks.png").convert("RGBA"), dtype=np.float64) / 255
    n = im.shape[0]
    c = (np.arange(n) + 0.5) / n * 2 * R - R
    xs, ys = np.meshgrid(c, c[::-1])  # PNG row 0 = V 1 (top)
    d = dist(xs, ys)
    ins = d >= 0
    print(f"{piece} masks by distance from border: A (AO) / G")
    print("  " + "  ".join(f"[{a},{b}) A {im[ins & (d >= a) & (d < b), 3].mean():.3f} G {im[ins & (d >= a) & (d < b), 1].mean():.3f}"
                           for a, b in bands))
