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
    ("A35", "Glossy foliage found by the new matte-foliage gate", [
        ("Q1", "A new kit gate requires every leaf, needle, frond and flower slot to keep a median roughness of at "
               "least 0.7 (the bushes and forest shrubs were matted from 0.50-0.64 after your shiny-bush report; "
               "grass draws at 0.78). These current slots are below it and sit on a shrink-only allow-list until you "
               "decide: the six oaks' leaf cards (median 0.58) and their canopy shells (0.62), the holly "
               "shrub_forest_b (0.37, built glossy on purpose), dune grass (0.66), ferns (0.67), and the water-lily "
               "petal and bud (0.60). Spruce needles (0.81), wildflowers (0.70), wheat (0.72) and the B2 hedges pass."),
    ], [
        ("O1", "Matte everything listed except the holly, which keeps its real-world gloss: the oaks, dune grass and "
               "ferns map onto 0.76-0.86 the way the bushes did (only the ORM green channel changes; thumbnails of "
               "oak, forest and beach tiles shift slightly), and lily flower parts leave the rule. Recommended."),
        ("O2", "Matte everything listed, holly included."),
        ("O3", "Matte only the oaks (the card's original complaint was the same sheen); leave the rest allow-listed."),
    ], []),
    ("A36", "Music bitrates against the full payload", [
        ("Q1", "With the P2 content in, one player downloads about 120 MiB in a full session (BC7 textures, WebM audio); "
               "music is 15 MiB of it (12%), all streamed in the background when a cue first plays, never in the 21 MiB "
               "initial download. Cutting Opus 128 to 96 kbps (and AAC 160 to 128) saves about 2.75 MB in a session "
               "that hears four cues; loudness, loop points and stereo width are unchanged, and the high band keeps its "
               "energy but its texture changes (measured, not heard). Lower rates (Opus 64, AAC 96) audibly risk the top "
               "end and AAC 96 breaks a loop seam, so they are off the table. Listen to meadow-day, the brightest cue: "
               "http://127.0.0.1:8095/.claude/worktrees/agent-af09c276bdfa7ba55/docs/review/music-bitrate.html"),
    ], [
        ("O1", "Keep the current bitrates (Opus 128k, AAC 160k). Recommended: music is outside every budget."),
        ("O2", "Cut to Opus 96k and AAC 128k, if you hear no difference on meadow-day."),
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
             "<p class=sub>2026-10-27, fifth round. A19 to A33 are answered (A33 O2 on 2026-10-27). A34 answered 2026-10-27 (&quot;all approved&quot;). A35 and A36 are open.</p>"]
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
