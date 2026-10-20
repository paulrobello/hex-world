"""Review sheet for tree_hero_oak_c: the approved reference, the textured EEVEE stills beside the hero
and _b, and the play shots (roosting birds, pair, play distance and top for the variant and the hero,
scale with all three hero oaks and a house cluster, sun and shade close-ups, dead top, trunk, wind pair,
eye level for both and under the variant's crown). Usage: python3 docs/checkpoints/tree_hero_oak_c/sheet.py
(from the repo root, after run.sh). Writes docs/checkpoints/tree_hero_oak_c/sheet.jpg."""
import pathlib

from PIL import Image, ImageDraw

D = pathlib.Path("docs/checkpoints/tree_hero_oak_c")
W = 480
H = 300


def tile(path, label):
    im = Image.open(path).convert("RGB")
    im = im.resize((W, int(im.height * W / im.width)))
    if im.height > H:
        top = (im.height - H) // 2
        im = im.crop((0, top, W, top + H))
    out = Image.new("RGB", (W, H), (30, 30, 30))
    out.paste(im, (0, (H - im.height) // 2))
    d = ImageDraw.Draw(out)
    d.rectangle((0, 0, W, 20), fill=(0, 0, 0))
    d.text((6, 4), label, fill=(255, 255, 255))
    return out


rows = [
    [(pathlib.Path("assets-source/refs/tree_hero_oak/ref-1.jpg"), "approved ref-1 (tree_hero_oak)"),
     (D / "c-lineup.jpg", "EEVEE textured from the south: hero, _b, _c"),
     (D / "c-lineup-e.jpg", "EEVEE textured from the east: _c, _b, hero"),
     (D / "c-lineup-top.jpg", "EEVEE top: hero, _b, _c (open dead middle)")],
    [(D / "c-close.jpg", "EEVEE close-up: dead leaders, tines, stub"),
     (D / "oakc-roost.jpg", "play: birds roosting on tree_hero_oak_c"),
     (D / "oakc-roost-close.jpg", "birds on the lobe tops, close"),
     (D / "oakc-play-pair.jpg", "play: _c at (-1,0) left, hero at (1,0) right")],
    [(D / "oakc-play.jpg", "play distance: tree_hero_oak_c"),
     (D / "oak-play.jpg", "play distance: tree_hero_oak (same camera)"),
     (D / "oakc-top.jpg", "top: tree_hero_oak_c"),
     (D / "oak-top.jpg", "top: tree_hero_oak")],
    [(D / "oakc-scale.jpg", "scale: _c (-1,0), hero (1,0), _b (1,-2), houses (0,-1)"),
     (D / "oakc-close-sun.jpg", "sun side close-up ~8 m: _c"),
     (D / "oakc-close-shade.jpg", "shade side close-up ~8 m: _c"),
     (D / "oakc-deadtop.jpg", "dead top, ~14 m: _c")],
    [(D / "oakc-trunk-sun.jpg", "trunk, stub and limbs, sun side: _c"),
     (D / "oakc-eye.jpg", "eye level 1.7 m, 20 m south: _c"),
     (D / "oak-eye.jpg", "eye level 1.7 m, 20 m south: hero"),
     (D / "oakc-under.jpg", "walk eye level under the crown: _c")],
    [(D / "oakc-wind-a.jpg", "wind pair, frame 0: _c"),
     (D / "oakc-wind-b.jpg", "wind pair, +75 frames: _c"),
     (D / "c-wind-u.jpg", "wind U (sway) on LOD0: black 0, white 1")],
]
sheet = Image.new("RGB", (W * 4, H * len(rows)), (30, 30, 30))
for j, r in enumerate(rows):
    for i, (p, label) in enumerate(r):
        sheet.paste(tile(p, label), (i * W, j * H))
sheet.save(D / "sheet.jpg", quality=85)
print("wrote", D / "sheet.jpg", sheet.size)
