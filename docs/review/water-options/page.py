"""Writes docs/review/2026-10-21-water-options.html from build.py's images and metrics.json.

  python3 docs/review/water-options/page.py

Serve the repo root (python3 scripts/dev/serve.py 8095 .) and open
http://127.0.0.1:8095/docs/review/2026-10-21-water-options.html."""
import html
import re
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE.parent / "2026-10-21-water-options.html"
M = json.loads((HERE / "metrics.json").read_text())
IMG = "water-options"

TRAIL = [
    (0.95, "Barely there. Sand the wash just left is only a shade darker than the damp band, so the retreat reads as the band itself."),
    (0.9, "Half of today's darkening. The trail is faint, just visible against the damp band."),
    (0.8, "Today's look (batch 14). A thin darker, glossy line follows the wash down the beach and dries back over about a second."),
    (0.7, "One and a half times today's darkening. The soaked line reads clearly as wet sand behind the retreating wash."),
    (0.6, "Twice today's darkening. A distinctly dark wet stripe trails the wash."),
]
CHURN = [
    (0.25, "A quarter of today's. Churn foam is gone in practice: only the ripple flecks remain at the fork and the bends."),
    (0.5, "Half of today's. A few faint flecks where the fork's arms meet and on the tight bend."),
    (1.0, "Today's look (batch 14). Light scattered flecks at the fork's meeting water and a faint streak on the tight bend."),
    (2.0, "Twice today's. White patches where the fork's arms meet and a white streak round the tight bend's inside."),
    (3.5, "3.5 times today's. Solid white water at the fork and a bright white band round the tight bend."),
]
STRIP_T = ["0.0 s", "0.27 s", "0.53 s", "0.8 s", "1.07 s"]
STILLS = [
    ("fork-oblique", "Fork, close"),
    ("fork-top", "Fork, from above"),
    ("bend60-oblique", "Tight bend (river_bend_60), close"),
    ("bend60-top", "Tight bend, from above"),
    ("bend-oblique", "Gentle bend (river_bend), close"),
    ("bend-top", "Gentle bend, from above"),
]
CSS = """:root{--bg:#f4f5f2;--card:#fff;--ink:#1c211e;--muted:#5b655f;--line:#d7dbd5;--accent:#2f6f4f;--cur:#eaf3ee}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--cur:#1d2b23}}
:root[data-theme=dark]{--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--cur:#1d2b23}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,sans-serif}
main{max-width:1400px;margin:0 auto;padding:20px 16px 80px}h1{font-size:22px;margin:0 0 6px}
nav{display:flex;flex-wrap:wrap;gap:6px;margin:12px 0 20px}nav a{font-size:13px;color:var(--accent);text-decoration:none;border:1px solid var(--line);border-radius:6px;padding:3px 8px}
section{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:16px;margin:0 0 18px;scroll-margin-top:12px}
h2{font-size:18px;margin:0 0 8px}h3{font-size:15px;margin:0 0 4px}p{margin:6px 0;max-width:100ch}
.top{font-size:13px;color:var(--muted);max-width:110ch}
.opt{border-top:1px solid var(--line);padding:14px 0 6px}.opt.cur{background:var(--cur);border-left:3px solid var(--accent);padding-left:12px;border-radius:4px}
.tag{display:inline-block;font-size:12px;font-weight:600;color:var(--accent);border:1px solid var(--accent);border-radius:4px;padding:0 6px;margin-left:6px}
.strip{display:grid;grid-template-columns:repeat(5,1fr);gap:6px;margin-top:8px}
.stills{display:grid;grid-template-columns:repeat(3,1fr);gap:6px;margin-top:8px}
figure{margin:0}figure img,video{width:100%;border-radius:4px;border:1px solid var(--line);display:block}
figcaption{font-size:12px;color:var(--muted)}
table{border-collapse:collapse;font-size:13px;margin:8px 0}td,th{border:1px solid var(--line);padding:3px 8px;text-align:right}th:first-child,td:first-child{text-align:left}
@media (max-width:760px){.strip{grid-template-columns:repeat(2,1fr)}.stills{grid-template-columns:1fr}}"""


def fig(src, cap, link=None):
    img = f"<img loading=lazy src='{IMG}/{src}' alt='{html.escape(cap)}'>"
    if link:
        img = f"<a href='{IMG}/{link}'>{img}</a>"
    return f"<figure>{img}<figcaption>{html.escape(cap)}</figcaption></figure>"


def option(n, value, desc, unit, body):
    cur = n == 3
    tag = "<span class=tag>current</span>" if cur else ""
    return (f"<div class='opt{' cur' if cur else ''}' id=o{n}><h3>O{n}: {unit} {value}{tag}</h3>"
            f"<p>{html.escape(desc)}</p>{body}</div>")


trail = "".join(
    option(n, v, d, "darkening x",
           "<div class=strip>" + "".join(fig(f"trail-o{n}-{k}.jpg", f"O{n} at {STRIP_T[k - 1]}", f"trail-o{n}-full.jpg" if k == 1 else None)
                                     for k in range(1, 6)) + "</div>")
    for n, (v, d) in enumerate(TRAIL, 1)
)
churn = "".join(
    option(n, v, d, "churn foam x",
           "<div class=stills>" + "".join(fig(f"churn-o{n}-{s}.jpg", f"O{n} {c}") for s, c in STILLS) + "</div>")
    for n, (v, d) in enumerate(CHURN, 1)
)
trail_rows = "".join(f"<tr><td>{r['option']} x{r['trail']}</td>" + "".join(f"<td>{v:.2f}</td>" for v in r["sand_luma"]) + "</tr>" for r in M["trail"])
churn_keys = [k for k in M["churn"][0] if k.endswith("_luma")]
churn_rows = "".join(f"<tr><td>{r['option']} x{r['churn']}</td>" + "".join(f"<td>{r[k]:.2f}</td>" for k in churn_keys) + "</tr>" for r in M["churn"])

page = f"""<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>
<title>Water Look Options</title><style>{CSS}</style></head><body><main>
<h1>Water look options, 2026-10-21 (A3)</h1>
<p class=top>Owner A3 Q1 and Q2 asked to see options for the drying trail's strength and the river churn foam's level; Q3 asked that the fallback river bed draw no drying trail. Every image is a native capture at 1280x800 on the default grade rich-3 with fixed clocks (`--clock fixed --water_clock fixed`), tilt-shift off, seed 1, the same camera per view in every option, and the review-only flag `--water_look trail=&lt;x&gt;,churn=&lt;g&gt;` (docs/agent-operability.md). The shipped defaults are unchanged: without the flag the shaders get exactly today's values, and `make thumbs` reproduces every committed thumbnail byte for byte. Each option pairs one trail value with one churn value in a single run, so both questions share the clock. Reproduce with `bash docs/review/water-options/shots.sh`, then `build.py` and `page.py` in the same folder.</p>
<nav><a href=#q1>Q1 drying trail</a><a href=#q2>Q2 churn foam</a><a href=#q3>Q3 fallback river bed</a></nav>

<section id=q1><h2>Q1: drying trail strength</h2>
<p>When the wash retreats, the sand it just left stays soaked and dries back to the damp band. The option value is the colour factor of that soaked sand (lower is darker); the dry-back time (about 1.1 s), the gloss and the damp band are the same in every option. The same factor also darkens the soaked line under the live run-up, so the edge of the wash changes with it. Each strip is a crop of the straight beach's waterline at five moments of one retreat (click the first frame for the whole view). The difference is a thin band right above the water, so the side-by-side clip shows it most clearly.</p>
<video controls loop muted playsinline preload=metadata src='{IMG}/trail-compare.mp4'></video>
<p class=top>Clip: all five options in step over 5 s of the straight beach (a run-up and a retreat), labelled in each corner.</p>
{trail}
<p class=top>Check: mean luminance of the strip crop at each moment. It falls steadily from O1 to O5 at every moment, so the knob is wired and nothing else changed.</p>
<table><tr><th>option</th>{''.join(f'<th>{t}</th>' for t in STRIP_T)}</tr>{trail_rows}</table>
</section>

<section id=q2><h2>Q2: churn foam level</h2>
<p>Churn foam is the white water where the river's flow turns or slows: the fork's meeting water and the bends, none on straights, fading out at every tile border. The option value scales it (today 1.0). Ripple flecks and the other foam (rapids, river mouth, coast) are the same in every option. It scales with how sharply the flow turns, so the tight bend and the fork show it and the gentle river_bend shows almost none at any level.</p>
<video controls loop muted playsinline preload=metadata src='{IMG}/churn-compare.mp4'></video>
<p class=top>Clip: all five options in step over 3 s of the fork (close view), labelled in each corner.</p>
{churn}
<p class=top>Check: whole-frame mean luminance per view (only the foam differs between options). It rises steadily from O1 to O5 in every view, most at the fork and the tight bend.</p>
<table><tr><th>option</th>{''.join(f'<th>{html.escape(k.removesuffix("_luma"))}</th>' for k in churn_keys)}</tr>{churn_rows}</table>
</section>

<section id=q3><h2>Q3: no drying trail on the fallback river bed</h2>
<p>Confirmed, already the current behaviour; nothing changed. The fallback (placeholder) bed is drawn only while a water tile's kit base is missing, by `assets/shaders/water_terrain.wesl`. That shader has no drying trail at all: it has no trail flag and never looks at past wash positions. On a river bed it also zeroes the live run-up (`reach = wash_amp * swash * (1 - river)`), so a fallback river bed shows only the static damp gradient at the waterline. The kit river beds (straights, bends, fork, bridges, spring) carry no wet band either.</p>
<p>One related case for the owner: the river mouth bases (`river_mouth_base`, `river_mouth_l_base`) are kit beach pieces with the wet band, so the mouth's sandy fan does draw the drying trail where it meets the sea, the same as a beach. Nothing was changed there.</p>
</section>
</main></body></html>
"""
OUT.write_text(re.sub(r"`([^`]+)`", r"<code>\1</code>", page))
print(f"wrote {OUT.relative_to(HERE.parents[2])}")
