"""rail_viaduct owner review sheets (SKILL.md "Owner checkpoints"): the approved references ref-2 (form) and
ref-4 (stone and deck) beside in-engine play stills (`make shots-piece` with viaduct-shots.json: each viaduct at
level 0 between rail_straight tiles at its deck level, on a meadow).
sheet.jpg: the references, deck 2 seen every way (three-quarter, front, deck close-up, under the arches, the
seam with the raised rail tile, top), the overview and the LOD1/LOD2 stills.
heights.jpg: the three heights side by side (three-quarter, front, seam).
Usage: python3 docs/checkpoints/rail_viaduct/sheet.py (from the repo root; needs Pillow), after the shots."""
import json
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/rail_viaduct"
REFS = ROOT / "assets-source/refs/rail_viaduct"
KIT = ROOT / "assets-source/kit"
W, H = 640, 400


def report(n: int) -> tuple[dict, dict]:
    bake = json.loads((KIT / f"rail_viaduct_l{n}/bake-report.json").read_text())
    comp = json.loads((KIT / f"rail_viaduct_l{n}/compress-report.json").read_text())
    return bake, comp


def draw(cells: list[tuple[pathlib.Path, str]], cols: int, title: str, note: str, out: pathlib.Path) -> None:
    rows = (len(cells) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * W, 60 + rows * (H + 22)), (24, 24, 24))
    d = ImageDraw.Draw(sheet)
    d.text((10, 8), title, fill=(230, 230, 230))
    d.text((10, 30), note, fill=(255, 193, 7))
    for i, (path, label) in enumerate(cells):
        x, y = (i % cols) * W, 60 + (i // cols) * (H + 22)
        d.text((x + 6, y + 4), label, fill=(230, 230, 230))
        img = Image.open(path).convert("RGB")
        img.thumbnail((W - 8, H - 8))
        sheet.paste(img, (x + 4, y + 22))
    sheet.save(out, quality=85)
    print(f"wrote {out}")


tris = {n: report(n)[0]["lodTris"] for n in (1, 2, 3)}
bake2, comp2 = report(2)
dens = bake2["texelDensityPxPerM"]
kb = comp2["budgets"]["ktxBytes"]
stone = dens.get("rail_viaduct_l2_stone") or next(iter(dens.values()))
TRIS = ", ".join(f"l{n} {t[0]}/{t[1]}/{t[2]}" for n, t in tris.items())
TITLE = (f"rail_viaduct review (in-engine play stills). Tris LOD0/1/2: {TRIS}; stone {stone['median']:.0f} px/m "
         f"median; KTX2 {kb['bc7']:,} B BC7 / {kb['astc']:,} B ASTC each (one shared bake); pack village, kind center.")
NOTE = ("Deck rail top N x 4.0 + 0.46 m, ballast 0.22 m over the raised neighbour's ground (asserted per LOD). "
        "The deck is not ground (RL-Q3). Pending owner review.")
draw(
    [
        (REFS / "ref-2.jpg", "approved ref-2: the form (three tall arches on tapering piers)"),
        (REFS / "ref-4.jpg", "approved ref-4: the stone and the deck detail"),
        (DOCS / "viaduct-l2-three-quarter.jpg", "deck 2, three-quarter at play distance"),
        (DOCS / "viaduct-l2-front.jpg", "deck 2, side on: three arches, battered piers, springing 4.5 m"),
        (DOCS / "viaduct-l2-deck.jpg", "deck 2, close: coping, parapet, ballast and track"),
        (DOCS / "viaduct-l2-under.jpg", "deck 2, under the arches at walker eye level"),
        (DOCS / "viaduct-l2-seam.jpg", "deck 2 meets rail_straight at level 2: ballast and rails flush"),
        (DOCS / "viaduct-l2-top.jpg", "deck 2, top: the track runs edge 0 to edge 3"),
        (DOCS / "viaduct-overview.jpg", "overview: deck 1, 2 and 3 between their raised rail tiles"),
        (DOCS / "viaduct-l2-lod1.jpg", "deck 2, LOD1 forced (switch at 150 m)"),
        (DOCS / "viaduct-l2-lod2.jpg", "deck 2, LOD2 forced (switch at 400 m)"),
    ],
    3,
    TITLE,
    NOTE,
    DOCS / "sheet.jpg",
)
draw(
    [
        (DOCS / "viaduct-l1-three-quarter.jpg", "deck 1 (rail top 4.46 m), three-quarter"),
        (DOCS / "viaduct-l2-three-quarter.jpg", "deck 2 (rail top 8.46 m), three-quarter"),
        (DOCS / "viaduct-l3-three-quarter.jpg", "deck 3 (rail top 12.46 m), three-quarter"),
        (DOCS / "viaduct-l1-front.jpg", "deck 1, side on: springing 0.5 m, crown 2.8 m"),
        (DOCS / "viaduct-l2-front.jpg", "deck 2, side on: springing 4.5 m, crown 6.8 m"),
        (DOCS / "viaduct-l3-front.jpg", "deck 3, side on: springing 8.5 m, crown 10.8 m"),
        (DOCS / "viaduct-l1-seam.jpg", "deck 1 meets rail_straight at level 1"),
        (DOCS / "viaduct-l2-seam.jpg", "deck 2 meets rail_straight at level 2"),
        (DOCS / "viaduct-l3-seam.jpg", "deck 3 meets rail_straight at level 3"),
    ],
    3,
    "rail_viaduct heights side by side (one design, pier height scales with the deck: RL-Q2, RL-Q4).",
    NOTE,
    DOCS / "heights.jpg",
)
