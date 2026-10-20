"""meadow_base: hex column (soil-strata skirt) + meadow terrain cap. Scripted route (PRD §11.1).
Geometry contract: plan Task 8 Interfaces. Border ring identical in every LOD; never decimated."""
import math
import os
import pathlib
import random
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv  # noqa: E402

PIECE = "meadow_base"
SPACING = (0.9, 1.8, None)
SKIRT_ROWS = (4, 2, 1)
MASK_SIZE = 1024  # 32 px/m over the 32 m square (D2)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
random.seed(PIECE)


def falloff(x, y):
    t = min(max(hexgeo.dist_to_border(x, y) / 2.0, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def undulation(x, y, m=math):
    """Deterministic meadow swell in [0, 1]: two crossed long waves plus a shorter ripple.
    m = math for scalars (the relief), numpy for arrays (the masks), so both share one formula and
    the scalar path stays bit-identical to libm."""
    s = (0.6 * m.sin(0.31 * x + 1.7) * m.cos(0.27 * y - 0.4)
         + 0.4 * m.sin(0.53 * x - 0.45 * y + 0.9))
    return 0.5 + 0.5 * s


def relief(x, y):
    # Contract: z in [-0.25, 0.0], never above 0, smoothstep falloff to 0 (value and slope) within
    # 2.0 m of the border, so a road bed at +0.08 m covers it.
    return -0.25 * falloff(x, y) * undulation(x, y)


def shade_slope(x, y):
    """The slope the cap shades with (its custom normals): the swell's own slope under the falloff,
    -0.25 * falloff * grad(undulation), leaving out the falloff's rise to the border ring. That rise
    (0.11 m over the last 2 m on both sides of every border) shaded as a ridge along each tile border
    in side light (owner, 2026-10-14). 0 at the border, the true slope where the falloff is 1."""
    f = falloff(x, y)
    a, b, c = 0.31 * x + 1.7, 0.27 * y - 0.4, 0.53 * x - 0.45 * y + 0.9
    dux = 0.5 * (0.6 * 0.31 * math.cos(a) * math.cos(b) + 0.4 * 0.53 * math.cos(c))
    duy = 0.5 * (-0.6 * 0.27 * math.sin(a) * math.sin(b) - 0.4 * 0.45 * math.cos(c))
    return -0.25 * f * dux, -0.25 * f * duy


for _x, _y in ((0.0, 0.0), (3.1, -4.2), (-6.5, 2.7)):  # falloff 1 here: the true slope of relief
    _fd = ((relief(_x + 1e-5, _y) - relief(_x - 1e-5, _y)) / 2e-5,
           (relief(_x, _y + 1e-5) - relief(_x, _y - 1e-5)) / 2e-5)
    assert all(abs(p - q) < 1e-6 for p, q in zip(shade_slope(_x, _y), _fd)), (_x, _y, _fd)


root = scene.empty(PIECE)
cap_mat = materials.pbr_material(f"{PIECE}_cap", materials.srgb("#4f7d32"), 0.9)
skirt_mat = materials.pbr_material(f"{PIECE}_skirt", materials.srgb("#6b4a2f"), 0.95)
ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
lods = []
for i, (spacing, rows) in enumerate(zip(SPACING, SKIRT_ROWS)):
    interior = hexgeo.lattice_points(spacing, spacing * 0.5) if spacing else []
    cap = geom.cdt_cap(f"{PIECE}_lod{i}_cap", ring, interior, relief)
    geom.set_cap_normals(cap, shade_slope)
    uv.planar_cap_uv(cap)
    cap.data.materials.append(cap_mat)
    sk = geom.skirt(f"{PIECE}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, rows)
    sk.data.uv_layers.new(name="UVMap")
    for d in sk.data.uv_layers["UVMap"].data:
        d.uv = (0.005, 0.005)
    sk.data.materials.append(skirt_mat)
    body = geom.join([cap, sk], f"{PIECE}_lod{i}")
    lod.triangulate(body)
    body.parent = root
    lods.append(body)
lod.assert_border_identical(lods)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris}")

col = collider.collider_from(lods[2], "hull", f"{PIECE}_collider")
col.parent = root
# Six prop anchors (seeded scatter-rules hook, PRD §6): alternating inner/outer rings with a seeded
# angular jitter, all at least 2.5 m inside the border, on the relief surface.
for n in range(6):
    a = math.radians(60 * n + 30 + random.uniform(-10.0, 10.0))
    r = (5.5 if n % 2 else 8.5) + random.uniform(-0.75, 0.75)
    x, y = r * math.cos(a), r * math.sin(a)
    assert hexgeo.dist_to_border(x, y) >= 2.5, (n, x, y)
    export.add_socket(f"anchor-prop-{n}", (x, y, relief(x, y)), root, kind="prop", radius=2.0)

texel = uv.measure_texel_density(lods[0], MASK_SIZE)
ao_stats = None
if not skip_bake:
    bake.setup_cycles(samples=64)
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(lods[0], "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    # Texel centres in Blender pixel order (row 0 = V 0): x = 32U - 16, y = 32V - 16.
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    # Porosity (G) dips on the swell crests, where the meadow drains and compacts; snow capacity (R)
    # is full everywhere on the cap. Night emissive (B) is 0: a meadow has no lights.
    swell = undulation(xs, ys, np)
    wet = (0.7 - 0.2 * swell).ravel().astype(np.float32)
    masks_png = bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
    # B6: AO over the hex footprint only (texels outside the hex are unbaked gutter).
    _, px = bake._load(masks_png)
    inside = (np.max([xs * math.cos(math.radians(-60 * k)) + ys * math.sin(math.radians(-60 * k))
                      for k in range(6)], axis=0) < hexgeo.APOTHEM - 0.5).ravel()
    a = px[inside, 3]
    ao_stats = {"texels": int(inside.sum()), "min": float(a.min()), "mean": float(a.mean()),
                "p01": float(np.percentile(a, 1)), "snowMean": float(px[inside, 0].mean()),
                "wetMin": float(px[inside, 1].min()), "wetMax": float(px[inside, 1].max())}
    print(f"{PIECE}: masks over the hex footprint {ao_stats}")
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                       channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, maskStats=ao_stats, skipped=skip_bake)
