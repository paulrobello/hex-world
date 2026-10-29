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

BAKERY_PAD = ROOT / "assets-source/kit/bakery_pad/renders/three_quarter.jpg"

SECTIONS = [
    ("A47", "Back-to-back tunnel portals (P4)", [
        ("Q1", "Two road tunnel portals placed face to face with no cover tile between them form a complete tunnel of zero length: cars enter one portal and leave the other at once. It is harmless and the spec does not set a minimum length. Keep it, or refuse a tunnel with no cover?"),
    ], [
        ("O1", "Keep it as is (recommended: no visual or routing fault, and changing it touches hex-core tunnel.rs, the HUD rules mirror and the parity tests)."),
        ("O2", "Refuse it: a tunnel needs at least one cover tile, and the HUD hint says why."),
    ], []),
    ("A48", "When to re-encode the legacy KTX2 files", [
        ("Q1", "95 of the 160 committed kit items were compressed before the encode was pinned to one thread with ASLR off (2026-10-21 and 26). Their files are valid and reproducible with the old flags, but each changes bytes once on its next re-encode, which dirties thumbnails and needs a look check. kit-compress now warns when it happens (card 01a185e1f1a6)."),
    ], [
        ("O1", "Re-encode all 95 together with the KTX-Software 5.x upgrade, as one byte change, one thumbs pass and one review (recommended: the upgrade re-encodes everything anyway)."),
        ("O2", "Re-encode all 95 now and again at the upgrade."),
        ("O3", "Leave them; each re-encodes at its next real rebuild."),
    ], []),
    ("A49", "bakery_pad piece approval", [
        ("Q1", "bakery_pad, the asphalt forecourt and two pickup spots beside the bakery, has shipped since 2026-10-19 with only its reference approved: it had no approvals.json, so the approvals page never listed it (fixed 2026-10-29). It is on docs/review/approvals.html now. Approve it?"),
    ], [], [(BAKERY_PAD, "bakery_pad, three-quarter render.")]),
    ("A50", "KTX-Software 5.0.0-rc2 measurement", [
        ("Q1", "KTX-Software 5.0 is still a release candidate (rc2, 2026-08-17). May I download rc2's macOS arm64 tools and library from the KhronosGroup/KTX-Software GitHub releases, into tools/ beside 4.4.2, to measure without shipping anything: whether the ASLR-off workaround can go, and how many KTX2 files and thumbnails the upgrade changes?"),
    ], [
        ("O1", "Yes, measure with rc2 now (recommended: it informs A48 and the upgrade card)."),
        ("O2", "No, wait for the 5.0 release."),
    ], []),
    ("A51", "Far shadow cascades on large worlds", [
        ("Q1", "On the 2,066-tile world the close camera draws about 1,328 after the per-cell cascade skip, mostly shadow cascades 2 and 3. Both ways of casting fewer LODs there were built and measured slower on MiniMac (the one-LOD pin by 4-7 ms, the LOD clamp by about 1 ms) and reverted (card 01a1781dfe02). The card's remaining idea is a coarser per-chunk shadow mesh built offline: several days of work and a new asset pass. Start it, or leave the card parked?"),
    ], [
        ("O1", "Leave it parked until large-world frame time is a problem again (recommended: the 2,066-tile world is within its budget on the reference machine)."),
        ("O2", "Start the per-chunk offline shadow LOD, measured on MiniMac against main."),
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
             "<p class=sub>2026-10-27, sixth round. A19 to A36 are answered (A35 O1 and A36 O1 on 2026-10-27). A19 to A42 are answered (A37 to A42 O1 on 2026-10-27). A19 to A44 are answered (A43 O1, A44 Q1 O1 and Q2 O3 on 2026-10-28). A19 to A46 are answered (A45 O1 and A46 O1 on 2026-10-28). A47 to A51 are open (2026-10-29).</p>"]
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
