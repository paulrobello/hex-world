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
    ("A34", "B2 dry-stone walls and hedgerows: built and gated, landing now", [
        ("Q1", "Approve the twelve pieces: wall_run, _b, wall_end, wall_bend, wall_junction and wall_gate (1.1 m, "
               "the boulders' granite, upright cope, closed five-bar gate between granite posts), and hedge_run, _b, "
               "hedge_end, hedge_bend, hedge_junction and hedge_oak (1.9 m, matte verge-bush green; hedge_oak stands "
               "the approved tree_oak_b, about 8 m, at its centre instead of a new oak). Corners at 120 degrees. "
               "Recommended: approve."),
        ("Q2", "Placement as built (O1 from A31): boundaries on shared edges where a farm or wheat field meets meadow, "
               "forest, oak or windmill; wall or hedge per edge from the seed, about half and half; one gate edge per "
               "farm, only where the ground inside is clear of the barn and paddock; about one hedge edge in three gets "
               "the oak; runs that would hit a hero are left out (the barn's edge keeps only its corner stubs). The "
               "junction pieces are built but never placed, since at most two edges meet at a corner. "
               "Recommended: accept."),
        ("Q3", "Draw cost, as you asked. B2 adds 48 overview draws on a 37-tile farm world (354 to 402) and 0 on the "
               "mixed river/beach/ocean test world, which has no farms. But the mixed world plus two farms is 531 "
               "draws without B2 and 549 with it, over the 500 budget: the farm tiles' own pieces are most of it, B2 "
               "adds 18. So the 500 budget fails once a world mixes water, coast and farms."),
    ], [
        ("O1", "Start shared scatter materials now (A28 option O2, colour-only pieces sharing plus texture arrays per "
               "variant group; about a week; estimated about 352 draws on the mixed world before B1, B2 and farms). "
               "Recommended, since every new piece family pushes this further."),
        ("O2", "Raise the overview budget to 600 and track it; revisit with P7 (mobile)."),
        ("O3", "Leave it: keep the mixed test world without farms at 500 and track worlds with farms."),
    ], [
        (B2_SHEET, "B2 review sheet: approved refs, Blender stills with a 1.7 m figure, the farm at play distance and "
                   "overview, the gate, a 120 degree bend, a wall meeting a hedge, and eye level in walk mode"),
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
             "<p class=sub>2026-10-27, fifth round. A19 to A33 are answered (A33 O2 on 2026-10-27). A34 answered 2026-10-27 (&quot;all approved&quot;). Nothing is open.</p>"]
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
