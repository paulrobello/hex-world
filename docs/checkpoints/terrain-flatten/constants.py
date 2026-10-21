"""Regenerate every runtime height that mirrors the meadow or forest relief from the one formula in
scripts/blender/kit/_terrain.py (owner D6-Q1, 2026-10-20). For each table row in
crates/hex-render/src/kit_scene.rs it finds the frame (rotation by 60 degrees x k in the base's
Blender frame) in which the old formula reproduces the committed height, then prints the new height
in that same frame. With --write it rewrites those heights in place. Also prints the farm.rs and
_hay_var.py test points. One-shot: it ran once against the pre-flatten heights (2026-10-20); on the
rewritten file the old formula no longer reproduces them, so a later relief change needs the new
formula as `old_*` here.
Usage: python3 docs/checkpoints/terrain-flatten/constants.py [--write]"""
import math
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/blender"))
sys.path.insert(0, str(ROOT / "scripts/blender/kit"))
import _terrain  # noqa: E402
from lib import hexgeo  # noqa: E402


def ss(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def old_meadow(x, y):
    return -0.25 * ss(hexgeo.dist_to_border(x, y) / 2.0) * _terrain.undulation(x, y)


def old_forest(x, y):
    return -ss(hexgeo.dist_to_border(x, y) / 2.0) * (0.15 * _terrain.undulation(x, y) + 0.1 * _terrain.mounds(x, y))


KIT = ROOT / "crates/hex-render/src/kit_scene.rs"
FOREST_TABLES = ("OAK_GROVE", "UNDERGROWTH", "FOREST_FLOOR", "FOREST_FLOOR_VARIANTS")
MEADOW_TABLES = ("HAY_BALES", "VERGE_BUSHES", "FARM_ANIMALS", "MEADOW_PROPS")
ROW = re.compile(r'^(\s*\("(\w+)", \[(-?[\d.]+), (-?[\d.]+)\], (-?[\d.]+), )(-?\d+\.\d+)(\),.*)$')


def lit(v):
    """A Rust literal at mm precision (0.0 on the plane, never -0.000)."""
    return "0.0" if abs(v) < 5e-4 else f"{v:.3f}"


def frame_of(old, x, z, want):
    """The rotation k (Blender frame) in which `old` gives `want` at runtime offset (x, z), or None."""
    hits = [k for k in range(6) if abs(old(*hexgeo.rotate_xy((x, -z), k)) - want) < 1.5e-3]
    return hits[0] if hits else None


def main(write):
    lines = KIT.read_text().split("\n")
    table = None
    changed = 0
    for i, line in enumerate(lines):
        m = re.match(r"^const (\w+): ", line)
        if m:
            table = m.group(1)
        if line.startswith("];"):
            table = None
        r = ROW.match(line)
        if not r or table not in FOREST_TABLES + MEADOW_TABLES:
            continue
        piece, x, z, want = r.group(2), float(r.group(3)), float(r.group(4)), float(r.group(6))
        if piece == "car_sedan" and table == "VERGE_BUSHES":
            continue  # cars::ROAD_SURFACE_Y, not the relief
        old, new = (old_forest, _terrain.forest_relief) if table in FOREST_TABLES else (old_meadow, _terrain.meadow_relief)
        k = frame_of(old, x, z, want)
        assert k is not None, (table, piece, x, z, want)
        v = new(*hexgeo.rotate_xy((x, -z), k))
        print(f"{table:22s} {piece:20s} ({x:5.1f}, {z:5.1f}) k={k}  {want:+.3f} -> {lit(v)}")
        lines[i] = f"{r.group(1)}{lit(v)}{r.group(7)}"
        changed += lines[i] != line
    singles = {"barn_red": ((-4.75, 8.227), -0.154), "bakery_shop": ((1.5, -8.6), -0.062),
               "bakery_shop_b": ((1.5, 9.0), -0.04)}
    for i, line in enumerate(lines):
        for piece, ((x, z), want) in singles.items():
            if f'..place("{piece}", ' in line and lines[i - 1].strip().startswith("y: "):
                k = frame_of(old_meadow, x, z, want)
                assert k is not None, (piece, x, z, want)
                v = _terrain.meadow_relief(*hexgeo.rotate_xy((x, -z), k))
                print(f"{'single':22s} {piece:20s} ({x:5.1f}, {z:5.1f}) k={k}  {want:+.3f} -> {lit(v)}")
                lines[i - 1] = re.sub(r"y: -?\d+\.\d+,", f"y: {lit(v)},", lines[i - 1])
    for i, line in enumerate(lines):
        if line.startswith("const WINDMILL_Y: f32 = "):
            v = _terrain.meadow_relief(0.0, 0.0)
            print(f"WINDMILL_Y {line.split('= ')[1]} -> {lit(v)}")
            lines[i] = f"const WINDMILL_Y: f32 = {lit(v)};"
    print("farm.rs the_relief_matches_the_kit_anchors (x, z, y):")
    for x, z in ((0.0, 0.0), (-2.0, 2.5), (-9.5, 1.0), (-8.0, -4.5), (7.0, 2.5), (3.5, 7.0), (2.0, 9.0)):
        print(f"    ({x}, {z}, {lit(_terrain.meadow_relief(x, -z))}),")
    print("farm.ron hero slots, old -> new relief (runtime x, z):")
    for name, (x, z) in (("barn", (4.75, -8.227)), ("stack", (8.9, -2.4)), ("round1", (7.5, -0.4)),
                         ("round2", (10.4, -1.4)), ("cow", (0.25, 8.25)), ("sheep", (-0.5, 10.75))):
        print(f"    {name:7s} {old_meadow(x, -z):+.3f} -> {_terrain.meadow_relief(x, -z):+.3f}")
    if write:
        KIT.write_text("\n".join(lines))
        print(f"wrote {KIT}")


main("--write" in sys.argv)
