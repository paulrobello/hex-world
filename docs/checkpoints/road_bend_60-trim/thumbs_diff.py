"""Before/after strips of the thumbnails `make thumbs` changed.

  python3 docs/checkpoints/road_bend_60-trim/thumbs_diff.py <before dir> <after dir> <out dir>

For every WebP in <before dir> (the committed thumbnails, `git show HEAD:assets/thumbs/<name>`),
compares it with the same name in <after dir> (assets/thumbs after `make thumbs`), prints the
largest per-channel difference, the pixels off by more than 8 levels and their bounding box, and
writes <out dir>/<name>.jpg: before | after | difference x8, each scaled 4x (nearest).
"""
import pathlib
import sys

import numpy as np
from PIL import Image

before, after, out = (pathlib.Path(a) for a in sys.argv[1:4])
out.mkdir(parents=True, exist_ok=True)
for pb in sorted(before.glob("*.webp")):
    a = np.asarray(Image.open(pb).convert("RGB")).astype(int)
    b = np.asarray(Image.open(after / pb.name).convert("RGB")).astype(int)
    d = np.abs(a - b)
    m = d.max(axis=2)
    line = f"{pb.stem}: max {int(m.max())}, pixels over 8 levels {int((m > 8).sum())} of {m.size}"
    if (m > 8).any():
        ys, xs = np.nonzero(m > 8)
        line += f", box x {xs.min()}-{xs.max()} y {ys.min()}-{ys.max()}"
    print(line)
    row = np.concatenate([a, np.full((a.shape[0], 4, 3), 24), b, np.full((a.shape[0], 4, 3), 24),
                          np.clip(d * 8, 0, 255)], axis=1).astype(np.uint8)
    img = Image.fromarray(row)
    img.resize((img.width * 4, img.height * 4), Image.NEAREST).save(out / f"{pb.stem}.jpg", quality=92)
