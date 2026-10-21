"""river_bend_60 review sheet: docs/checkpoints/river_bend_60/sheet.jpg.

  python3 docs/checkpoints/river_bend_60/run.py      (from the repo root, after cargo build -p hex-app --release)

Shoots play.json in play (the tight bend on (1, 0) between a river straight on (2, 0) and the wide
river_bend on (1, 1), which runs on into a straight on (0, 2), framed by meadows): play distance, a
high view twice 30 frames apart (water moving on the fixed water clock), both seams close up, the
inner and outer banks close up and eye level in walk mode. Shots are `photo_export`s (offscreen):
window screenshots came back black at high host load (pitfalls.md, "Look-dev and engine shots").
Each PNG's mean is checked and a run with a black frame is retried once. Not a timing measurement,
so it does not wait on host load. The sheet adds the approved references and both pieces' six-view
renders. Needs Pillow."""
import pathlib
import subprocess
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFont, ImageStat

ROOT = pathlib.Path(__file__).resolve().parents[3]
DOCS = ROOT / "docs/checkpoints/river_bend_60"
SHOTS = ROOT / "target/shots"
NAMES = ("rb60-play", "rb60-top-a", "rb60-top-b", "rb60-seam-straight", "rb60-seam-bend", "rb60-inner-close",
         "rb60-outer-close", "rb60-eye")
LABELS = {
    "rb60-play": "play distance: tight bend (1,0) between a straight (2,0) and the wide bend (1,1)",
    "rb60-top-a": "high view, frame A",
    "rb60-top-b": "high view, frame A + 30 (water moving)",
    "rb60-seam-straight": "seam with river_straight (edge 0), close",
    "rb60-seam-bend": "seam with the wide river_bend (edge 1), close",
    "rb60-inner-close": "inner bank and knob, close (sun side)",
    "rb60-outer-close": "outer bank, close (shade side)",
    "rb60-eye": "eye level, walk mode, 1.7 m",
}
VIEWS = ("front", "back", "left", "right", "top", "three_quarter")


def mean(p):
    return sum(ImageStat.Stat(Image.open(p).convert("L")).mean) / 255.0


def shoot():
    log = ROOT / "target/rb60-play.log"
    for attempt in range(2):
        for n in NAMES:
            (SHOTS / f"{n}.png").unlink(missing_ok=True)
        with open(log, "w") as f:
            r = subprocess.run(["caffeinate", "-u", "-d", "-t", "900", ROOT / "target/release/hex-world", "--script",
                                DOCS / "play.json", "--scenario", "play", "--size", "1280x800", "--seed", "1",
                                "--fps", "off", "--tilt", "off", "--clock", "fixed", "--water_clock", "fixed"],
                               cwd=ROOT, stdout=f, stderr=subprocess.STDOUT)
        shots = [SHOTS / f"{n}.png" for n in NAMES]
        if r.returncode == 0 and all(p.exists() and mean(p) > 0.03 for p in shots):
            return
        print(f"run {attempt}: exit {r.returncode} or a black or missing frame, retrying once")
    sys.exit(f"still black or missing after a retry (log {log})")


def fit(im, w, h):
    im = im.convert("RGB")
    s = min(w / im.width, h / im.height)
    return im.resize((int(im.width * s), int(im.height * s)), Image.LANCZOS)


def sheet():
    pad, label = 12, 26
    try:
        f = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 18)
        fb = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 26)
    except OSError:
        f = fb = ImageFont.load_default()
    rows = []
    refs = [(f"approved ref ({p.stem})", Image.open(p)) for p in sorted((ROOT / "assets-source/refs/river").glob("ref-*.jpg"))]
    rows.append((refs, 330))
    for piece in ("river_bend_60_base", "river_bend_60_center"):
        rows.append(([(f"{piece} {v} (six-view, untextured)", Image.open(ROOT / f"assets-source/kit/{piece}/renders/{v}.jpg"))
                      for v in VIEWS[:3]], 300))
        rows.append(([(f"{piece} {v} (six-view, untextured)", Image.open(ROOT / f"assets-source/kit/{piece}/renders/{v}.jpg"))
                      for v in VIEWS[3:]], 300))
    play = [(LABELS[n], Image.open(SHOTS / f"{n}.png")) for n in NAMES]
    for i in range(0, len(play), 2):
        rows.append((play[i:i + 2], 500))
    W = 1640
    H = 60 + sum(label + h + pad for _, h in rows) + pad
    s = Image.new("RGB", (W, H), (24, 24, 24))
    d = ImageDraw.Draw(s)
    d.text((pad, 16), "river_bend_60: tight river bend across adjacent edges (7.42 m arc between the 1 m "
           "river_edge stubs), base + bank dressing, in play", fill=(235, 235, 235), font=fb)
    y = 60
    for cells, h in rows:
        w = (W - pad * (len(cells) + 1)) // len(cells)
        for c, (text, im) in enumerate(cells):
            x = pad + c * (w + pad)
            d.text((x, y + 3), text, fill=(220, 220, 220), font=f)
            s.paste(fit(im, w, h), (x, y + label))
        y += label + h + pad
    s.save(DOCS / "sheet.jpg", quality=85)
    a = Image.open(SHOTS / "rb60-top-a.png").convert("RGB")
    b = Image.open(SHOTS / "rb60-top-b.png").convert("RGB")
    print(f"water moving: top A/B mean |diff| {sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3:.3f} / 255")
    print(f"wrote {DOCS / 'sheet.jpg'}")


if __name__ == "__main__":
    if "--sheet-only" not in sys.argv:
        shoot()
    sheet()
