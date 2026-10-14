"""Tileable 'litter' PBR set (D2: 1024² over 2 m): the forest floor of forest_base (ref forest/ref-1,
ref-2): brown leaf litter, shed spruce needles and twigs over dark humus, with moss patches.

Drawn in numpy, not shader nodes (pitfalls: nodes cannot draw a leaf, and a sliced 4D Voronoi reads
as polka dots). Every element is stamped with wrap-around indices and every noise field is
band-limited white noise filtered in the FFT domain, so the tile is periodic on both axes by
construction and has no mirror symmetry. Arrays are in Blender pixel order (row 0 = v 0); the normal
is OpenGL tangent space (+Y = +v). AO = 1 like every tileable set (the piece's masks carry AO)."""
import math
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export, scene  # noqa: E402

SET, SIZE, TILE_M = "litter", 1024, 2.0
PX_M = SIZE / TILE_M
GENERATOR = pathlib.Path(__file__).resolve()
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
rng = np.random.default_rng(int.from_bytes(SET.encode(), "big"))


def srgb(h: str) -> np.ndarray:
    return np.array([int(h[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255


def pnoise(lo: float, hi: float) -> np.ndarray:
    """Periodic band-limited noise in [0, 1]: white noise kept between lo and hi cycles per tile."""
    f = np.fft.fftfreq(SIZE) * SIZE
    r = np.hypot(*np.meshgrid(f, f))
    spec = np.fft.fft2(rng.standard_normal((SIZE, SIZE)))
    spec *= np.exp(-0.5 * ((r - (lo + hi) / 2) / max((hi - lo) / 2, 1e-6)) ** 2)
    n = np.real(np.fft.ifft2(spec))
    n = (n - n.mean()) / (n.std() + 1e-9)
    return np.clip(0.5 + 0.18 * n, 0.0, 1.0).astype(np.float32)


# Layers, back to front. rgb is sRGB-encoded, height in metres. Moss is a ground layer (soft,
# irregular cushions) that the litter then partly covers, so it never reads as pasted-on blobs.
humus_n = pnoise(10, 40)
rgb = (srgb("#2a1f15") + (srgb("#46331f") - srgb("#2a1f15")) * humus_n[..., None]).astype(np.float32)
patch = pnoise(7, 18)
fine = pnoise(120, 260)
moss = np.clip((patch - 0.6) / 0.12, 0, 1) * (0.6 + 0.4 * np.clip((fine - 0.35) / 0.3, 0, 1))
moss_col = srgb("#33451b") + (srgb("#62742a") - srgb("#33451b")) * fine[..., None]
rgb = rgb * (1 - moss[..., None]) + moss_col * moss[..., None]
height = (humus_n * 0.004 + moss * (0.003 + 0.002 * fine)).astype(np.float32)
rough = (0.95 * (1 - moss) + 0.92 * moss).astype(np.float32)
ys, xs = np.mgrid[0:SIZE, 0:SIZE]


def stamp(cx: float, cy: float, ext: int):
    """Wrapped window of half-size ext px around (cx, cy) px: (row idx, col idx, dx, dy) in px."""
    x0, y0 = int(math.floor(cx)) - ext, int(math.floor(cy)) - ext
    cols = np.arange(x0, x0 + 2 * ext + 1)
    rows = np.arange(y0, y0 + 2 * ext + 1)
    dx = cols[None, :] + 0.5 - cx
    dy = rows[:, None] + 0.5 - cy
    return np.ix_(rows % SIZE, cols % SIZE), dx, dy


LEAF_COLS = [srgb(c) for c in ("#6b4325", "#8a5a2e", "#a06a35", "#7a4f2a", "#5a3a22", "#b07d45", "#4a3020")]
LEAVES, NEEDLES, TWIGS = 2300, 5200, 70
z_top = 0.004
# Broadleaf litter (oak, beech): 4-7 cm, flattened and curled, mostly decayed browns.
for i in range(LEAVES):
    cx, cy = rng.uniform(0, SIZE, 2)
    length = rng.uniform(0.045, 0.08) * PX_M
    width = length * rng.uniform(0.45, 0.62)
    if rng.random() < 0.8 * moss[int(cy) % SIZE, int(cx) % SIZE]:
        continue  # fewer leaves settle on the moss cushions
    th = rng.uniform(0, 2 * math.pi)
    c, s = math.cos(th), math.sin(th)
    idx, dx, dy = stamp(cx, cy, int(length * 0.6) + 2)
    sl = (dx * c + dy * s) / (length / 2)
    q = (-dx * s + dy * c) / (width / 2)
    lobes = 1.0 - 0.18 * (0.5 + 0.5 * np.cos(2 * math.pi * 3.0 * (sl + 1) / 2 + rng.uniform(0, 6)))
    env = np.sqrt(np.clip(1 - sl * sl, 0, 1)) * (0.8 + 0.2 * sl) * lobes
    inside = (np.abs(sl) <= 1) & (np.abs(q) < env)
    if not inside.any():
        continue
    tone = LEAF_COLS[int(rng.integers(len(LEAF_COLS)))] * rng.uniform(0.7, 1.0)
    across = np.clip(np.abs(q) / np.maximum(env, 1e-3), 0, 1)
    curl = rng.uniform(-1, 1)
    col = tone[None, None, :] * (1.0 - 0.22 * across[..., None] ** 2)
    col = np.where((np.abs(q) < 0.07)[..., None], col * 1.15, col)  # midrib
    z_top += 1.2e-6
    h = z_top + 0.0012 * (curl * across ** 2 + 0.3 * (1 - sl * sl))
    rgb[idx] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[idx])
    height[idx] = np.where(inside, h, height[idx])
    rough[idx] = np.where(inside, rng.uniform(0.62, 0.8), rough[idx])

# Shed spruce needles (1.2-1.8 cm, reddish tan) and thin twigs (8-22 cm) on top of the leaves.
for kind, count in (("needle", NEEDLES), ("twig", TWIGS)):
    for i in range(count):
        cx, cy = rng.uniform(0, SIZE, 2)
        if kind == "needle":
            length, half_w = rng.uniform(0.012, 0.018) * PX_M, 0.75
            tone = srgb("#8c5e34") * rng.uniform(0.75, 1.15)
        else:
            length, half_w = rng.uniform(0.08, 0.22) * PX_M, rng.uniform(1.0, 1.8)
            tone = srgb("#5d4a3a") * rng.uniform(0.75, 1.1)
        th = rng.uniform(0, 2 * math.pi)
        c, s = math.cos(th), math.sin(th)
        idx, dx, dy = stamp(cx, cy, int(length / 2) + 3)
        along = dx * c + dy * s
        across = np.abs(-dx * s + dy * c)
        inside = (np.abs(along) <= length / 2) & (across < half_w)
        if not inside.any():
            continue
        z_top += 4e-7
        ridge = 1.0 - np.clip(across / half_w, 0, 1) ** 2
        col = tone[None, None, :] * (0.8 + 0.3 * ridge[..., None])
        rgb[idx] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[idx])
        height[idx] = np.where(inside, z_top + 0.0015 * half_w / 1.8 * ridge, height[idx])
        rough[idx] = np.where(inside, 0.7, rough[idx])

# Cavity darkening: a little baked occlusion in the albedo where the surface sits below its
# neighbourhood (the set's ORM AO stays 1 like every tileable set).
blur = height.copy()
for _ in range(4):
    blur = (blur + np.roll(blur, 3, 0) + np.roll(blur, -3, 0) + np.roll(blur, 3, 1) + np.roll(blur, -3, 1)) / 5
cav = np.clip((blur - height) / 0.002, 0, 1)
rgb = rgb * (1 - 0.35 * cav[..., None])

# Normal from the periodic height field (central differences with wrap), in metres per pixel.
d = 1.0 / PX_M
gx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) / (2 * d)
gy = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) / (2 * d)
k = 0.35  # flattened slope: litter reads as a mat, not as sharp relief
n = np.stack([-gx * k, -gy * k, np.ones_like(gx)], -1)
n /= np.linalg.norm(n, axis=-1, keepdims=True)

out.mkdir(parents=True, exist_ok=True)
ones = np.ones((SIZE, SIZE), np.float32)
albedo = np.concatenate([np.clip(rgb, 0, 1), ones[..., None]], -1).reshape(-1, 4)
bake.write_png(SIZE, albedo, out / f"{SET}.albedo.png", non_color=False)
bake.write_png(SIZE, np.concatenate([n * 0.5 + 0.5, ones[..., None]], -1).reshape(-1, 4),
               out / f"{SET}.normal.png")
bake.write_png(SIZE, np.stack([ones, rough, 0 * ones, ones], -1).reshape(-1, 4), out / f"{SET}.orm.png")

# Self-review numbers: the wrap is exact by construction; the mirror correlation must stay low.
lum = rgb.mean(-1)
z = (lum - lum.mean()) / (lum.std() + 1e-9)
mirror = {"flipU": float((z * z[:, ::-1]).mean()), "flipV": float((z * z[::-1, :]).mean())}
stats = {"albedoMeanSrgb": [round(float(x), 4) for x in rgb.reshape(-1, 3).mean(0)],
         "mossCover": round(float((moss > 0.5).mean()), 4), "roughMean": round(float(rough.mean()), 4),
         "mirrorCorrelation": {k_: round(v, 4) for k_, v in mirror.items()}}
print(f"{SET}: {stats}")
bake.write_bake_report(str(out / "bake-report.json"),
                       generator=GENERATOR.relative_to(export.REPO).as_posix(),
                       generatorSha256=bake.sha256_file(GENERATOR),
                       set=SET, size=SIZE, tileM=TILE_M, device="numpy", samples=0, method="numpy-drawn",
                       stats=stats)
