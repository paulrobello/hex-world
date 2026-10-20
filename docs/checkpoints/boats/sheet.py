"""Owner review sheets for the boats (SKILL.md "Owner checkpoints"): one sheet per piece in
docs/checkpoints/<id>/sheet.jpg with the approved references, the headless EEVEE stills with the masks
bound (evidence only; on a water plane and the water contract's river profile), and the in-engine
shots with tilt-shift off: `water_kit` (the boats on the look-dev water of the ocean tiles, play
distance with houses and windmills for scale, sun and shade close-ups, a view near the water) and
`kit` (the landing stage on a dry river straight, where the posts show against the bed, with a rowing
boat in the channel; eye level 1.7 m on the deck). Props: no border seam. 3 columns.
Usage: python3 docs/checkpoints/boats/sheet.py (from the repo root; needs Pillow)."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/boats"
REFS = ROOT / "assets-source/refs"
W, H = 640, 400
STILL = "Blender EEVEE + masks (evidence only)"
WATER = "engine water_kit (look-dev water at y 0; the static placement does not ride the swell)"

SHEETS = {
    "boat_rowing": [
        (REFS / "boat_rowing/ref-1.jpg", "reference ref-1 (approved, D4): white, blue band, varnished inside"),
        (REFS / "boat_rowing/ref-2.jpg", "reference ref-2 (approved): on a river at the landing stage"),
        (REFS / "boat_rowing/ref-3.jpg", "reference ref-3 (approved): pulled up on a beach"),
        (DOCS / "eevee-boat_rowing-studio-q.jpg", f"{STILL}: frames, thwarts, floorboards, oars"),
        (DOCS / "eevee-boat_rowing-studio-side.jpg", f"{STILL}: side, raked stem, 0.15 m draft"),
        (DOCS / "eevee-boat_rowing-water-close.jpg", f"{STILL}: on water, oars swept 25 deg"),
        (DOCS / "boat_rowing-water-close-sun.jpg", f"{WATER}: ~6 m, sun side"),
        (DOCS / "boat_rowing-water-close-shade.jpg", f"{WATER}: ~6 m, shade side"),
        (DOCS / "boat_rowing-water-eye.jpg", f"{WATER}: 1.7 m over the water, ~7 m"),
        (DOCS / "boats-water-play.jpg", "engine water_kit, play ~50 m: rowing boat (front), sailing boat, houses"),
        (DOCS / "landing_stage-kit-eye.jpg", "engine kit, eye level 1.7 m on the stage deck (dry channel)"),
        (DOCS / "eevee-boat_rowing-water-play.jpg", f"{STILL}: play distance on water"),
    ],
    "boat_sail": [
        (REFS / "boat_sail/ref-1.jpg", "reference ref-1 (approved, D4): blue hull, white band, tan sails"),
        (REFS / "boat_sail/ref-2.jpg", "reference ref-2 (approved): main and one jib on the ocean (BQ9)"),
        (DOCS / "eevee-boat_sail-studio-q.jpg", f"{STILL}: gaff main, jib, 6 m mast, sails amidships"),
        (DOCS / "eevee-boat_sail-studio-inside.jpg", f"{STILL}: thwarts, floorboards, two fish baskets"),
        (DOCS / "eevee-boat_sail-water-close.jpg", f"{STILL}: on water, main 25 deg, jib 20 deg"),
        (DOCS / "eevee-boat_sail-water-play.jpg", f"{STILL}: play distance on water"),
        (DOCS / "boat_sail-water-close-sun.jpg", f"{WATER}: ~10 m, sun side"),
        (DOCS / "boat_sail-water-close-shade.jpg", f"{WATER}: ~10 m, shade side; faces away from the kit sun read dark, as house_twostorey's north wall"),
        (DOCS / "boats-water-play.jpg", "engine water_kit, play ~50 m: sailing boat (back), rowing boat, houses"),
    ],
    "landing_stage": [
        (REFS / "boat_rowing/ref-2.jpg", "reference boat_rowing ref-2 (approved, BQ5): the landing stage"),
        (DOCS / "eevee-landing_stage-bank-moored.jpg", f"{STILL}: on the river profile, rowing boat moored, oars trailed"),
        (DOCS / "eevee-landing_stage-bank-low.jpg", f"{STILL}: low, deck 0.45 m over the water"),
        (DOCS / "eevee-landing_stage-dry-posts.jpg", f"{STILL}: water off: posts set into the bed"),
        (DOCS / "landing_stage-kit-channel.jpg", "engine kit, river straight (-2, 6), no water drawn: posts in the bed"),
        (DOCS / "landing_stage-kit-close-sun.jpg", "engine kit, ~6 m from the channel (dry), deck front and fender"),
        (DOCS / "landing_stage-kit-eye.jpg", "engine kit, eye level 1.7 m on the deck"),
        (DOCS / "boats-kit-play.jpg", "engine kit, play ~45 m: river straights, rail bridge, farm"),
    ],
}


def cell(path: pathlib.Path, label: str) -> Image.Image:
    im = Image.open(path).convert("RGB")
    im.thumbnail((W, H))
    c = Image.new("RGB", (W, H + 26), (24, 24, 24))
    c.paste(im, ((W - im.width) // 2, 26 + (H - im.height) // 2))
    ImageDraw.Draw(c).text((8, 6), label, fill=(235, 235, 235))
    return c


for piece, cells_in in SHEETS.items():
    comp = json.loads((ROOT / f"assets-source/kit/{piece}/compress-report.json").read_text())
    b = comp["budgets"]
    exp = json.loads((ROOT / f"assets-source/kit/{piece}/export-report.json").read_text())
    tris = "/".join(str(exp["lodTris"][f"{piece}_lod{i}"]) for i in range(3))
    title = (f"{piece} (P3 boats, D4): review sheet. LOD {tris} tris (body + parts), GLB "
             f"{b['glbBytes'] / 1000:.0f} KB, masks 1024 KTX2 {b['ktxBytes']['bc7'] / 1e6:.2f} MB BC7 / "
             f"{b['ktxBytes']['astc'] / 1e6:.2f} MB ASTC. Waterline = origin plane. Pending owner review.")
    cells = [cell(p, label) for p, label in cells_in]
    cols = 3
    rows = (len(cells) + cols - 1) // cols
    cw, ch = cells[0].size
    sheet = Image.new("RGB", (cw * cols, ch * rows + 30), (24, 24, 24))
    ImageDraw.Draw(sheet).text((8, 8), title, fill=(235, 235, 235))
    for k, c in enumerate(cells):
        sheet.paste(c, ((k % cols) * cw, 30 + (k // cols) * ch))
    out = ROOT / f"docs/checkpoints/{piece}/sheet.jpg"
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out, quality=85)
    print(f"wrote {out.relative_to(ROOT)} ({len(cells)} cells)")
