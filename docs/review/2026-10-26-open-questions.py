"""Writes docs/review/2026-10-26-open-questions.html: the owner questions still open,
each with its review images embedded, so the page works served locally or published on its own.
Run from the repo root: python3 docs/review/2026-10-26-open-questions.py
View: python3 scripts/dev/serve.py 8095 . then http://127.0.0.1:8095/docs/review/2026-10-26-open-questions.html"""
import base64
import html
import io
import pathlib

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/review/2026-10-26-open-questions.html"
CP = ROOT / "docs/checkpoints"


def img(path: pathlib.Path, width: int = 1400) -> str:
    """A JPEG data URI of `path`, scaled down to `width` px wide."""
    im = Image.open(path).convert("RGB")
    if im.width > width:
        im = im.resize((width, round(im.height * width / im.width)), Image.Resampling.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=82)
    return "data:image/jpeg;base64," + base64.b64encode(buf.getvalue()).decode()


def figure(path: pathlib.Path, caption: str, width: int = 1400) -> str:
    src = img(path, width)
    return (f'<figure><a href="{src}" target="_blank"><img src="{src}" alt="{html.escape(caption)}"></a>'
            f"<figcaption>{html.escape(caption)}</figcaption></figure>")


B1 = ROOT / ".claude/worktrees/agent-a02bc131da7e3f31b/docs/checkpoints/boulders"
SHEET_TOP = pathlib.Path("/tmp/claude-501/b1-sheet-top.jpg")
SHEET_BOTTOM = pathlib.Path("/tmp/claude-501/b1-sheet-bottom.jpg")
THUMBS = pathlib.Path("/tmp/claude-501/thumbs-cmp.jpg")
B2 = CP / "walls-hedges-refs/refs-sheet.jpg"
B3 = CP / "water-props/sheet.jpg"

SECTIONS = [
    ("A30", "B1 boulders and outcrop: built and gated, ready to land", [
        ("Q1", "Approve the five pieces: boulder_granite, _b, _c, _d (2.2 to 4.5 m) and outcrop_granite (8 x 5 x 1.3 m), "
               "built from the A25 refs at the approved sizes, scatter densities, coast band and LOD rule. They pass every "
               "asset gate. Recommended: approve."),
        ("Q2", "Rule order. The boulders and outcrop lead their tiles' scatter rules (O1), so every meadow and forest lays "
               "out again. Appended after the existing rules instead (O2), earlier props keep their spots, but the outcrop "
               "never fits (0 placed in 64 seeds on meadow and on forest). Recommended: O1."),
        ("Q3", "Thumbnails. Meadow and forest thumbnails now show boulders, and the thumbnail camera aims 2 m higher "
               "(look-at 3.5 to 5.5 m) so a spruce tip no longer touches the frame edge. Every thumbnail shifts slightly, "
               "as in the comparison below (left: now, right: with B1). Recommended: accept."),
        ("Q4", "Every-piece check. With boulders sharing the beach's 0 to 6 cap and the forest's rules reordered, the "
               "31-tile mixed world no longer happens to draw beach_driftwood's base variant or log_fallen_c, so the "
               "check that every scatter piece draws fails. O1: add a few tiles to the mixed world so every piece draws "
               "again. O2: accept the miss for those two. Recommended: O1."),
        ("Q5", "Smaller defaults as built: no outcrop on the coast band (it never fits), no boulders on beach_outer (none "
               "fit), the outcrop's bake at 109 px/m against about 128 (its size at 1024²), and the small boulders "
               "ending at 400 m as approved. Recommended: accept."),
    ], [], [
        (SHEET_TOP, "B1 review sheet, part 1: the approved refs, the Blender renders with a 1.7 m figure, and the pieces in the kit view"),
        (SHEET_BOTTOM, "B1 review sheet, part 2: in play on meadow, forest, oak and coast tiles, and at walking eye level"),
        (THUMBS, "Thumbnails: left as now, right with B1 (boulders in meadow and forest, camera look-at 2 m higher)"),
    ]),
    ("A31", "B2 dry-stone walls and hedgerows: references and layout", [
        ("Q1", "Approve refs 1 to 4 (top row) as the look and scale: dry granite walls with an upright cope, the same "
               "granite as the boulders, a timber five-bar gate between granite posts, and untrimmed hawthorn and hazel "
               "hedges in the matte verge-bush green, one with a hedgerow oak. The image model keeps drawing right-angle "
               "corners; hex boundaries meet at 120°, so corners would be built to that spec rather than copied. "
               "Recommended: approve as look and scale."),
        ("Q2", "Sizes. Wall 1.1 m tall, 0.7 m wide at the foot. Hedge 1.9 m tall, 1.5 m wide. Pieces: 4 m straight runs "
               "(two variants each), a 2 m end, a 120° bend, a three-way junction, a 4 m gateway (wall), and a 4 m run with "
               "an 8 m oak (hedge). A 16 m tile edge is then exactly 2 + 4 + 4 + 4 + 2 m. Recommended: these sizes."),
        ("Q3", "Where they go. O1: automatic field boundaries on shared tile edges, wherever a farm or wheat tile meets "
               "a grass land tile; wall or hedge picked per edge from the world seed, one gate edge per farm. Roads, "
               "rivers, rail and coast break them, and you place nothing yourself. O2: boundaries inside the farm tile "
               "(the paddock fence already does this). O3: short 4 to 8 m runs as meadow scatter. A per-edge toggle you "
               "control could come later; it needs a save-format change. Recommended: O1 now, O3 later."),
        ("Q4", "Walking: walls and hedges block the walker, gaps pass, a closed gate blocks. Recommended: yes."),
        ("Q5", "Draw cost: about 40 to 65 more draws at the overview where walls and hedges show. The mixed scatter "
               "world now sits at 452 of 500 after the cascade skip, so with B1, B3 and B2 together the overview may "
               "pass 500 again; shared scatter materials (A28) would then come back. Recommended: build, measure, and "
               "bring the numbers back here."),
    ], [], [
        (B2, "B2 reference sheet: walls_hedges refs 1 to 4 (top row) beside the approved boulder, farm and well refs"),
    ]),
    ("A32", "B3 lily pads and reed bed: built and gated, landing now", [
        ("Q1", "Approve the four pieces: lily_cluster (3 pads), lily_cluster_b (7 pads and a white lily), lily_cluster_c "
               "(5 pads and a pink bud), all using the spring's own pad so they read as one family, and reed_bed "
               "(70 common reed stems, about 3.6 x 1.8 m, 2.4 m tall, swaying in the wind), scattered on river bends, "
               "slow straights and springs at the approved densities. They pass every asset gate and are landing now with "
               "approval pending. Recommended: approve."),
        ("Q2", "Far pads. The approval says pads drop out past 400 m, but the gate needs every piece to keep a far LOD. "
               "So the far LOD keeps the biggest pads as flat 2-triangle quads (1 of 3, 7 of 7 and 3 of 5). The other way "
               "is to let a piece have no far LOD at all, a change across the gate, scatter test and exporter. "
               "Recommended: accept as built."),
        ("Q3", "Reeds on springs are rare: 8 of 168 spring tiles get a reed bed, against 68 of 168 bends, because the "
               "spring's granite ring and built-in pads leave little room. O1: accept (reeds mostly on bends). O2: a "
               "smaller reed bed variant for springs. Recommended: O1."),
        ("Q4", "At its largest scale the reed bed's leaning edge plumes can reach past its 1.85 m footprint, over a "
               "bank prop or within about 0.8 m of the tile edge. Reducing the lean needs a rebuild. Recommended: accept."),
    ], [], [
        (B3, "B3 review sheet: approved refs, Blender stills with a 1.7 m figure, the spring and a bend at play distance, "
             "close-ups, eye level, and the spring's own pads beside lily_cluster_c"),
    ]),
]

CSS = """
:root{--bg:#f6f5f2;--fg:#1d1d1b;--muted:#5d5b55;--line:#d9d6cf;--card:#fff;--accent:#2f6f4f}
@media (prefers-color-scheme:dark){:root{--bg:#161615;--fg:#e8e6e1;--muted:#a19e96;--line:#34332f;--card:#1f1f1d;--accent:#7cc39b}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 system-ui,-apple-system,sans-serif}
main{max-width:1100px;margin:0 auto;padding:24px 16px 64px}h1{font-size:1.6rem;margin:0 0 4px}
.sub{color:var(--muted);margin:0 0 28px}section{background:var(--card);border:1px solid var(--line);border-radius:10px;
padding:20px;margin:0 0 24px}h2{margin:0 0 12px;font-size:1.25rem}h2 span{color:var(--accent);margin-right:8px}
dl{margin:0 0 12px;display:grid;grid-template-columns:3.2em 1fr;gap:8px 12px}dt{font-weight:600;color:var(--accent)}
dd{margin:0}.opts dt{color:var(--fg)}figure{margin:16px 0 0}img{width:100%;height:auto;border-radius:6px;
border:1px solid var(--line)}figcaption{color:var(--muted);font-size:.9rem;margin-top:6px}
.note{color:var(--muted);font-size:.9rem}
"""


def main() -> None:
    parts = [f"<!doctype html><html lang=en><head><meta charset=utf-8>"
             f"<meta name=viewport content='width=device-width,initial-scale=1'>"
             f"<title>Hex World open questions</title><style>{CSS}</style></head><body><main>",
             "<h1>Hex World: open questions</h1>",
             "<p class=sub>2026-10-27, fourth round. A19 to A29 are answered; A30 to A32 are open.</p>"]
    for code, title, questions, options, figures in SECTIONS:
        parts.append(f"<section><h2><span>{code}</span>{html.escape(title)}</h2><dl>")
        parts += [f"<dt>{q}</dt><dd>{html.escape(t)}</dd>" for q, t in questions]
        parts.append("</dl>")
        if options:
            parts.append("<dl class=opts>")
            parts += [f"<dt>{o}</dt><dd>{html.escape(t)}</dd>" for o, t in options]
            parts.append("</dl>")
        parts += [figure(p, c) for p, c in figures]
        parts.append("</section>")
    parts.append("</main></body></html>")
    OUT.write_text("".join(parts))
    print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
