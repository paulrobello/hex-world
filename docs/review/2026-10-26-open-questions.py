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
    ("A43", "Title theme on a first visit", [
        ("Q1", "A37 is built: New world plays the title theme, which hands over to the biome music after the first "
               "tile (keeping the 30 s dwell). A first-time visitor never clicks New world: the game opens straight "
               "onto a starting meadow, so they never hear it. Browsers allow sound only after the first click or key."),
    ], [
        ("O1", "Also play it for a first visit with no saved worlds, starting at the visitor's first click or key "
               "and handing over after their first tile the same way. Recommended: it is the one moment a new "
               "player meets the game."),
        ("O2", "Keep it to New world only, as built."),
    ], []),
    ("A44", "Shared scatter materials: draws met, one camera slightly slower", [
        ("Q1", "Shared materials are built and gated (branch not landed). Draws: the mixed world's overview 485 to 389, "
               "the mixed world with farms and B2 walls 547 to 445, all inside 500; thumbnails unchanged and no piece "
               "thinned. Frame time on MiniMac (4 rounds, load 2.9-5.6): every view within noise except the "
               "2,066-tile overview, which is 0.24 ms slower (45.11 against 44.84 ms, about 0.5%) in every round: that "
               "frame is GPU-bound, so fewer draws do not help and the merged draws fetch 0.7% more vertices. Lowering "
               "the merge threshold (1,024 vertices per saved draw instead of 2,048) should remove most of that, "
               "but needs another hour on MiniMac."),
        ("Q2", "The PNG fallback (GPUs with neither BC7 nor ASTC, which build no texture arrays by the GPU-memory "
               "decision) draws 528 for the mixed world with farms, over 500."),
    ], [
        ("O1", "Q1: tune the threshold first in the next quiet MiniMac slot, and land only if no camera is slower "
               "(your A28 bar). Recommended."),
        ("O2", "Q1: land now as built; the 0.24 ms overview cost is accepted and the threshold tuning follows later."),
        ("O3", "Q2: track the PNG fallback's draws (report only, like its initial download) rather than gate them. "
               "Recommended: arrays on PNG would cost 908 MiB of GPU memory."),
        ("O4", "Q2: gate PNG at 500 too, which needs another way to cut its draws."),
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
             "<p class=sub>2026-10-27, sixth round. A19 to A36 are answered (A35 O1 and A36 O1 on 2026-10-27). A19 to A42 are answered (A37 to A42 O1 on 2026-10-27). A43 and A44 are open.</p>"]
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
