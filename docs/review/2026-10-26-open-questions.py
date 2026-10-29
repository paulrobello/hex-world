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
    ("A54", "Mega tiles: massif_granite rock texture and pines", [
        ("Q1", "Texture route for the 36 m peak. The kit gate asks for 256 px/m on a unique bake; one 2048 px rock map over a peak this size reaches about 32 px/m (31.8 px/m rock, 30.8 px/m scree), so the cliffs read soft up close (sheet: the cliff foot at ~30 m). The first build's shape is a smooth cone and is being reworked toward ref-2's buttresses and slabs now; this question is only about texturing."),
        ("Q2", "The few pines on the lower slopes in ref-2."),
    ], [
        ("Q1 O1", "Unique bake for rock and scree with a gate floor scoped to this piece at 30 px/m, like the boulders' scoped floor of 100 (recommended by the asset agent: simplest, one material, no new texture set; soft at walk-mode distance)."),
        ("Q1 O2", "Unique bake for the rock, scree on the existing gravel texture set (same rock softness, crisper scree)."),
        ("Q1 O3", "A new tileable world-space granite set for the cliffs, sharp at any distance (orchestrator's pick if close-up quality matters: one more texture set and a new material path, a larger change)."),
        ("Q2 O1", "Runtime scatter rule on the lower slopes, like every other tree and rock (recommended: your rule that scatter is never baked)."),
        ("Q2 O2", "Baked into the piece."),
    ], [(ROOT / "docs/checkpoints/massif_granite/texture-routes-sheet.jpg", "First build, headless EEVEE (Blender's view transform; no runtime grass or pines): ref-2 and ref-4, the play and low cameras, the cliff foot at ~30 m, and the three texture routes")]),
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
             "<p class=sub>2026-10-27, sixth round. A19 to A36 are answered (A35 O1 and A36 O1 on 2026-10-27). A19 to A42 are answered (A37 to A42 O1 on 2026-10-27). A19 to A44 are answered (A43 O1, A44 Q1 O1 and Q2 O3 on 2026-10-28). A19 to A46 are answered (A45 O1 and A46 O1 on 2026-10-28). A19 to A51 are answered (A47 to A51 O1 on 2026-10-29; A49 approved). A19 to A52 are answered (A52 O1 for every question, 2026-10-29). A53 answered 2026-10-29 (ref-2, as drawn). A54 (massif texture route and pines) is open.</p>"]
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
