"""Review sheets for the forest undergrowth, sapling_spruce and shrub_forest (refs approved 2026-10-13,
forest ref-1: "undergrowth of ferns, shrubs and saplings"): the approved references, textured headless
EEVEE stills (still.py), the in-engine shots (`--tilt off`) from undergrowth.json on the undergrowth
forest at (-3, 2) in the kit scenario, and play-scenario shots from undergrowth-play.json taken with
docs/checkpoints/shrub_forest/integration.patch applied (pine forest scatter, walk mode, a walk into a
shrub; dumps ugp-*.json). One sheet per piece, in docs/checkpoints/<piece>/.
Run from the repo root after run.sh: python3 docs/checkpoints/sapling_spruce/sheet.py"""
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/sapling_spruce"
REFS = ROOT / "assets-source/refs/forest"
COMMON_TAIL = [
    (DOCS / "ug-play.jpg", "in engine, play distance, tilt off: the undergrowth forest at (-3, 2), 3 shrubs, "
                           "3 saplings, 2 ferns, 2 spruces"),
    (DOCS / "ug-eye.jpg", "eye level 1.7 m across the undergrowth"),
    (DOCS / "ug-scale.jpg", "scale: shrubs and saplings beside tree_forest, tree_forest_c and fern_clump"),
    (DOCS / "undergrowth-lineup.jpg", "EEVEE: fern_clump, sapling_spruce, shrub_forest, 1.7 m figure; "
                                      "tree_forest behind"),
    (DOCS / "ugp-play.jpg", "PLAY, integration.patch applied: five pine forest tiles at play distance "
                            "(13 shrubs, 10 saplings drawn; most under the spruce crowns)"),
    (DOCS / "ugp-walk-shrub.jpg", "PLAY, patch applied, walk mode eye level: a scattered shrub at the forest edge"),
    (DOCS / "ugp-walk-sapling.jpg", "PLAY, patch applied, walk mode: a scattered sapling 0.84 m from a spruce, "
                                    "its tiers lost in the crowns (owner question)"),
    (DOCS / "ugp-walk-shrub-blocked.jpg", "PLAY, patch applied: walked into a shrub, blocked 'tree' by shrub_forest "
                                          "1.0 m from its centre"),
]
SHEETS = {
    "sapling_spruce": [
        (REFS / "ref-1.jpg", "ref-1 (approved): saplings in the undergrowth among the spruces"),
        (REFS / "ref-2.jpg", "ref-2 (approved): the spruce whose needles the sapling shares"),
        (DOCS / "sapling_spruce-eevee-three_quarter.jpg", "sapling_spruce, 2.2 m, five sparse tiers (EEVEE)"),
        (DOCS / "sapling_spruce-eevee-low_side.jpg", "sapling_spruce, low side view (EEVEE)"),
        (DOCS / "ug-sapling-sun.jpg", "two saplings close up, sun side (about 4 m)"),
        (DOCS / "ug-sapling-shade.jpg", "saplings close up, shade side, toward the sun (about 4 m)"),
        *COMMON_TAIL,
    ],
    "shrub_forest": [
        (REFS / "ref-1.jpg", "ref-1 (approved): shrubs in the undergrowth"),
        (REFS / "ref-2.jpg", "ref-2 (approved): leafy shrubs at the trunk"),
        (DOCS / "shrub_forest-eevee-three_quarter.jpg", "shrub_forest, 1.33 m tall, 2.06 m across (EEVEE)"),
        (DOCS / "shrub_forest-eevee-low_side.jpg", "shrub_forest, low side view: stems under the dome (EEVEE)"),
        (DOCS / "ug-shrub-sun.jpg", "shrub close up, sun side (about 4 m)"),
        (DOCS / "ug-shrub-shade.jpg", "shrub close up, shade side, toward the sun (about 4 m)"),
        *COMMON_TAIL,
    ],
}
W, H, PAD, LABEL = 800, 500, 12, 28
for piece, cells in SHEETS.items():
    cols, rows = 2, (len(cells) + 1) // 2
    sheet = Image.new("RGB", (cols * (W + PAD) + PAD, rows * (H + LABEL + PAD) + PAD), (30, 30, 30))
    draw = ImageDraw.Draw(sheet)
    for i, (path, label) in enumerate(cells):
        img = Image.open(path).convert("RGB")
        img.thumbnail((W, H))
        x = PAD + (i % cols) * (W + PAD)
        y = PAD + (i // cols) * (H + LABEL + PAD)
        sheet.paste(img, (x + (W - img.width) // 2, y + LABEL + (H - img.height) // 2))
        draw.text((x + 4, y + 6), label, fill=(230, 230, 230))
    out = ROOT / "docs/checkpoints" / piece / "sheet.jpg"
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out, quality=85)
    print("wrote", out)
