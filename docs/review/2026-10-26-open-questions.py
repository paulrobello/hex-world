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
    ("A52", "Mega tiles: a 7-hex piece (centre plus ring)", [
        ("Q1", "Height: how does the massif meet the world? The scoping (card 01a13bcc6f8d notes) found every seam, cliff and ground rule is built per cell."),
        ("Q2", "Networks: may roads and rivers cross the massif or end at it?"),
        ("Q3", "Tunnels: may a road tunnel run under a mega tile?"),
        ("Q4", "Placement and picker: how is it offered and placed?"),
        ("Q5", "Replace and bulldoze: what happens when a mega tile is replaced or removed?"),
        ("Q6", "Rules: the HUD mirrors the engine's placement rules in TypeScript, the biggest drift risk the scoping found. Mirror the 7-cell rules too, or have the HUD ask the engine?"),
    ], [
        ("Q1 O1", "Its 12 outer edges sit flat at the tile's own level, like a raised Meadow, and the peak rises inside the footprint (recommended: neighbours keep today's seam rules; only the interior is new ground)."),
        ("Q1 O2", "A height field across the 7 cells, with outer edges at several levels."),
        ("Q2 O1", "No roads or rivers in the first round: every outer edge is plain ground (recommended: road and river stitching stay per cell)."),
        ("Q2 O2", "Roads may cross on a pass or end at the foot."),
        ("Q3 O1", "Refused under a mega tile in the first round (recommended)."),
        ("Q3 O2", "Allowed, with the massif as cover."),
        ("Q4 O1", "Its own inventory entry; the ghost shows the whole footprint; it needs all 7 cells empty and at least one placed neighbour outside it; R turns it about the centre (recommended)."),
        ("Q4 O2", "Built from smaller pieces that merge when the ring is complete."),
        ("Q5 O1", "Removed and undone as one tile; replacing a mega tile with a single tile, or the reverse, is refused (recommended)."),
        ("Q5 O2", "Replace swaps the whole footprint for single tiles."),
        ("Q6 O1", "The HUD asks the engine through the bridge for mega-tile legality instead of mirroring it (recommended: one source of truth for the new rules)."),
        ("Q6 O2", "Mirror the rules in rules.ts with parity tests, as for single tiles."),
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
             "<p class=sub>2026-10-27, sixth round. A19 to A36 are answered (A35 O1 and A36 O1 on 2026-10-27). A19 to A42 are answered (A37 to A42 O1 on 2026-10-27). A19 to A44 are answered (A43 O1, A44 Q1 O1 and Q2 O3 on 2026-10-28). A19 to A46 are answered (A45 O1 and A46 O1 on 2026-10-28). A19 to A51 are answered (A47 to A51 O1 on 2026-10-29; A49 approved). A52 (mega tiles) is open.</p>"]
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
