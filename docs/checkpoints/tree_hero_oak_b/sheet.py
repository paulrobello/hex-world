"""Review sheet for tree_hero_oak_b: the approved reference, the six-view renders beside the hero's,
and the play shots (pair, play distance and top for both oaks, sun and shade close-ups, trunk, eye
level for both). Usage: python3 docs/checkpoints/tree_hero_oak_b/sheet.py (from the repo root, after
run.sh). Writes docs/checkpoints/tree_hero_oak_b/sheet.jpg."""
import pathlib

from PIL import Image, ImageDraw

D = pathlib.Path("docs/checkpoints/tree_hero_oak_b")
K = pathlib.Path("assets-source/kit")
W = 640


def tile(path, label):
    im = Image.open(path).convert("RGB")
    im = im.resize((W, int(im.height * W / im.width)))
    ImageDraw.Draw(im).rectangle((0, 0, W, 24), fill=(0, 0, 0))
    ImageDraw.Draw(im).text((8, 5), label, fill=(255, 255, 255))
    return im


rows = [
    [(pathlib.Path("assets-source/refs/tree_hero_oak/ref-1.jpg"), "approved ref-1 (tree_hero_oak)"),
     (D / "eevee-pair.jpg", "EEVEE textured: tree_hero_oak (left), tree_hero_oak_b (right)"),
     (D / "renders-compare.jpg", "6-view: hero, _b, tree_oak, tree_oak_b (front/left/3q/top)")],
    [(D / "oakb-play-pair.jpg", "play: tree_hero_oak_b at (-1,0) left, tree_hero_oak at (1,0) right"),
     (D / "oakb-play.jpg", "play distance, west camera: tree_hero_oak_b left"),
     (D / "oak-play.jpg", "play distance, east camera: tree_hero_oak right")],
    [(D / "oakb-top.jpg", "top, west camera: tree_hero_oak_b (split crown)"),
     (D / "oak-top.jpg", "top, east camera: tree_hero_oak (round crown)"),
     (D / "oakb-close-sun.jpg", "sun side close-up ~8 m: tree_hero_oak_b")],
    [(D / "oakb-close-shade.jpg", "shade side close-up ~8 m: tree_hero_oak_b"),
     (D / "oakb-trunk-sun.jpg", "trunk and fork, sun side: tree_hero_oak_b"),
     (D / "oakb-eye.jpg", "eye level 1.7 m, 20 m south: tree_hero_oak_b")],
    [(D / "oak-eye.jpg", "eye level 1.7 m, 20 m south: tree_hero_oak"),
     (D / "wind-u.jpg", "wind U (sway) on LOD0: black 0 at the base, white 1"),
     (D / "eevee-close.jpg", "EEVEE textured close-up: fork and heavy limb")],
]
tiles = [[tile(p, l) for p, l in r] for r in rows]
h = [max(t.height for t in r) for r in tiles]
sheet = Image.new("RGB", (W * 3, sum(h)), (30, 30, 30))
y = 0
for r, rh in zip(tiles, h):
    for i, t in enumerate(r):
        sheet.paste(t, (i * W, y))
    y += rh
sheet.save(D / "sheet.jpg", quality=85)
print("wrote", D / "sheet.jpg", sheet.size)
