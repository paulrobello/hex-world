"""Equirect HDRI -> GGX-prefiltered specular cubemap + SH9 irradiance cubemap for Bevy (D5).
Bevy samples cubemaps at (w.x, w.y, -w.z) for world direction w (environment_map.wesl), and
picks mip = perceptual_roughness * (levels - 1)."""
import math

import numpy as np

# wgpu/Vulkan cube face basis for face-local (u, v) in [-1, 1], v down (rows top-down).
_FACES = [
    lambda u, v: np.stack([np.ones_like(u), -v, -u], -1),   # +X
    lambda u, v: np.stack([-np.ones_like(u), -v, u], -1),   # -X
    lambda u, v: np.stack([u, np.ones_like(u), v], -1),     # +Y
    lambda u, v: np.stack([u, -np.ones_like(u), -v], -1),   # -Y
    lambda u, v: np.stack([u, -v, np.ones_like(u)], -1),    # +Z
    lambda u, v: np.stack([-u, -v, -np.ones_like(u)], -1),  # -Z
]


def face_dirs(face, n):
    c = (np.arange(n) + 0.5) / n * 2 - 1
    u, v = np.meshgrid(c, c)
    d = _FACES[face](u, v)
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


def cube_to_world(d):
    return d * np.array([1.0, 1.0, -1.0])


def load_equirect(path):
    import bpy
    img = bpy.data.images.load(str(path))
    w, h = img.size
    a = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)[::-1, :, :3].copy()  # Blender rows are bottom-up


def sample_equirect(eq, w):
    """Runtime world dir (Y-up) -> Blender dir (x, -z, y) -> Blender's equirect mapping
    (Cycles projection.h: u = 0.5 - atan2(y, x) / 2pi)."""
    bx, by, bz = w[..., 0], -w[..., 2], w[..., 1]
    u = np.arctan2(-by, bx) / (2 * math.pi) + 0.5
    v = np.arctan2(bz, np.hypot(bx, by)) / math.pi + 0.5
    h, wd = eq.shape[:2]
    x = np.clip((u * wd).astype(int) % wd, 0, wd - 1)
    y = np.clip(((1 - v) * h).astype(int), 0, h - 1)
    return eq[y, x]


def _downsample(eq):
    h, w = eq.shape[:2]
    return eq[: h // 2 * 2, : w // 2 * 2].reshape(h // 2, 2, w // 2, 2, 3).mean(axis=(1, 3))


def prefilter(eq, base=256, samples=128):
    levels_n = int(math.log2(base)) + 1
    pyramid = [eq]
    while min(pyramid[-1].shape[:2]) > 8:
        pyramid.append(_downsample(pyramid[-1]))
    i = np.arange(samples)
    xi1 = (i + 0.5) / samples
    xi2 = np.array([int(f"{k:032b}"[::-1], 2) / 2**32 for k in i])  # Hammersley
    out = []
    for m in range(levels_n):
        n = base >> m
        rough = m / (levels_n - 1)
        a = max(rough * rough, 1e-4)
        faces = []
        for f in range(6):
            N = cube_to_world(face_dirs(f, n))
            if m == 0:
                faces.append(sample_equirect(eq, N))
                continue
            up = np.where(np.abs(N[..., 1:2]) < 0.999, [[0, 1, 0]], [[1, 0, 0]])
            T = np.cross(up, N)
            T /= np.linalg.norm(T, axis=-1, keepdims=True)
            B = np.cross(N, T)
            acc = np.zeros(N.shape[:2] + (3,))
            wsum = np.zeros(N.shape[:2] + (1,))
            phi = 2 * math.pi * xi1
            cos_t = np.sqrt((1 - xi2) / (1 + (a * a - 1) * xi2))
            sin_t = np.sqrt(1 - cos_t * cos_t)
            src = pyramid[min(len(pyramid) - 1, int(rough * (len(pyramid) - 1) + 0.5))]
            for s in range(samples):
                Hl = np.array([sin_t[s] * math.cos(phi[s]), sin_t[s] * math.sin(phi[s]), cos_t[s]])
                H = T * Hl[0] + B * Hl[1] + N * Hl[2]
                L = 2 * np.sum(N * H, -1, keepdims=True) * H - N
                nl = np.sum(N * L, -1, keepdims=True)
                wgt = np.clip(nl, 0, None)
                acc += sample_equirect(src, L) * wgt
                wsum += wgt
            faces.append(acc / np.maximum(wsum, 1e-6))
        rgba = np.concatenate([np.stack(faces), np.ones((6, n, n, 1))], -1)
        out.append(rgba.astype(np.float32))
    return out


def _sh9(d):
    x, y, z = d[..., 0], d[..., 1], d[..., 2]
    return np.stack([0.282095 * np.ones_like(x), 0.488603 * y, 0.488603 * z, 0.488603 * x,
                     1.092548 * x * y, 1.092548 * y * z, 0.315392 * (3 * z * z - 1),
                     1.092548 * x * z, 0.546274 * (x * x - y * y)], -1)


def irradiance(eq, size=32):
    h, w = eq.shape[:2]
    th = (np.arange(h) + 0.5) / h * math.pi
    ph = (np.arange(w) + 0.5) / w * 2 * math.pi
    P, T = np.meshgrid(ph, th)
    u, v = P / (2 * math.pi), 1 - T / math.pi
    bx, by, bz = np.cos((u - 0.5) * 2 * math.pi) * np.cos((v - 0.5) * math.pi), \
        -np.sin((u - 0.5) * 2 * math.pi) * np.cos((v - 0.5) * math.pi), np.sin((v - 0.5) * math.pi)
    wdir = np.stack([bx, bz, -by], -1)  # Blender -> runtime
    dA = (np.sin(T) * (math.pi / h) * (2 * math.pi / w))[..., None]
    coeffs = np.einsum("hwk,hwc->kc", _sh9(wdir) * dA, eq)
    band = np.array([math.pi, 2 * math.pi / 3, 2 * math.pi / 3, 2 * math.pi / 3, math.pi / 4, math.pi / 4,
                     math.pi / 4, math.pi / 4, math.pi / 4])
    faces = []
    for f in range(6):
        N = cube_to_world(face_dirs(f, size))
        faces.append(np.einsum("hwk,kc->hwc", _sh9(N) * band, coeffs) / math.pi)
    rgba = np.concatenate([np.stack(faces), np.ones((6, size, size, 1))], -1)
    return [np.clip(rgba, 0, None).astype(np.float32)]
