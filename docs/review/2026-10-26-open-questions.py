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
B2_SHEET = ROOT / ".claude/worktrees/agent-a166db2800bc9c429/docs/checkpoints/walls-hedges/sheet.jpg"

SECTIONS = [
    ("A37", "Where the title theme plays", [
        ("Q1", "title-theme (179 s, approved with the music set) is mastered, but the game has no title screen and no "
               "event plays it, so it never sounds. As of today it is left out of the deployed site (it was 7 MB "
               "of the deploy that nothing fetched) until something plays it. Browsers only allow sound after the "
               "first click, so it cannot play over the opening load."),
    ], [
        ("O1", "Play it when a new, empty world starts (after the New world click), fading into the biome music "
               "once the first tiles go down. Recommended: the closest moment to a title screen the game has now."),
        ("O2", "Play it on the world list (Worlds sheet) while it is open."),
        ("O3", "Keep it unshipped until a title or menu screen exists."),
    ], []),
    ("A38", "P4: how a player sets a tile's height", [
        ("Q1", "P4 adds height levels 0-3 (4 m a level). This answer shapes the data model, the save migration and "
               "whether the per-rotation thumbnails stay valid, so it comes first."),
    ], [
        ("O1", "Auto, like auto-rotate: a tile takes the level its neighbours imply, and R/scroll cycles the legal "
               "levels when more than one fits. Tiles store heights relative to their own level, so thumbnails stay "
               "as they are. Recommended."),
        ("O2", "A raise/lower tool: placed tiles are lifted or lowered with keys or a bar tool; placement checks "
               "the edges."),
        ("O3", "Height is part of the tile type: plateau, slope and cliff tiles in the inventory, no level choice."),
    ], []),
    ("A39", "P4: rail and road grade", [
        ("Q1", "A one-tile ramp climbs 4 m over 27.7 m, about 14%. That reads fine for a stylised road but is far "
               "too steep for a steam train (real lines stay under about 3%)."),
    ], [
        ("O1", "Roads ramp in one tile; rail never changes level and crosses height on viaducts and in tunnels. "
               "Recommended."),
        ("O2", "Rail ramps span several tiles, which needs half or quarter edge heights and breaks the "
               "whole-level rule."),
        ("O3", "Accept a stylised 14% rail ramp."),
    ], []),
    ("A40", "P4: mountains", [
        ("Q1", "How big a mountain is."),
    ], [
        ("O1", "Single-tile hills and peaks that rise inside the hex and meet neighbours at their edge level. "
               "Recommended for P4; bigger massifs come with the 7-hex mega-tile card."),
        ("O2", "Mountains built by stacking levels (level-3 tiles with cliff and slope edges)."),
        ("O3", "Wait for mega tiles and make mountains 7-hex pieces."),
    ], []),
    ("A41", "P4: rivers that flow downhill", [
        ("Q1", "River lanes gain a flow direction, placement refuses a river that would run uphill, and waterfalls "
               "form where a river meets a cliff edge. Today's rivers have no direction."),
    ], [
        ("O1", "Saves load with flow derived from the sea and height (flat rivers keep flowing as now); new placement "
               "enforces downhill. Recommended."),
        ("O2", "Rivers stay directionless on level ground; direction only matters on slopes and at waterfalls."),
    ], []),
    ("A42", "P4: how mountains and cliffs are modelled", [
        ("Q1", "PRD §11 sends mountains, cliffs and the volcano through live Blender MCP sculpting, but your rule "
               "since 2026-10-13 is headless Blender only."),
    ], [
        ("O1", "Headless procedural generators (displacement, erosion-style noise, baked like every piece so far). "
               "Recommended; the PRD gets corrected."),
        ("O2", "Lift the headless rule for these hero pieces and sculpt them live."),
    ], []),
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
             "<p class=sub>2026-10-27, sixth round. A19 to A36 are answered (A35 O1 and A36 O1 on 2026-10-27). A37 to A42 answered 2026-10-27 (&quot;use recommended options for all open questions&quot;: O1 for each). Nothing is open.</p>"]
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
