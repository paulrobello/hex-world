"""Minimal KTX2 writer: uncompressed VK_FORMAT_R16G16B16A16_SFLOAT cubemaps (IBL, D5).
Layout per the KTX2 spec: header, level index, basic DFD, then level data smallest-first."""
import struct

import numpy as np

VK_R16G16B16A16_SFLOAT = 97
_ID = bytes([0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A])


def _dfd():
    samples = b""
    for i, ch in enumerate((0, 1, 2, 15)):  # R, G, B, A
        ctype = ch | 0x80 | 0x40  # FLOAT | SIGNED
        samples += struct.pack("<IIII", (i * 16) | (15 << 16) | (ctype << 24), 0, 0xBF800000, 0x3F800000)
    block = struct.pack("<IIIIII", 0, 2 | ((24 + len(samples)) << 16),
                        1 | (1 << 8) | (1 << 16),  # RGBSDA, BT709 primaries, linear transfer
                        0, 8, 0) + samples
    return struct.pack("<I", 4 + len(block)) + block


def write_cubemap_rgba16f(path, levels):
    """levels[i]: (6, n_i, n_i, 4) float array, faces +X,-X,+Y,-Y,+Z,-Z, rows top-down.
    Returns {"clamped", "nonFinite"} texel-channel counts: finite values beyond float16's 65504
    are clamped (the HDRI sun), NaN is written as 0 and +-inf as +-65504."""
    n0 = levels[0].shape[1]
    dfd = _dfd()
    index_off = 80
    dfd_off = index_off + 24 * len(levels)
    data_off = dfd_off + len(dfd)
    counts = {"clamped": 0, "nonFinite": 0}
    blobs = []
    for lv in levels:
        a = np.asarray(lv, np.float32)
        finite = np.isfinite(a)
        counts["clamped"] += int(np.count_nonzero(finite & (np.abs(a) > 65504)))
        counts["nonFinite"] += int(np.count_nonzero(~finite))
        a = np.clip(np.nan_to_num(a, nan=0.0, posinf=65504, neginf=-65504), -65504, 65504)
        blobs.append(np.ascontiguousarray(a, np.float16).tobytes())
    offsets = [0] * len(levels)
    cur = data_off
    for i in reversed(range(len(levels))):  # smallest level first in the file
        cur = (cur + 7) & ~7
        offsets[i] = cur
        cur += len(blobs[i])
    head = _ID + struct.pack("<9I", VK_R16G16B16A16_SFLOAT, 2, n0, n0, 0, 0, 6, len(levels), 0)
    head += struct.pack("<IIIIQQ", dfd_off, len(dfd), 0, 0, 0, 0)
    assert len(head) == 80
    index = b"".join(struct.pack("<QQQ", offsets[i], len(blobs[i]), len(blobs[i])) for i in range(len(levels)))
    out = bytearray(head + index + dfd)
    for i in reversed(range(len(levels))):
        out += b"\0" * (offsets[i] - len(out))
        out += blobs[i]
    with open(path, "wb") as f:
        f.write(out)
    return counts
