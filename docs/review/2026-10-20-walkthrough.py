"""Builds docs/review/2026-10-20-walkthrough.html: every item waiting on the owner after 2026-10-20.

Run from the repo root: python3 docs/review/2026-10-20-walkthrough.py [--standalone OUT.html]
Image paths are relative to docs/review/. The build fails on a missing image. --standalone embeds
every image (downscaled to 1600 px JPEG) so the page works as a single attached file.
"""
import base64
import io
import sys
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/review/2026-10-20-walkthrough.html"
CP = "../checkpoints/"

# (id, title, subtitle, summary, recommendation, questions, [(image, caption)])
SECTIONS = [
    # --- Decisions that block work -------------------------------------------------------------
    ("D1", "Hero oak third variant: land it?", "tree_hero_oak_c (not landed)",
     "An ancient stag-headed oak: the hero's twisted trunk under a low, broken four-lobed crown with two bare dead leaders and a broken stub. 12.56 m to the dead tips (hero 13.29 m), 12 bird perches, 2580 / 1271 / 375 tris. The hero slot would split its one in three between _b and _c, so about half of today's _b cells change to _c. Every oak tile then streams all three hero oaks: about 17.9 MB of textures (BC7) instead of about 12 MB. The initial download is unaffected.",
     "Land it if the larger oak-tile stream is acceptable on the web; otherwise keep two hero oaks.",
     ["D1-Q1. Land it with the 12 to 17.9 MB oak-tile stream?",
      "D1-Q2. Dead wood is bark-coloured. A paler deadwood needs a fourth material slot. Worth it?",
      "D1-Q3. Birds also perch on the dead leaders?"],
     [("2026-10-20/tree_hero_oak_c-sheet.jpg", "review sheet"), ("2026-10-20/tree_hero_oak_c-play.jpg", "in play")]),
    ("D2", "Rail buffer stop: pick a look", "rail_buffer_stop references (reference gate)",
     "No approved ref showed a buffer stop, so three were generated. Trains now reverse 1 m inside an open line end.",
     "ref-3 (sleeper-built): its solid face reads at play distance and it fits bend and junction ends. Trains then stop 1.6 m inside the edge.",
     ["D2-Q1. ref-2 (bent rail) or ref-3 (sleepers)?",
      "D2-Q2. Move the train stop point to fit the stop (1.6 m for ref-3, 3.5 m for ref-2)?",
      "D2-Q3. Draw stops in the placement ghost at open ends? (recommend yes)",
      "D2-Q4. Leave stops out of thumbnails? (recommend yes)",
      "D2-Q5. A soft buffer clunk when a train reverses? (recommend yes)"],
     [("../../assets-source/refs/rail_buffer_stop/sheet.jpg", "ref-1, ref-2, ref-3")]),
    ("D3", "Beach dressing: driftwood and dune grass", "references (reference gate)",
     "Beach tiles read bare: their centres carry a sparse untextured dressing (grey cylinder logs, flat tufts, pebbles). No approved ref shows driftwood or marram, so references were generated.",
     "Approve both, replace the baked logs and tufts with the new pieces, keep the pebbles.",
     ["D3-Q1 (BD-A1, BD-A2). Approve the driftwood and dune_grass refs?",
      "D3-Q2 (BD-Q1). Replace the centres' baked logs and tufts, or keep both?",
      "D3-Q3 (BD-Q4). About 3 logs and 10-14 grass clumps per straight beach tile?",
      "D3-Q4 (BD-Q3). Each grass clump on its own low sand mound?"],
     [(CP + "beach-dressing/refs-sheet.jpg", "new refs (top) with the approved beach and river-mouth refs")]),
    ("D4", "Boats and lighthouse", "references (reference gate, P3 boats, P6 lighthouse)",
     "Eight refs: a rowing boat (studio, at a river landing stage, pulled up on sand), a sailing fishing boat (studio, under sail), and a white-and-red lighthouse on a granite headland with a keeper's cottage (tile, lamp room, dusk beam). Finding: the river bridges' arch crown is 0.95 m above the water, so no boat passes under; river boats stop at bridges or the arch is raised.",
     "Approve the looks; decide moorings vs moving boats before modelling.",
     ["D4-Q1 (BA1-BA3). Approve the rowing boat, sailing boat and lighthouse refs?",
      "D4-Q2 (BQ3, BQ7). River boats stop at bridges, or raise the bridge arch?",
      "D4-Q3. Boats moored in place, or rowing and sailing routes? How many?",
      "D4-Q4. Lighthouse beam speed and colour, and how it shows before day/night (P5)?",
      "Full list BQ1-BQ14 in the owner queue."],
     [(CP + "refs-boats-lighthouse/refs-sheet.jpg", "rowing boat, sailing boat, lighthouse")]),
    ("D5", "Wheat field rotation", "rotation rule",
     "Tiles with buildings or hero props now turn through all six rotations (the windmill ghost fix). The wheat field has no hero props, so it still counts as one orientation and its rows cannot be turned. Its rows look the same turned 180 degrees but not 60.",
     "Give it three distinct rotations so you can choose the row direction.",
     ["D5-Q1. Three rotations for the wheat field?"],
     [(CP + "ghost/ghost-windmill-a.jpg", "windmill ghost, rotation 1"), (CP + "ghost/ghost-windmill-b.jpg", "rotation 2 of 6")]),
    ("D6", "Terrain border rise", "meadow, forest, river, spring bases",
     "The ridge along tile borders is gone by shading (normals ignore the border ramp). Geometry is unchanged, so under a low dusk sun faint shadow lines remain from the real 0.11 m rise (0.046 m on river bases).",
     "Accept the shading fix; flatten the geometry only if the dusk lines bother you.",
     ["D6-Q1. Flatten the 0.11 m rise in geometry too? (rebuilds fence_paddock, wheat_field, two hay bales; moves walker and grass heights slightly)",
      "D6-Q2. Approve the field in play (card criterion 5)."],
     [(CP + "terrain-hump/sheet.jpg", "meadow and forest before/after"), (CP + "terrain-hump-water/sheet.jpg", "river, spring, beach before/after")]),
    # --- Your requests, check they are right ---------------------------------------------------
    ("C1", "Walk mode fords rivers, 5 m/s default", "check",
     "Rivers and spring streams no longer stop the walker: feet hold 0.6 m under the surface, eye 1.1 m above the water. The sea still stops you. Walk speed defaults to 5 m/s, sprint 10. The debug performance panel now stays visible in walk mode.",
     "Nothing to decide. A fix for the black sky in walk mode is in progress.", [],
     [(CP + "first-person/fp-04-river-bank.jpg", "mid-ford, facing the far bank"), (CP + "first-person/fp-04b-river-crossed.jpg", "far bank, looking back")]),
    ("C2", "Beach Curve and River mouth: one picker entry each", "check",
     "Headland and bay are one entry, Beach Curve; the two river mouth handednesses are one entry, River mouth. R steps through every legal orientation of both members. Tile ids and saves are unchanged.",
     "Nothing to decide.", [],
     [(CP + "beach-curve/picker-beach-curve.jpg", "the Tiles sheet"), (CP + "beach-curve/ghost-bay.jpg", "Rotation 2 of 4 on the bay")]),
    ("C3", "Windmill, farm, oak and bakery turn through six rotations", "check",
     "The ghost was drawn, but R could not turn all-grass tiles. Tiles with hero props now turn through all six rotations.",
     "Nothing to decide (see D5 for the wheat field).", [],
     [(CP + "ghost/ghost-farm-a.jpg", "farm ghost a"), (CP + "ghost/ghost-farm-b.jpg", "farm ghost b")]),
    ("C4", "Rail network, trains, crossing, station", "check, plus defaults to confirm",
     "Rail socket and tiles (straight, 60 degree bend, junction, level crossing, river rail bridge, station). Trains (engine, carriage, brake third, one livery per train) follow the track, alternate at junctions, hold at the fouling point, reverse at line ends, stop 8 s at stations, whistle on departure and chuff as they run. Cars hold 7 m short of a closed crossing and ride the crossing deck at 0.46 m. Trains route between stations is still to come.",
     "Confirm the defaults the rail agent assumed.",
     ["C4-Q1 (Q11). One livery per train from a six-colour heritage palette?",
      "C4-Q2 (Q14). Crossing gates open?",
      "C4-Q3 (Q19). Right-hand turnout only?",
      "C4-Q4 (Q20). Red lever kept?",
      "C4-Q5 (Q10). Carriage as built?"],
     [(CP + "rail/rail-junction.jpg", "a train on each junction leg"), (CP + "rail/rail-crossing.jpg", "a van held at the closed crossing"),
      (CP + "rail_station/station-play-train.jpg", "train standing at the station"), (CP + "rail/rail-walk-eye.jpg", "eye level beside the track")]),
    # --- New pieces waiting on your look -------------------------------------------------------
    ("A1", "Barn variant", "barn_red_b",
     "A lower red board barn with a catslide roof over an open lean-to shed, a ridge cupola and a hayloft door. About one farm in three.",
     None,
     ["A1-Q1. The collider is the whole roof box, so the walker stops at the eave and cannot enter the shed. Follow the closed walls instead?",
      "A1-Q2. The shed interior reads very dark at eye level. Accept?",
      "A1-Q3. Ridge or cupola as a bird perch?"],
     [(CP + "barn_red_b/sheet.jpg", "review sheet")]),
    ("A2", "Bakery variant", "bakery_shop_b",
     "A twin-gabled hipped bakehouse with a three-pot bread-oven chimney, a shelved shop window and a long tiled canopy. About one bakery in three.",
     None,
     ["A2-Q1. The back-left roof corner sits 0.08 m from the border (original 0.12 m). Pull it in?",
      "A2-Q2. The main eave runs in front of both gables (the ref breaks it). Accept?"],
     [(CP + "bakery_shop_b/sheet.jpg", "review sheet")]),
    ("A3", "Two-storey house variant", "house_twostorey_c",
     "A stone-and-render T-plan farmhouse with a front cross wing, an end stack and a door hood. The two-storey slot now splits one in three between _b and _c; about half of former _b cells became _c, and the house thumbnails changed.",
     None,
     ["A3-Q1. The walker stops in the paved corners beside the wing (box collider). Accept?",
      "A3-Q2. Stone-slate roof tone right, or browner?"],
     [(CP + "house_twostorey_c/sheet.jpg", "review sheet")]),
    ("A4", "Windmill variant", "windmill_b",
     "A white smock mill on a buff brick base with a railed stage, an ogee cap and shuttered sails. About one windmill tile in three; sails turn as the original's do.",
     None,
     ["A4-Q1. Buff brick base acceptable against 'white'?",
      "A4-Q2. Shutters on both sides of the sails a big enough change?",
      "A4-Q3. Stage painted white instead of dark timber?"],
     [(CP + "windmill_b/sheet.jpg", "review sheet")]),
    ("A5", "Scattered oak third variant", "tree_oak_c",
     "An old broad low oak, 7.0 m tall with a 9.6 m crown. About 89% of existing oak tiles re-pick at least one oak (positions mostly unchanged).",
     None,
     ["A5-Q1. Accept the re-pick in existing worlds?",
      "A5-Q2. Perch birds on it? (recommend no until Q33)"],
     [(CP + "tree_oak_c/sheet.jpg", "review sheet")]),
    ("A6", "Verge bush third variant", "bush_verge_c",
     "A flowering hawthorn-like bush, 1.05 m. Your answers are in: bush_verge stays most common (now 50 / 25 / 25%), and the blossom sits behind a season hook so P5 hides it in autumn and winter.",
     None,
     ["A6-Q1. Is the taller 1.05 m upright form wanted next to 0.88 m and 0.65 m?"],
     [(CP + "bush_verge_c/sheet.jpg", "review sheet"), (CP + "bush_verge_c/vergec-season-autumn.jpg", "season hook preview (autumn, blossom hidden)")]),
    ("A7", "Forest shrub third variant", "shrub_forest_c",
     "A bramble mound, 1.15 m tall and 2.5 m across, bright in shade where the holly went near black. Your answer is in: the 0.3 m reach past the border is fine.",
     None,
     ["A7-Q1. About 95% of forest tiles re-pick their shrubs. Accept?",
      "A7-Q2. The walker stops about 0.26 m inside the leaf skirt. Accept?"],
     [(CP + "shrub_forest_c/sheet.jpg", "review sheet")]),
    ("A8", "Spruce sapling third variant", "sapling_spruce_c",
     "A shade-suppressed young spruce: crooked stem bare to 1.1 m, five sparse level tiers. 68% of saplings re-pick; none move.",
     None,
     ["A8-Q1. Accept the re-pick in existing worlds?"],
     [(CP + "sapling_spruce_c/sheet.jpg", "review sheet")]),
    ("A9", "Red field poppy", "wildflower_single_e",
     "A scarlet poppy on a bowed stem with a nodding bud, drawn about one single flower in ten as an accent.",
     None,
     ["A9-Q1. No approved ref shows a poppy. Approve from the sheet?",
      "A9-Q2. One in ten the right share?",
      "A9-Q3. Hide poppies outside summer in P5?",
      "A9-Q4. More poppies on road and rail verges (needs per-tile weights)?"],
     [(CP + "wildflower_single_e/sheet.jpg", "review sheet")]),
    ("A10", "Hereford cow", "animal_cow_c",
     "The standing cow in a polled-Hereford coat (red-brown body, white face, belly and socks), about one cow slot in three.",
     None,
     ["A10-Q1. Its LOD2 head fix moves the collider front 1.4 cm. Keep (the same fix is being applied to the original cows)?",
      "A10-Q2. Is the darker red-brown inside the approved brown-and-white look?"],
     [(CP + "animal_cow_c/sheet.jpg", "review sheet")]),
    ("A11", "Goldfinch", "bird_goldfinch",
     "A second bird species. Flocks are one species each and alternate starling and goldfinch.",
     None,
     ["A11-Q1. No approved ref shows the plumage. Approve from the sheet, or generate refs?",
      "A11-Q2. At 4x scale it is low-contrast on meadow at play distance. Keep 4x or draw it at 5x?",
      "A11-Q3. 50/50 flocks, or keep starlings the majority?"],
     [(CP + "bird_goldfinch/sheet.jpg", "review sheet")]),
    ("A12", "Mackerel", "fish_mackerel",
     "A sea fish: fish spawned in sea water draw as mackerel, rivers and springs keep the trout.",
     None,
     ["A12-Q1. No approved ref shows a mackerel. Approve from the sheet, or generate refs?",
      "A12-Q2. The ocean-palette trout no longer appears in play. Accept?",
      "A12-Q3. Under water it reads as a dark navy silhouette. Brighten sea fish like river fish?"],
     [(CP + "fish_mackerel/sheet.jpg", "review sheet")]),
    ("A13", "Brake third carriage", "train_carriage_b",
     "A brake third with a raised roof lookout and an open tail veranda. Every train runs engine, carriage, brake third, the brake third leading when the engine pushes.",
     None,
     ["A13-Q1. No approved ref draws a second carriage design. Accept as built, or generate refs?",
      "A13-Q2. Brake third last in every train, or a mix per train?",
      "A13-Q3. 3.95 m lookout height acceptable?"],
     [(CP + "train_carriage_b/sheet.jpg", "review sheet")]),
    ("A14", "Farm animals: LOD2 neck fix", "animal_cow, _b, animal_sheep, _b, _c",
     "The dark neck and leg-junction patches at LOD2 are gone (buried AO texels refilled from their island). Only the AO maps changed.",
     "Nothing to decide beyond a look.", [],
     [(CP + "animal-lod2-neck/sheet.jpg", "before / after at forced LOD2")]),
]

LINKS = [
    ("Owner queue 2026-10-20 (text)", "2026-10-20-owner-queue.md"),
    ("Owner review 2026-10-19 (sheets and Q1-Q12)", "2026-10-19-walkthrough.html"),
    ("Owner queue 2026-10-19", "2026-10-19-owner-queue.md"),
    ("Owner queue 2026-10-14", "2026-10-14-owner-queue.md"),
    ("Audio cues to approve by ear (farm, windmill, wheat, bakery, train)", "../../assets-source/audio/review.html"),
]

OLDER = (
    "Still open from 2026-10-19 and 2026-10-20 (handoff.md, Waiting on Paul): station and rail Q10-Q20, Q27, Q28; "
    "audio Q21-Q24, Q45; forest Q25, Q26, Q31-Q35, Q46-Q48, Q60-Q62; houses Q29, Q30, Q40, Q41; oak Q49, Q50; "
    "farm Q36-Q39, Q42-Q44, Q55-Q59; Q63 (forest re-pick), Q64 (cottage re-pick), Q65 (cottage porch posts, a fix is in progress); "
    "roads Q51-Q54. Every piece landed since 2026-10-19 also awaits your look approval."
)

CSS = """:root{--bg:#f4f5f2;--card:#fff;--ink:#1c211e;--muted:#5b655f;--line:#d7dbd5;--accent:#2f6f4f;--rec:#eaf3ee;--q:#fff6df}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23;--q:#2a2516}}
:root[data-theme=dark]{--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23;--q:#2a2516}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,sans-serif}
main{max-width:1200px;margin:0 auto;padding:20px 16px 80px}h1{font-size:22px;margin:0 0 6px}h3{font-size:15px;margin:22px 0 8px;color:var(--muted)}
nav{display:flex;flex-wrap:wrap;gap:6px;margin:12px 0 20px}nav a{font-size:13px;color:var(--accent);text-decoration:none;border:1px solid var(--line);border-radius:6px;padding:3px 8px}
section{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:16px;margin:0 0 18px;scroll-margin-top:12px}
h2{font-size:18px;margin:0 0 8px}h2 span{color:var(--muted);font-weight:500}p{margin:6px 0;max-width:90ch}
.rec{background:var(--rec);border-left:3px solid var(--accent);padding:8px 12px;border-radius:4px;margin:10px 0}
ul.q{background:var(--q);border-radius:4px;padding:8px 12px 8px 30px;margin:10px 0}ul.q li{margin:4px 0}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(360px,1fr));gap:10px;margin-top:10px}
figure{margin:0}figure img{width:100%;border-radius:4px;border:1px solid var(--line);display:block}
figcaption{font-size:12px;color:var(--muted);word-break:break-word;margin-top:3px}.top{font-size:13px;color:var(--muted)}
ul.links li{margin:4px 0}"""

GROUPS = [("D", "Decisions that block work"), ("C", "Your requests: check they are right"), ("A", "New pieces waiting on your look")]


STANDALONE = "--standalone" in sys.argv


def src(path: str) -> str:
    """The image URL: the relative path, or an embedded downscaled JPEG in standalone mode."""
    if not STANDALONE:
        return path
    from PIL import Image

    im = Image.open((OUT.parent / path).resolve()).convert("RGB")
    im.thumbnail((1600, 1600))
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=82)
    return "data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode()


def section(sid, title, sub, summary, rec, qs, imgs):
    for path, _ in imgs:
        if not (OUT.parent / path).resolve().is_file():
            raise SystemExit(f"missing image for {sid}: {path}")
    imgs = [(src(p), c) for p, c in imgs]
    h = [f"<section id='{sid}'><h2>{escape(sid)} {escape(title)} <span>{escape(sub)}</span></h2>", f"<p>{escape(summary)}</p>"]
    if rec:
        h.append(f"<div class=rec><b>Recommendation:</b> {escape(rec)}</div>")
    if qs:
        h.append("<ul class=q>" + "".join(f"<li>{escape(q)}</li>" for q in qs) + "</ul>")
    h.append("<div class=grid>" + "".join(
        f"<figure><a href='{p}' target=_blank><img src='{p}' loading=lazy alt='{escape(c)}'></a><figcaption>{escape(c)}</figcaption></figure>"
        for p, c in imgs) + "</div></section>")
    return "\n".join(h)


def main() -> None:
    nav = "".join(f"<a href='#{s[0]}'>{s[0]}</a>" for s in SECTIONS)
    body = []
    for prefix, label in GROUPS:
        body.append(f"<h3>{escape(label)}</h3>")
        body += [section(*s) for s in SECTIONS if s[0].rstrip("0123456789") == prefix]
    links = "".join(f"<li><a href='{escape(u)}'>{escape(t)}</a></li>" for t, u in LINKS)
    html = (
        "<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>"
        f"<title>Owner Review 2026-10-20</title><style>\n{CSS}\n</style></head><body><main>\n"
        "<h1>Owner review, 2026-10-20</h1>\n"
        "<p class=top>Everything waiting on you after the 2026-10-20 session. D items block work, C items are your requests to check, A items are new pieces waiting on your look. "
        "Click an image for full size. Running build: <a href='http://127.0.0.1:8094/'>http://127.0.0.1:8094/</a>. "
        "Answer by code (for example D2-Q1: ref-3).</p>\n"
        f"<nav>{nav}<a href='#older'>Older</a></nav>\n" + "\n".join(body) +
        f"\n<h3>Older and other review pages</h3><section id='older'><p>{escape(OLDER)}</p><ul class=links>{links}</ul></section>\n"
        "</main></body></html>\n"
    )
    out = Path(sys.argv[sys.argv.index("--standalone") + 1]) if STANDALONE else OUT
    out.write_text(html)
    print(f"wrote {out}: {len(SECTIONS)} sections, {out.stat().st_size // 1024} KiB")


if __name__ == "__main__":
    main()
