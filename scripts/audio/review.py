"""Writes assets-source/audio/review.html: one player per cue for the owner's audition.

Called by master.py after every run. Deterministic (no timestamps), so an unchanged tree rewrites
the same bytes. Players reference the committed runtime files by relative path.
"""

from __future__ import annotations

import html
import json
import os

import audio_lib as A

CLASS_ORDER = ("sfx", "ui", "loop", "music")
CLASS_TITLE = {"sfx": "Placement SFX (−16 LUFS)", "ui": "UI cues (−20 LUFS)",
               "loop": "Ambience loops (−16 LUFS)", "music": "Music (−18 LUFS)"}

STYLE = """
:root { --bg:#f6f7f4; --card:#ffffff; --ink:#1d2320; --muted:#5d6762; --line:#d9ddd8; --accent:#2f6f4f; --warn:#9a5b00; }
@media (prefers-color-scheme: dark) { :root { --bg:#141816; --card:#1d2220; --ink:#e7ebe8; --muted:#a3ada8; --line:#2f3633; --accent:#7cc39c; --warn:#f0b35a; } }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink); font:15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width:1080px; margin:0 auto; padding:24px 16px 64px; }
h1 { font-size:24px; margin:0 0 4px; }
h2 { font-size:17px; margin:32px 0 12px; }
p.lead { color:var(--muted); margin:0 0 8px; max-width:70ch; }
.grid { display:grid; grid-template-columns:repeat(auto-fill, minmax(320px, 1fr)); gap:12px; }
.card { background:var(--card); border:1px solid var(--line); border-radius:8px; padding:14px; }
.card h3 { font-size:15px; margin:0 0 2px; font-family:ui-monospace, Menlo, monospace; }
.meta { color:var(--muted); font-size:13px; margin:0 0 8px; }
.nums { display:flex; flex-wrap:wrap; gap:6px 14px; font-size:13px; margin:8px 0; }
.nums b { font-variant-numeric:tabular-nums; }
audio { width:100%; margin:4px 0; }
details { font-size:13px; color:var(--muted); }
details p { margin:6px 0; color:var(--ink); }
.dev { color:var(--warn); font-size:13px; }
button { font:inherit; font-size:13px; padding:6px 10px; border-radius:6px; border:1px solid var(--line); background:transparent; color:var(--accent); cursor:pointer; min-height:32px; }
.pending { color:var(--muted); font-style:italic; }
"""

SCRIPT = """
let ctx = null, playing = null;
async function gapless(btn) {
  if (playing) { playing.src.stop(); playing.btn.textContent = 'Gapless loop (Web Audio)'; const same = playing.btn === btn; playing = null; if (same) return; }
  try {
    ctx = ctx || new AudioContext();
    const url = btn.dataset.src, buf = await ctx.decodeAudioData(await (await fetch(url)).arrayBuffer());
    const src = ctx.createBufferSource();
    src.buffer = buf; src.loop = true;
    src.loopStart = +btn.dataset.start / +btn.dataset.rate; src.loopEnd = +btn.dataset.end / +btn.dataset.rate;
    src.connect(ctx.destination); src.start();
    playing = { src, btn }; btn.textContent = 'Stop loop';
  } catch (e) {
    btn.textContent = 'Needs http: python3 -m http.server (repo root)';
  }
}
"""


def write(layout: A.Layout, reg: dict) -> None:
    manifest = json.loads((layout.runtime / "manifest.json").read_text())
    out = layout.source / "review.html"
    rel_runtime = os.path.relpath(layout.runtime, out.parent)
    by_class: dict[str, list[str]] = {c: [] for c in CLASS_ORDER}
    for entry in manifest["cues"]:
        by_class[entry["class"]].append(card(layout, entry, rel_runtime))
    sections = "".join(
        f"<h2>{CLASS_TITLE[c]}</h2><div class=grid>{''.join(cards)}</div>" for c, cards in by_class.items() if cards)
    log_path = layout.source / "generation-log.json"
    attempts = len(json.loads(log_path.read_text())["attempts"]) if log_path.exists() else 0
    doc = f"""<!doctype html>
<html lang=en><head><meta charset=utf-8><meta name=viewport content="width=device-width, initial-scale=1">
<title>Hex World Audio Review</title><style>{STYLE}</style></head>
<body><main>
<h1>Hex World audio review</h1>
<p class=lead>Every mastered cue, played from the committed runtime files (Opus in WebM, AAC m4a fallback). Numbers are
measured on the WebM by <code>make audio-master</code>. Loop cues have a gapless Web Audio player that loops at the
manifest loop points, as the game will; the plain player's <code>loop</code> attribute can gap between repeats in some browsers.
Serve the repo root over http (<code>python3 -m http.server</code>) for the gapless player.</p>
<p class=lead>Generation attempts logged: {attempts} of {reg['generationCap']}. Approvals pending: creative and owner.</p>
{sections}
</main><script>{SCRIPT}</script></body></html>
"""
    out.write_text(doc)


def card(layout: A.Layout, entry: dict, rel_runtime: str) -> str:
    cid = html.escape(entry["id"])
    events = ", ".join(entry["events"]) or "none (continuous emitter or bed)"
    if entry["status"] != "mastered":
        prompt = entry.get("promptFile") or ""
        return (f"<div class=card><h3>{cid}</h3><p class=meta>{entry['kind']} · events: {html.escape(events)}</p>"
                f"<p class=pending>No source yet. Generate it in Suno from <code>{html.escape(prompt)}</code>, then run make audio-master.</p></div>")
    prov = json.loads((layout.root / entry["provenance"]).read_text())
    webm = prov["loudness"]["after"]["webm"]
    m4a = prov["loudness"]["after"]["m4a"]
    lvl = next(s for s in prov["processing"] if s["step"] == "loudness")
    files = entry["files"]
    src = f"{rel_runtime}/{files['webm']['path']}"
    m4a_src = f"{rel_runtime}/{files['m4a']['path']}"
    loop_attr = " loop" if entry["loop"] else ""
    gapless = ""
    if entry["loop"]:
        lp = entry["loop"]
        gapless = (f"<button onclick=gapless(this) data-src='{src}' data-start={lp['startSample']} "
                   f"data-end={lp['endSample']} data-rate={lp['sampleRate']}>Gapless loop (Web Audio)</button>")
    limiter = (f"limited {lvl['requiredPeakReductionDb']} dB on {lvl['windowsOverCeilingPct']}% of 20 ms windows"
               if lvl["mode"] == "limited" else lvl["mode"])
    dev = f"<p class=dev>Deviation: {html.escape(prov['deviation'])}</p>" if prov.get("deviation") else ""
    kb = (files["webm"]["bytes"] + files["m4a"]["bytes"]) / 1024
    source_rel = os.path.relpath(layout.root / prov["source"]["path"], layout.source)
    prompt = prov.get("prompt") or (prov.get("promptFile") or {}).get("path", "")
    return f"""<div class=card><h3>{cid}</h3>
<p class=meta>{entry['kind']} · {entry['channels']} ch · {entry['playback']} · events: {html.escape(events)}</p>
<audio controls preload=none{loop_attr}><source src="{src}" type="audio/webm; codecs=opus"><source src="{m4a_src}" type="audio/mp4"></audio>
{gapless}
<div class=nums><span>length <b>{entry['durationSeconds']:.2f} s</b></span>
<span>WebM <b>{webm['integrated']:.1f} LUFS</b> / <b>{webm['truePeak']:.1f} dBTP</b></span>
<span>m4a <b>{m4a['integrated']:.1f}</b> / <b>{m4a['truePeak']:.1f}</b></span>
<span>source <b>{prov['loudness']['before']['integrated']:.1f} LUFS</b></span>
<span>runtime <b>{kb:.0f} KB</b></span></div>
<p class=meta>{html.escape(limiter)}</p>{dev}
<details><summary>Prompt and generation</summary><p>{html.escape(prompt)}</p>
<p>{html.escape(prov['provider']['name'])} · {html.escape(prov['provider']['model'])} · attempt {prov['generation'].get('attempt')} ·
id <code>{html.escape(str(prov['generation']['id']))}</code> · <a href="{html.escape(source_rel)}">source</a></p></details>
</div>"""
