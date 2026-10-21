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
    if (btn.dataset.end) { src.loopStart = +btn.dataset.start / +btn.dataset.rate; src.loopEnd = +btn.dataset.end / +btn.dataset.rate; }
    src.connect(ctx.destination); src.start();
    playing = { src, btn }; btn.textContent = 'Stop loop';
  } catch (e) {
    btn.textContent = 'Needs http: python3 -m http.server (repo root)';
  }
}
const lastVariant = {};
function randomVariant(btn, pool) {
  const players = [...document.querySelectorAll(`[data-pool="${pool}"] audio`)];
  if (!players.length) return;
  for (const a of document.querySelectorAll('audio')) a.pause();
  let i = Math.floor(Math.random() * players.length);
  if (players.length > 1 && i === lastVariant[pool]) i = (i + 1 + Math.floor(Math.random() * (players.length - 1))) % players.length;
  lastVariant[pool] = i;
  const a = players[i];
  a.currentTime = 0;
  a.play();
  const label = document.querySelector(`[data-now="${pool}"]`);
  if (label) label.textContent = 'playing ' + a.closest('.card').querySelector('h3').textContent;
}
"""


def write(layout: A.Layout, reg: dict) -> None:
    manifest = json.loads((layout.runtime / "manifest.json").read_text())
    out = layout.source / "review.html"
    rel_runtime = os.path.relpath(layout.runtime, out.parent)
    by_class: dict[str, list[str]] = {c: [] for c in CLASS_ORDER}
    for entry in manifest["cues"]:
        if not entry.get("pool"):
            by_class[entry["class"]].append(card(layout, entry, rel_runtime))
    sections = pool_sections(layout, reg, manifest, rel_runtime) + ab_sections(layout, manifest, rel_runtime) + "".join(
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


POOL_NOTE = {
    "forest-birds": ("Single birdsong phrases cut from the paid forest-birdsong take (attempt 15), whose birds the owner "
                     "liked. Each cut is high-passed at 2.8 kHz and low-passed at 9.5 kHz (two 2-pole stages each, so the "
                     "50 Hz-multiple hum comb and the hiss over 9 kHz go), then gated between notes. The game plays one at "
                     "a forest tile near the focus every 12-40 s."),
    "forest-leaves": ("Short leaf rustles, one ElevenLabs take each, high-passed at 250 Hz and gated so each settles to "
                      "silence. The game plays one at a forest tile near the focus every 20-60 s."),
    "car-door": ("A car door closing, one ElevenLabs take each, high-passed (two stages, 90 Hz for variant 2 and 60 Hz for "
                 "variant 3) to drop the sub-60 Hz boom the takes carried. car-door-1 was retired at mastering (see its "
                 "retiredNote). The game plays one at the parking spot when a car finishes parking, within 3 hex steps "
                 "of the focus, on the ambience bus."),
    "car-start": ("One variant: an engine start and gentle pull-away, high-passed at 90 Hz. It is a loud starter crank, "
                  "about 1 s of silence, then a quieter engine that drives off. car-start-1 was retired at mastering (see "
                  "its retiredNote). The game plays it at the spot when a car pulls out, within 3 hex steps of the focus, "
                  "at a rate varied by car id."),
    "car-drive": ("One variant: the receding half of a putt-putt pass-by take (car-drive-2, attempt 35), high-passed at "
                  "70 Hz and gated so it settles to silence. Three car-drive-1 takes failed (see its retiredNote). The "
                  "game plays it once per trip at a driving car within 3 hex steps of the focus, the sound following "
                  "the car, at a rate varied by car id (about 5% either way) so passes differ."),
    "farm-cow": ("A single soft moo, one ElevenLabs take each, high-passed at 60 Hz. The game plays one at a farm tile "
                 "within 3 hex steps of the focus every 30-90 s, at level 0.25 on the ambience bus."),
    "farm-sheep": ("A sheep or lamb bleat, one ElevenLabs take each, high-passed at 90 Hz. The game plays one at a farm "
                   "tile within 3 hex steps of the focus every 30-90 s (its own timer, apart from the cow), at level 0.25."),
    "windmill-creak": ("A slow wooden shaft and sail creak, one ElevenLabs take each, high-passed at 60 Hz. The game plays "
                       "one at a windmill tile within 3 hex steps of the focus every 25-70 s, at level 0.3."),
    "wheat-rustle": ("A dry wheat-ears rustle in one gust, band-passed 250 Hz-12 kHz and gated so it settles to silence. "
                     "wheat-rustle-1 is the retake (attempt 53). The game plays one at a wheat field tile within 3 hex "
                     "steps of the focus every 20-60 s, at level 0.3."),
    "shop-bell": ("A small shop-door bell, one ElevenLabs take each, high-passed at 250 Hz. Not on a timer: the game rings "
                  "one 1.2 s after a car parks at a bakery (its door closes first), at the spot, within 3 hex steps of "
                  "the focus, at level 0.35."),
    "train-whistle": ("A steam whistle, one ElevenLabs take each, high-passed at 150 Hz. Mastered, not played: there is no "
                      "rail tile or train in play yet, so only hexWorld.audio.play('train-whistle') sounds it."),
    "train-chuff": ("A steam engine chuffing past, one 6 s one-shot (not a loop), high-passed at 50 Hz. Mastered, not "
                    "played: it waits for trains, which would carry it as the cars carry car-drive."),
}


def floor_nums(prov: dict) -> str:
    """The floor criterion per runtime file (audio_lib.floor_report, recorded by make audio-master)."""
    spans = []
    for r in prov.get("runtime", []):
        fl = r.get("floor")
        if not fl:
            continue
        b = fl["floorBandPct"]
        ok = "pass" if not fl["problems"] else "FAIL"
        spans.append(f"<span>{r['path'].rsplit('.', 1)[1]} floor <b>{fl['floorBelowPeakDb']} dB</b> under peak ({ok}), "
                     f"&lt;1.5k {b['below1500']}% · 1.5–9k {b['1500to9k']}% · &gt;9k {b['above9k']}%</span>")
    return f"<div class=nums>{''.join(spans)}</div>" if spans else ""


def pool_sections(layout: A.Layout, reg: dict, manifest: dict, rel_runtime: str) -> str:
    """One-shot pools, listed first and newest review round first: each variant with its floor and screening
    numbers and its use, and a random-variant player."""
    out = []
    cue_by_id = {c["id"]: c for c in reg["cues"]}
    rnd = lambda e: ((cue_by_id.get(e["id"]) or {}).get("review") or {}).get("round", 0)  # noqa: E731
    pools = list(dict.fromkeys(e["pool"] for e in manifest["cues"] if e.get("pool")))
    pool_round = {p: max(rnd(e) for e in manifest["cues"] if e.get("pool") == p) for p in pools}
    newest = max(pool_round.values(), default=0)
    pools.sort(key=lambda p: -pool_round[p])
    shown_earlier = False
    shown_notes: set[str] = set()
    for i, pool in enumerate(pools):
        group = [e for e in manifest["cues"] if e.get("pool") == pool]
        is_new = bool(newest) and pool_round[pool] == newest
        note = next(((cue_by_id[e["id"]].get("review") or {}).get("roundNote") for e in group
                     if ((cue_by_id[e["id"]].get("review") or {}).get("round") == newest)), "") if is_new else ""
        if is_new and i == 0:
            out.append(f"<h2>Round {newest}: new one-shot pools to audition</h2><p class=lead>{html.escape(note or '')}</p>")
            shown_notes.add(note or "")
        round_lead = ""
        if i > 0 and is_new and note and note not in shown_notes:
            round_lead = f"<p class=lead>{html.escape(note)}</p>"
            shown_notes.add(note)
        if newest and not is_new and not shown_earlier:
            out.append("<h2>Earlier one-shot pools</h2>")
            shown_earlier = True
        label = "New one-shot pool" if is_new else "One-shot pool"
        out.append(f"<h2>{label}: {html.escape(pool)} ({len(group)} variants)</h2>{round_lead}"
                   f"<p class=lead>{html.escape(POOL_NOTE.get(pool, ''))} The floor numbers are the owner's 2026-10-14 "
                   "criterion: the quietest interior 50 ms windows at least 40 dB under the loudest, and not a hiss.</p>"
                   f"<p><button onclick=\"randomVariant(this, '{html.escape(pool)}')\">Play a random variant</button> "
                   f"<span class=meta data-now='{html.escape(pool)}'></span></p>"
                   f"<div class=grid data-pool='{html.escape(pool)}'>"
                   + "".join(card(layout, e, rel_runtime, cue_by_id.get(e["id"])) for e in group) + "</div>")
    return "".join(out)


def previous_card(layout: A.Layout, cue_id: str) -> str:
    """The runtime files a cue shipped before its last re-master, kept in candidates/ for an A/B."""
    base = layout.source / "candidates" / f"{cue_id}-previous"
    if not base.with_suffix(".webm").exists():
        return ""
    webm, m4a = f"candidates/{cue_id}-previous.webm", f"candidates/{cue_id}-previous.m4a"
    return f"""<div class=card><h3>{html.escape(cue_id)} (previous)</h3>
<p class=meta>The runtime file before this re-master, for comparison only.</p>
<audio controls preload=none loop><source src="{webm}" type="audio/webm; codecs=opus"><source src="{m4a}" type="audio/mp4"></audio>
<button onclick=gapless(this) data-src='{webm}'>Gapless loop (Web Audio)</button></div>"""


def ab_sections(layout: A.Layout, manifest: dict, rel_runtime: str) -> str:
    """Owner A/B: re-mastered cues against their previous files, review candidates, and new cues to audition."""
    reg = layout.registry()
    by_id = {e["id"]: e for e in manifest["cues"] + manifest.get("candidates", [])}
    out = []
    for cue in reg["cues"]:
        if cue.get("retired"):
            continue
        prev = previous_card(layout, cue["id"])
        if prev and cue["id"] in by_id:
            note = cue.get("filter") and f"New: re-mastered with the filter {html.escape(json.dumps(cue['filter']))}."
            out.append(f"<h2>A/B: {html.escape(cue['id'])}, previous against new</h2><p class=lead>{note or ''}</p>"
                       f"<div class=grid>{prev}{card(layout, by_id[cue['id']], rel_runtime)}</div>")
    targets = sorted({e["candidateFor"] for e in manifest.get("candidates", [])})
    cue_by_id = {c["id"]: c for c in reg["cues"]}
    for target in targets:
        wired = next((c.get("sourceCue") for c in reg["cues"] if c["id"] == target), None)
        cands = [e for e in manifest["candidates"] if e["candidateFor"] == target]
        out.append(f"<h2>Candidates for {html.escape(target)}</h2><p class=lead>Pick one. Until then the game's "
                   f"<code>{html.escape(target)}</code> masters <b>{html.escape(str(wired))}</b> (cues.json "
                   f"<code>sourceCue</code>). Candidates are not shipped or loaded by the game.</p>")
        rounds = sorted({(cue_by_id.get(e["id"]) or {}).get("review", {}).get("round", 1) for e in cands}, reverse=True)
        for rnd in rounds:
            group = [e for e in cands if (cue_by_id.get(e["id"]) or {}).get("review", {}).get("round", 1) == rnd]
            note = next(((cue_by_id[e["id"]].get("review") or {}).get("roundNote") for e in group
                         if (cue_by_id[e["id"]].get("review") or {}).get("roundNote")), None)
            if len(rounds) > 1:
                label = "new round" if rnd == rounds[0] else "earlier round"
                out.append(f"<h3>Round {rnd} ({label})</h3>" + (f"<p class=lead>{html.escape(note)}</p>" if note else ""))
            out.append("<div class=grid>" + "".join(card(layout, e, rel_runtime, cue_by_id.get(e["id"])) for e in group)
                       + "</div>")
    new = [by_id[c["id"]] for c in reg["cues"]
           if c.get("cut") and not c.get("pool") and c["kind"] != "music" and c["id"] in by_id]
    if new:
        out.append("<h2>New occasional one-shots</h2><div class=grid>"
                   + "".join(card(layout, e, rel_runtime) for e in new) + "</div>")
    return "".join(out)


def screening(cue: dict | None) -> str:
    """A candidate's pre-master screen (cues.json review.screening, from scripts/audio/screen.py)."""
    review_block = (cue or {}).get("review") or {}
    s = review_block.get("screening")
    if not s:
        return ""
    others = ", ".join(f"{p['hz']} Hz +{p['db']}" for p in s.get("otherTonalPeaks", [])) or "none"
    filt = review_block.get("filterApplied")
    return (f"<div class=nums><span>300 Hz–8 kHz <b>{s['band300to8kPct']}%</b></span>"
            f"<span>&lt;300 Hz <b>{s['below300HzPct']}%</b></span><span>&gt;8 kHz <b>{s['above8kPct']}%</b></span>"
            f"<span>1–6 kHz vs full <b>{s['band1to6kVsFullDb']} dB</b></span>"
            f"<span>deepest dip <b>{s['levelDeepestDipDb']} dB</b> (spread {s['levelSpreadDb']})</span>"
            f"<span>200 Hz series <b>+{s['tone200HzSeriesMaxDb']} dB</b>, other tones {html.escape(others)}</span>"
            f"<span>birds <b>{'yes' if s['birds'] else 'no'}</b> ({s['birdChirpEvents']} chirp events)</span>"
            + (f"<span>raw floor <b>{s['rawFloorBelowPeakDb']} dB</b> under peak</span>" if "rawFloorBelowPeakDb" in s else "")
            + "</div>"
            f"<p class=meta>Screened on the {html.escape(s['measuredOn'])}."
            + (f" Filter: {html.escape(filt)}." if filt else "") + "</p>")


def use_line(cue: dict | None) -> str:
    """A round-3-or-later review cue's use in the game (cues.json `use`), shown on its card."""
    if not cue or ((cue.get("review") or {}).get("round") or 0) < 3:
        return ""
    return f"<p class=meta>Use: {html.escape(cue.get('use', ''))}</p>"


def card(layout: A.Layout, entry: dict, rel_runtime: str, cue: dict | None = None) -> str:
    cid = html.escape(entry["id"])
    events = ", ".join(entry["events"]) or "none (continuous emitter or bed)"
    if entry["status"] not in ("mastered", "candidate"):
        prompt = entry.get("promptFile") or ""
        return (f"<div class=card><h3>{cid}</h3><p class=meta>{entry['kind']} · events: {html.escape(events)}</p>"
                f"<p class=pending>No source yet. Generate it in Suno from <code>{html.escape(prompt)}</code>, then run make audio-master.</p></div>")
    prov = json.loads((layout.root / entry["provenance"]).read_text())
    webm = prov["loudness"]["after"]["webm"]
    m4a = prov["loudness"]["after"]["m4a"]
    lvl = next(s for s in prov["processing"] if s["step"] == "loudness")
    files = entry["files"]
    src = os.path.normpath(f"{rel_runtime}/{files['webm']['path']}")
    m4a_src = os.path.normpath(f"{rel_runtime}/{files['m4a']['path']}")
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
    src_rec = prov["source"]
    if src_rec.get("store"):
        source_link = (f"source <code>{html.escape(src_rec['key'])}</code> in the local store "
                       f"(git-ignored, <code>assets-source/audio/music/</code> of the main checkout)")
    else:
        source_rel = os.path.relpath(layout.root / src_rec["path"], layout.source)
        source_link = f'<a href="{html.escape(source_rel)}">source</a>'
    prompt = prov.get("prompt") or (prov.get("promptFile") or {}).get("path", "")
    pi = (prov.get("parameters") or {}).get("prompt_influence")
    influence = f"<p>prompt influence {pi}</p>" if pi is not None else ""
    return f"""<div class=card><h3>{cid}</h3>
<p class=meta>{entry['kind']} · {entry['channels']} ch · {entry['playback']} · events: {html.escape(events)}</p>
<audio controls preload=none{loop_attr}><source src="{src}" type="audio/webm; codecs=opus"><source src="{m4a_src}" type="audio/mp4"></audio>
{gapless}
<div class=nums><span>length <b>{entry['durationSeconds']:.2f} s</b></span>
<span>WebM <b>{webm['integrated']:.1f} LUFS</b> / <b>{webm['truePeak']:.1f} dBTP</b></span>
<span>m4a <b>{m4a['integrated']:.1f}</b> / <b>{m4a['truePeak']:.1f}</b></span>
<span>source <b>{prov['loudness']['before']['integrated']:.1f} LUFS</b></span>
<span>runtime <b>{kb:.0f} KB</b></span></div>
<p class=meta>{html.escape(limiter)}</p>{dev}{floor_nums(prov)}{screening(cue)}{use_line(cue)}
<details><summary>Prompt and generation</summary><p>{html.escape(prompt)}</p>{influence}
<p>{html.escape(prov['provider']['name'])} · {html.escape(prov['provider']['model'])} · attempt {prov['generation'].get('attempt')} ·
id <code>{html.escape(str(prov['generation']['id']))}</code> · {source_link}</p></details>
</div>"""
