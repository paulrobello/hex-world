"""Writes docs/review/2026-10-26-open-questions.html: the owner questions still open on 2026-10-26 (A25, A26),
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


SECTIONS = [
    ("A29", "Engine drops to 60 Hz while a HUD control has focus", [
        ("Q1", "Found while fixing the frame pacing judder: whenever a HUD control has keyboard focus (opening "
               "Settings focuses its tab button), the engine falls to a 60 Hz update (Bevy's default for an "
               "unfocused window). On a 120 Hz display, opening Settings halves the frame rate. "
               "O1: keep the engine at full rate when the canvas is unfocused on the web (about one line; the "
               "browser already throttles background tabs). O2: the HUD hands focus back to the canvas when a "
               "sheet closes (still 60 Hz while a sheet is open). O3: leave it. Recommended: O1."),
    ], [], []),
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
             "<p class=sub>2026-10-27, third round. A19 to A28 are answered.</p>"]
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
