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
    ("A25", "B1 boulders and outcrop: references", [
        ("Q1", "Approve the references. Recommended: ref-1 sets the four boulders and ref-2 the outcrop; ref-3 "
               "(meadow and forest edge) and ref-4 (coast) set placement and look only, since they read larger "
               "than the sizes in Q2."),
        ("Q2", "Sizes, all above the existing rock_small (1.9 m): boulders 2.2, 2.6, 3.4 and 4.5 m long, and an "
               "8 x 5 x 1.2 m whaleback outcrop. Recommended as listed."),
        ("Q3", "Where and how many: boulders on meadow, forest and oak tiles (about 0.3 to 0.4 per tile), the "
               "outcrop on meadow and forest (0.15 to 0.2), small boulders most common; none on farm, wheat and "
               "windmill tiles. Recommended."),
        ("Q4", "Coast: boulders stand at the grass-to-sand line, not on the wet beach (the waves ignore props, so "
               "water would run through them), and count inside the beach tiles' 0 to 6 cap. Recommended."),
        ("Q5", "Distance: every boulder stays at 150 m; past 400 m only the two big boulders and the outcrop "
               "remain, which keeps the draw budget. Recommended."),
        ("Q6", "Texture: a unique bake per piece at about 128 px/m (recommended), or one shared tileable granite "
               "with a masks map per piece (cheaper, less unique)."),
        ("Q7", "The walker cannot climb onto the outcrop. Recommended."),
    ], [], [
        (CP / "boulders-refs/refs-sheet.jpg", "ref-1 four boulders, ref-2 the outcrop, ref-3 meadow and forest "
         "edge with a 1.7 m hiker, ref-4 coast."),
    ]),
    ("A26", "B3 lily pads and reed bed: references", [
        ("Q8", "Approve the references. Recommended: ref-1 sets the pads' colour and form (dark olive, matching "
               "the spring's own pads, which they reuse); ref-3 and ref-4 are layout only (their pads came out "
               "lighter)."),
        ("Q9", "Pieces: three lily clusters (3 pads; 7 with a white flower; 5 with a pink bud, 1.8 to 2.8 m "
               "across) and a dense reed bed about 4 x 2 m and 2.4 m tall, distinct from the sparse cattail "
               "clumps on the bank. Recommended."),
        ("Q10", "Where: the spring (always lilies, a reed bed half the time) and river bends; not straights, "
                "forks or mouths. They get their own 0 to 2 per tile so the riverbank's 0 to 4 stones and reeds "
                "keep their new half and half share. Recommended."),
        ("Q11", "Never on the ocean, beach or river-mouth fan, where waves move the surface; reed beds root in "
                "shallow water 0.4 to 0.8 m deep. Recommended."),
        ("Q12", "Distance: pads drop out past 400 m (as the spring's do); the reed bed keeps a low silhouette. "
                "Recommended."),
    ], [], [
        (CP / "water-props-refs/refs-sheet.jpg", "ref-1 lily clusters, ref-2 the reed bed, ref-3 on the spring, "
         "ref-4 on a slow river bend."),
    ]),
    ("A27", "Matte forest shrubs (landed, batch 32)", [
        ("Q1", "Approve the matte hazel and bramble leaves (roughness about 0.57 to 0.81, the verge bushes' fix). "
               "The holly stays glossy; meshes and other maps are byte-identical."),
    ], [], [
        (CP / "shrub-matte/sheet.jpg", "Hazel and bramble before and after at the same cameras."),
    ]),
    ("A28", "Scatter draws on a mixed river, beach and ocean world", [
        ("Q1", "A mixed world with every scatter piece measured 548 draws against the 500 overview budget. A "
               "per-tile shadow-cascade skip (no shading change) brings it to 452 natively and 468 on the web; it "
               "lands only if its MiniMac frame-time run tonight shows no camera slower. Shared scatter materials, "
               "your preferred route, cannot remove a web draw without a bigger renderer change. Pick one: "
               "O2 vertex pulling plus texture arrays per variant group (about 352 draws, about a week, risk: "
               "thumbnail edges, memory); O3 one material per alpha mode (about 284 draws, 1 to 2 weeks, needs "
               "re-baked maps and your look review); or stop at the cascade skip for now. Recommended: stop at the "
               "cascade skip if MiniMac confirms it, and revisit O2 when scatter grows again."),
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
             "<p class=sub>2026-10-26, second round. A19, A22 and A23 are answered. Answer by code, for example "
             "\"A25 all recommended, A26 Q10 also straights\".</p>"]
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
