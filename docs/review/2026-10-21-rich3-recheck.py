"""Writes docs/review/2026-10-21-rich3-recheck.html: each family's image committed before the rich-3 re-take
(copied into docs/review/rich3-before/) next to the re-take. Run from the repo root: python3 docs/review/2026-10-21-rich3-recheck.py.
View: python3 scripts/dev/serve.py 8095 . then http://127.0.0.1:8095/docs/review/2026-10-21-rich3-recheck.html"""
import html

import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
REVIEW = ROOT / "docs/review"
NOTES = {
    "intro": (
        "Each pair shows the image committed before this re-take (left, copied into rich3-before/, labelled with its "
        "commit, date and the grade it was shot on) and the re-take on main f80b27c with the default grade rich-3 "
        "(ACES fitted at EV100 12.5 with a +0.013 linear shadow lift; every new state dump reports grade rich-3). "
        "Cameras, seed and scripts are unchanged. Most earlier images were taken on tony (TonyMcMapface EV100 12.9), "
        "the wet band and most water look-dev stills on the earlier aces look, and the river fork and river bend 60 "
        "already on rich-3, so those two pairs show only content changes since 2026-10-20. Other differences in "
        "boats, fish, gulls, grass and dust are newer content, not the grade. Click an image for full size."),
    "skirts": (
        "Grass and soil read more saturated (mean saturation +9%). Shaded skirt walls go darker: the house close-up's "
        "shaded wall measures 14/255 against 25 on tony, about the backdrop's own 14, so it separates from the "
        "backdrop only by its edge. Lit walls drop from 57 to 43. The sheet is rebuilt from the new frames "
        "(skirt-o4/sheet.py)."),
    "river": (
        "Water and banks read richer (saturation +10%) and slightly darker overall. The mouth wall facing away from "
        "the sun stays near black (8/255, tony 10), darker than the backdrop; the bridge arch underside reads as "
        "before."),
    "bend60": "Before and after are both rich-3: no grade change to check here, only content since 2026-10-20.",
    "fork": ("Before and after are both rich-3 (luma and saturation match within 0.5): the pair confirms nothing "
             "drifted since 2026-10-20."),
    "mouth": ("Sand and sea read warmer and more saturated (+9%); the fan, foam and the grass line are unchanged in "
              "shape. The shaded wall under the mirrored fan (05) is near black."),
    "beach": "Sand is a little brighter and the water more saturated (+10%).",
    "coast": ("Ocean and sand more saturated (+14%), brightness unchanged. Shaded coast walls stay dark but brown "
              "rather than black."),
    "wetband": ("Compared with the older aces look: brighter (+5%) and less saturated, so the waterline reads "
                "lighter."),
    "waterplay": ("Water more saturated (+15%). 11 to 13 (bulldoze, undo, redo) are now mostly hidden by the "
                  "bulldoze dust cloud added 2026-10-14: the script shoots before the dust clears."),
    "water": ("Compared with the older aces look (the overview with tony). These stills are the water look-dev "
              "scenario's placeholder world. The shaded river cut wall in water-edge-river is near black."),
    "pickup": ("New: three close-ups of driving pickups (green, teal, white paint) on the cars-kit road tree, taken "
               "with docs/checkpoints/pickup/pickup.json under --clock fixed. The left image is the earlier tony "
               "close-up from the cars-kit hub for colour reference."),
    "later": ("Not re-taken, per the no-new-asset-work rule: the social share image (web/brand/og-image.png) and every "
              "icon or brand image. They still show the tony look and need a re-shoot once generated images are "
              "allowed again."),
}

T14 = "9c80a87, 2026-10-14, tony"
# (section id, title, [(before folder under rich3-before, after folder relative to the repo root, before label,
#  file glob)], extra after-only files relative to the repo root)
FAMILIES = [
    ("skirts", "Hex skirts (O4 cutaway)", [("skirts", "docs/checkpoints/skirt-o4", T14, "*.jpg")], []),
    ("river", "River family: straight, bend, bridge, mouth wall (river_edge engine shots)",
     [("river-edge", "assets-source/kit/river_edge/shots", T14, "engine-*.jpg")], []),
    ("bend60", "River bend 60", [("river-bend-60", "docs/checkpoints/river_bend_60", "6d74073, 2026-10-20, rich-3",
                                  "sheet.jpg")], []),
    ("fork", "River fork", [("river-fork", "docs/checkpoints/river_fork", "681c432, 2026-10-20, rich-3 (rf-state.json)",
                             "*.jpg")], []),
    ("mouth", "River mouth and spring",
     [("river-mouth", "docs/checkpoints/river-mouth", "374b1f6 / a36663b, 2026-10-14 (state dump: tony)",
       "river-mouth-*.jpg")], []),
    ("beach", "Beach close-ups", [("beach", "assets-source/kit/beach_base/shots", T14, "beach-*.jpg")], []),
    ("coast", "Coast", [("coast", "docs/checkpoints/coast", T14, "*.jpg")], []),
    ("wetband", "Beach wet band", [("wet-band", "docs/checkpoints/wet-band",
                                    "c9a4d03, 2026-10-14 04:15, aces (before the Tony look)", "*.jpg")], []),
    ("waterplay", "Water in play", [("water-play", "docs/checkpoints/water-play", T14, "*.jpg")], []),
    ("water", "Water look-dev stills", [("water", "docs/checkpoints/water",
                                         "8f831bb, 2026-10-13, aces (overview: 9c80a87, tony)", "*.jpg")], []),
    ("pickup", "Pickup truck close-up in play",
     [("pickup", "docs/checkpoints/pickup", T14 + ", cars-kit hub close-up", "*.jpg")], []),
]


def fig(src: str, label: str) -> str:
    s = html.escape(src, quote=True)
    return (f"<figure><a href='{s}' target=_blank><img src='{s}' loading=lazy alt='{html.escape(label)}'></a>"
            f"<figcaption>{html.escape(label)}</figcaption></figure>")


def section(sid, title, groups):
    note = NOTES.get(sid, "")
    out = [f"<section id='{sid}'><h2>{html.escape(title)}</h2>"]
    if note:
        out.append(f"<div class=rec>{html.escape(note)}</div>")
    for before_dir, after_dir, label, glob in groups:
        befores = {p.name: p for p in sorted((REVIEW / "rich3-before" / before_dir).glob(glob))}
        # Older comparison stills (*-before.jpg, compare-*.jpg) in the same folders were not re-taken.
        afters = {p.name: p for p in sorted((ROOT / after_dir).glob(glob))
                  if not p.stem.endswith("-before") and not p.name.startswith("compare-")}
        if sid == "pickup":
            pairs = [(next(iter(befores.values()), None), a) for a in afters.values()]
        else:
            pairs = [(befores.get(n), afters.get(n)) for n in sorted(set(befores) | set(afters))]
        for b, a in pairs:
            name = (a or b).name
            out.append(f"<div class=pair><h3>{html.escape(name)}</h3>")
            out.append(fig("rich3-before/" + before_dir + "/" + b.name, f"Before ({label})") if b else
                       "<div class=none>No earlier image</div>")
            out.append(fig("../../" + after_dir + "/" + a.name, "After (rich-3 re-take, 2026-10-21)") if a else
                       "<div class=none>Not re-taken</div>")
            out.append("</div>")
    out.append("</section>")
    return "\n".join(out)


CSS = """:root{--bg:#f4f5f2;--card:#fff;--ink:#1c211e;--muted:#5b655f;--line:#d7dbd5;--accent:#2f6f4f;--rec:#eaf3ee}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23}}
:root[data-theme=dark]{--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--rec:#1d2b23}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,sans-serif}
main{max-width:1400px;margin:0 auto;padding:20px 16px 80px}h1{font-size:22px;margin:0 0 6px}
nav{display:flex;flex-wrap:wrap;gap:6px;margin:12px 0 20px}nav a{font-size:13px;color:var(--accent);text-decoration:none;border:1px solid var(--line);border-radius:6px;padding:3px 8px}
section{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:16px;margin:0 0 18px;scroll-margin-top:12px}
h2{font-size:18px;margin:0 0 8px}p{margin:6px 0;max-width:100ch}
.rec{background:var(--rec);border-left:3px solid var(--accent);padding:8px 12px;border-radius:4px;margin:10px 0;max-width:100ch}
.pair{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-top:14px}
.pair h3{grid-column:1/-1;font-size:12px;font-weight:600;color:var(--muted);margin:0;word-break:break-all}
figure{margin:0}figure img{width:100%;border-radius:4px;border:1px solid var(--line);display:block}
figcaption{font-size:12px;color:var(--muted)}.none{border:1px dashed var(--line);border-radius:4px;min-height:80px;display:flex;align-items:center;justify-content:center;color:var(--muted);font-size:13px;padding:12px;text-align:center}
.top{font-size:13px;color:var(--muted);max-width:110ch}
@media (max-width:640px){.pair{grid-template-columns:1fr}}"""

parts = ["<!doctype html><html lang=en><head><meta charset=utf-8>"
         "<meta name=viewport content='width=device-width,initial-scale=1'>",
         f"<title>Rich-3 Colour Re-check</title><style>{CSS}</style></head><body><main>",
         "<h1>Colour re-check on rich-3, 2026-10-21</h1>",
         f"<p class=top>{html.escape(NOTES['intro'])}</p><nav>"]
parts += [f"<a href='#{sid}'>{html.escape(title.split(':')[0])}</a>" for sid, title, _, _ in FAMILIES]
parts.append("</nav>")
parts += [section(sid, title, groups) for sid, title, groups, _ in FAMILIES]
parts.append(f"<section id='later'><h2>Left for later</h2><div class=rec>{html.escape(NOTES['later'])}</div></section>")
parts.append("</main></body></html>")
(REVIEW / "2026-10-21-rich3-recheck.html").write_text("\n".join(parts) + "\n")
print("wrote", REVIEW / "2026-10-21-rich3-recheck.html")
