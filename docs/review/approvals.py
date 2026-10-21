"""Builds docs/review/approvals.html: every kit piece and audio cue still waiting on the owner.

Run from the repo root: python3 docs/review/approvals.py
The page reads the tree as it is: a piece is listed while its newest piece-gate decision in
approvals.json is not an owner approval (a rebuild's pending re-approval request lists it again),
a cue until its provenance records the owner's approval. Decisions made on the
page stay in the browser; "Copy answers" puts them on the clipboard to paste into chat, and the
orchestrator records them. Rerun after each landing to drop what was approved.
"""
import json
from datetime import date
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/review/approvals.html"
UP = "../../"

FAMILIES = [
    ("Farm", ("animal_", "hay_", "barn_", "fence_", "wheat_", "windmill")),
    ("Bakery", ("bakery_",)),
    ("Rail", ("rail_", "train_", "river_rail")),
    ("Boats and coast", ("boat_", "beach_", "driftwood", "dune_", "lighthouse", "landing_")),
    ("Rivers", ("river_",)),
    ("Roads", ("road_", "house_pad", "car_")),
    ("Houses", ("house_",)),
    ("Trees", ("tree_", "sapling_")),
    ("Plants and props", ("shrub_", "fern_", "log_", "rock_", "wildflower_", "bush_", "cattail", "reed")),
    ("Wildlife", ("bird_", "fish_")),
]


def family(pid: str) -> str:
    for name, prefixes in FAMILIES:
        if pid.startswith(prefixes):
            return name
    return "Other"


def owner_approved(rows: list) -> bool:
    """Whether the newest owner decision or re-approval request on the piece gate is an owner
    approval: a pending or rejected entry after an approval (a rebuild sent back for review)
    lists the piece again."""
    approved = False
    for r in rows:
        if r.get("gate") != "piece":
            continue
        who = str(r.get("who", ""))
        status = r.get("status", "approved")
        if status in ("pending", "rejected") or who.startswith("pending"):
            approved = False
        elif "owner" in who or "Paul Robello" in who:
            approved = True
    return approved


def pending_pieces() -> list[dict]:
    manifest = {p["id"]: p for p in json.loads((ROOT / "assets/kit/manifest.json").read_text())["pieces"]}
    out = []
    for f in sorted((ROOT / "assets-source/kit").glob("*/approvals.json")):
        pid = f.parent.name
        rows = json.loads(f.read_text())
        if owner_approved(rows):
            continue
        note = next((r.get("notes", "") for r in reversed(rows) if r.get("gate") == "piece"), "")
        prov = f.parent / "provenance.json"
        lods = json.loads(prov.read_text()).get("lods", []) if prov.exists() else []
        images = []
        cp = ROOT / "docs/checkpoints" / pid
        if (cp / "sheet.jpg").exists():
            images.append(f"docs/checkpoints/{pid}/sheet.jpg")
        renders = f.parent / "renders"
        for view in ("three_quarter", "front", "left", "back", "right", "top"):
            if (renders / f"{view}.jpg").exists():
                images.append(f"assets-source/kit/{pid}/renders/{view}.jpg")
        if cp.is_dir():
            images += [f"docs/checkpoints/{pid}/{p.name}" for p in sorted(cp.glob("*play*.jpg"))][:3]
        out.append({
            "id": pid,
            "family": family(pid),
            "pack": manifest.get(pid, {}).get("pack", "not in manifest"),
            "tris": " / ".join(str(l.get("tris")) for l in lods),
            "note": note,
            "images": images,
        })
    return out


def pending_cues() -> list[dict]:
    manifest = json.loads((ROOT / "assets/audio/manifest.json").read_text())
    out = []
    for c in manifest["cues"]:
        prov = ROOT / c["provenance"] if c.get("provenance") else None
        approvals = json.loads(prov.read_text()).get("approvals", {}) if prov and prov.exists() else {}
        if isinstance(approvals, dict) and approvals.get("owner", "pending") != "pending":
            continue
        files = c.get("files", {})
        out.append({
            "id": c["id"],
            "kind": c["kind"],
            "events": ", ".join(c.get("events") or []),
            "seconds": c.get("durationSeconds"),
            "loop": bool(c.get("loop")),
            "webm": files.get("webm", {}).get("path"),
            "m4a": files.get("m4a", {}).get("path"),
        })
    return out


CSS = """:root{--bg:#f4f5f2;--card:#fff;--ink:#1c211e;--muted:#5b655f;--line:#d7dbd5;--accent:#2f6f4f;--ok:#2f6f4f;--okbg:#e3f1e8;--fix:#9a4a12;--fixbg:#fbeadb;--skip:#5b655f;--skipbg:#ecedea}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--ok:#7cc39c;--okbg:#1d2b23;--fix:#f0a46a;--fixbg:#33241a;--skip:#a1aba5;--skipbg:#232725}}
:root[data-theme=dark]{--bg:#121513;--card:#1b1f1d;--ink:#e6eae7;--muted:#a1aba5;--line:#2c3330;--accent:#7cc39c;--ok:#7cc39c;--okbg:#1d2b23;--fix:#f0a46a;--fixbg:#33241a;--skip:#a1aba5;--skipbg:#232725}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,sans-serif}
main{max-width:1240px;margin:0 auto;padding:20px 16px 120px}h1{font-size:22px;margin:0 0 4px}.top{font-size:13px;color:var(--muted);margin:0 0 12px;max-width:90ch}
.bar{display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin:12px 0}.bar button,.bar a{font:13px system-ui,sans-serif;color:var(--accent);background:none;border:1px solid var(--line);border-radius:6px;padding:4px 9px;cursor:pointer;text-decoration:none}
.bar button[aria-pressed=true]{background:var(--accent);color:var(--card);border-color:var(--accent)}
h2{font-size:18px;margin:28px 0 4px;scroll-margin-top:12px}h2 small{color:var(--muted);font-weight:500;font-size:14px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:12px;margin-top:10px}
.card{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:12px;display:flex;flex-direction:column;gap:8px}
.card[data-d=approve]{border-color:var(--ok);box-shadow:inset 4px 0 0 var(--ok)}.card[data-d=changes]{border-color:var(--fix);box-shadow:inset 4px 0 0 var(--fix)}.card[data-d=skip]{opacity:.6}
.card h3{font-size:15px;margin:0;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;word-break:break-all}.meta{font-size:12px;color:var(--muted)}
.hero{width:100%;aspect-ratio:4/3;object-fit:contain;background:var(--bg);border:1px solid var(--line);border-radius:4px;cursor:zoom-in;display:block}
.strip{display:flex;gap:4px;overflow-x:auto}.strip img{height:46px;border:1px solid var(--line);border-radius:3px;cursor:pointer;flex:none}.strip img.on{outline:2px solid var(--accent)}
details{font-size:13px;color:var(--muted)}details p{margin:4px 0}audio{width:100%}
.dec{display:flex;gap:6px}.dec button{flex:1;font:13px system-ui,sans-serif;padding:6px 4px;border-radius:6px;border:1px solid var(--line);background:none;color:var(--ink);cursor:pointer}
.dec button[data-v=approve][aria-pressed=true]{background:var(--okbg);border-color:var(--ok);color:var(--ok);font-weight:600}
.dec button[data-v=changes][aria-pressed=true]{background:var(--fixbg);border-color:var(--fix);color:var(--fix);font-weight:600}
.dec button[data-v=skip][aria-pressed=true]{background:var(--skipbg);border-color:var(--skip);font-weight:600}
textarea{width:100%;min-height:34px;font:13px system-ui,sans-serif;background:var(--bg);color:var(--ink);border:1px solid var(--line);border-radius:6px;padding:5px 7px;resize:vertical}
.groupbtn{font:12px system-ui,sans-serif;color:var(--accent);background:none;border:1px solid var(--line);border-radius:6px;padding:2px 8px;cursor:pointer;margin-left:8px;vertical-align:2px}
footer{position:fixed;left:0;right:0;bottom:0;background:var(--card);border-top:1px solid var(--line);padding:10px 16px;display:flex;gap:10px;align-items:center;justify-content:center;flex-wrap:wrap;font-size:14px}
footer button{font:14px system-ui,sans-serif;padding:7px 14px;border-radius:6px;border:1px solid var(--accent);background:var(--accent);color:var(--card);cursor:pointer}footer button.ghost{background:none;color:var(--accent)}
#lb{position:fixed;inset:0;background:rgba(0,0,0,.85);display:none;align-items:center;justify-content:center;z-index:9;cursor:zoom-out}#lb img{max-width:96vw;max-height:94vh}
.hidden{display:none!important}"""

JS = r"""
const KEY='hexworld-approvals-'+document.body.dataset.gen;
let state={};try{state=JSON.parse(localStorage.getItem(KEY)||'{}')}catch(e){}
const save=()=>{try{localStorage.setItem(KEY,JSON.stringify(state))}catch(e){}};
const cards=[...document.querySelectorAll('.card')];
function paint(card){const s=state[card.dataset.id]||{};card.dataset.d=s.d||'';
  card.querySelectorAll('.dec button').forEach(b=>b.setAttribute('aria-pressed',b.dataset.v===s.d));
  const t=card.querySelector('textarea');if(t&&t.value!==(s.n||''))t.value=s.n||'';}
function count(){const v=Object.values(state);const n=k=>v.filter(s=>s.d===k).length;
  document.getElementById('tally').textContent=`${n('approve')} approved · ${n('changes')} need changes · ${n('skip')} skipped · ${cards.length-v.filter(s=>s.d).length} undecided`;}
cards.forEach(card=>{paint(card);
  card.querySelectorAll('.dec button').forEach(b=>b.onclick=()=>{const s=state[card.dataset.id]||{};s.d=s.d===b.dataset.v?'':b.dataset.v;state[card.dataset.id]=s;save();paint(card);count();filter();});
  const t=card.querySelector('textarea');if(t)t.oninput=()=>{const s=state[card.dataset.id]||{};s.n=t.value;state[card.dataset.id]=s;save();};
  card.querySelectorAll('.strip img').forEach(img=>img.onclick=()=>{const h=card.querySelector('.hero');h.src=img.src;card.querySelectorAll('.strip img').forEach(i=>i.classList.toggle('on',i===img));});});
document.querySelectorAll('.groupbtn').forEach(b=>b.onclick=()=>{document.querySelectorAll(`.card[data-g="${b.dataset.g}"]`).forEach(c=>{if(c.classList.contains('hidden'))return;const s=state[c.dataset.id]||{};if(!s.d){s.d='approve';state[c.dataset.id]=s;paint(c);}});save();count();filter();});
let view='all';
function filter(){cards.forEach(c=>{const d=(state[c.dataset.id]||{}).d||'';c.classList.toggle('hidden',view==='undecided'?!!d:view==='all'?false:d!==view);});
  document.querySelectorAll('h2[data-g]').forEach(h=>{const any=[...document.querySelectorAll(`.card[data-g="${h.dataset.g}"]`)].some(c=>!c.classList.contains('hidden'));h.classList.toggle('hidden',!any);h.nextElementSibling.classList.toggle('hidden',!any);});}
document.querySelectorAll('[data-view]').forEach(b=>b.onclick=()=>{view=b.dataset.view;document.querySelectorAll('[data-view]').forEach(x=>x.setAttribute('aria-pressed',x===b));filter();});
function answers(){const by=k=>cards.filter(c=>(state[c.dataset.id]||{}).d===k);
  const line=c=>{const s=state[c.dataset.id];return `- ${c.dataset.id}${s.n?': '+s.n.trim():''}`;};
  const out=[`Approvals page answers (docs/review/approvals.html, generated ${document.body.dataset.gen})`];
  const a=by('approve'),f=by('changes'),k=by('skip');
  if(a.length){out.push('','Approved:');a.forEach(c=>out.push(line(c)));}
  if(f.length){out.push('','Needs changes:');f.forEach(c=>out.push(line(c)));}
  if(k.length){out.push('','Skipped for now:');k.forEach(c=>out.push(line(c)));}
  return out.join('\n');}
document.getElementById('copy').onclick=async e=>{const t=answers();try{await navigator.clipboard.writeText(t);e.target.textContent='Copied';}catch(_){document.getElementById('out').value=t;document.getElementById('out').classList.remove('hidden');e.target.textContent='Select the text below';}setTimeout(()=>e.target.textContent='Copy answers',1800);};
document.getElementById('reset').onclick=()=>{if(confirm('Clear every decision on this page?')){state={};save();cards.forEach(paint);count();filter();}};
const lb=document.getElementById('lb');document.querySelectorAll('.hero').forEach(h=>h.onclick=()=>{lb.querySelector('img').src=h.src;lb.style.display='flex';});lb.onclick=()=>lb.style.display='none';
document.addEventListener('keydown',e=>{if(e.key==='Escape')lb.style.display='none';});
document.addEventListener('play',e=>{document.querySelectorAll('audio').forEach(a=>{if(a!==e.target)a.pause();});},true);
count();filter();
"""


def piece_card(p: dict) -> str:
    imgs = p["images"]
    hero = f'<img class=hero loading=lazy src="{UP}{escape(imgs[0])}" alt="{escape(p["id"])}">' if imgs else "<p class=meta>No image found.</p>"
    strip = "".join(
        f'<img loading=lazy src="{UP}{escape(i)}" alt="" title="{escape(Path(i).name)}"{" class=on" if n == 0 else ""}>' for n, i in enumerate(imgs)
    )
    note = f"<details><summary>Agent notes</summary><p>{escape(p['note'])}</p></details>" if p["note"] else ""
    return (
        f'<div class=card data-id="{escape(p["id"])}" data-g="{escape(p["family"])}"><h3>{escape(p["id"])}</h3>'
        f'<div class=meta>pack {escape(p["pack"])} · tris {escape(p["tris"] or "?")} (LOD0 / 1 / 2)</div>'
        f"{hero}<div class=strip>{strip}</div>{note}{decision()}</div>"
    )


def cue_card(c: dict, group: str) -> str:
    src = "".join(
        f'<source src="{UP}assets/audio/{escape(path)}" type="audio/{kind}">'
        for path, kind in ((c["webm"], "webm"), (c["m4a"], "mp4")) if path
    )
    loop = " loop" if c["loop"] else ""
    meta = f'{escape(c["kind"])} · {c["seconds"]} s' + (" · loops" if c["loop"] else "") + (f' · plays on {escape(c["events"])}' if c["events"] else "")
    return (
        f'<div class=card data-id="{escape(c["id"])}" data-g="{escape(group)}"><h3>{escape(c["id"])}</h3>'
        f'<div class=meta>{meta}</div><audio controls preload=none{loop}>{src}</audio>{decision()}</div>'
    )


def decision() -> str:
    return (
        '<div class=dec><button data-v=approve>Approve</button><button data-v=changes>Needs changes</button>'
        '<button data-v=skip>Skip</button></div><textarea placeholder="Note (optional): what to change, or why"></textarea>'
    )


def section(name: str, cards: list[str]) -> str:
    return (
        f'<h2 id="{escape(name)}" data-g="{escape(name)}">{escape(name)} <small>{len(cards)}</small>'
        f'<button class=groupbtn data-g="{escape(name)}">Approve all undecided here</button></h2>'
        f'<div class=grid>{"".join(cards)}</div>'
    )


def main() -> None:
    pieces, cues = pending_pieces(), pending_cues()
    cue_groups = {"Music": "music", "Sound effects": "sfx", "Ambience one-shots": "ambience"}
    sections = []
    for label, kind in cue_groups.items():
        cards = [cue_card(c, label) for c in cues if c["kind"] == kind]
        if cards:
            sections.append((label, cards))
    for name in [f for f, _ in FAMILIES] + ["Other"]:
        cards = [piece_card(p) for p in pieces if p["family"] == name]
        if cards:
            sections.append((name, cards))
    gen = date.today().isoformat()
    nav = "".join(f'<a href="#{escape(n)}">{escape(n)} ({len(c)})</a>' for n, c in sections)
    html = (
        "<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>"
        f"<title>Asset Approvals</title><style>{CSS}</style></head><body data-gen='{gen}'><main>"
        f"<h1>Asset approvals</h1><p class=top>Everything still waiting on your look or your ear: {len(cues)} sounds and {len(pieces)} kit pieces, "
        f"read from the tree on {gen}. Mark each one, add a note where something should change, then Copy answers and paste the text into chat. "
        "Decisions stay in this browser until you clear them. Click a picture to enlarge it, or a small view to swap it in.</p>"
        "<div class=bar><button data-view=all aria-pressed=true>All</button><button data-view=undecided aria-pressed=false>Undecided</button>"
        "<button data-view=approve aria-pressed=false>Approved</button><button data-view=changes aria-pressed=false>Needs changes</button>"
        f"<button data-view=skip aria-pressed=false>Skipped</button></div><div class=bar>{nav}</div>"
        + "".join(section(n, c) for n, c in sections)
        + "</main><footer><span id=tally></span><button id=copy>Copy answers</button><button id=reset class=ghost>Clear</button>"
        "<textarea id=out class=hidden readonly rows=6 style='max-width:600px'></textarea></footer>"
        f"<div id=lb><img alt=''></div><script>{JS}</script></body></html>"
    )
    OUT.write_text(html)
    print(f"wrote {OUT.relative_to(ROOT)}: {len(cues)} cues, {len(pieces)} pieces")


if __name__ == "__main__":
    main()
