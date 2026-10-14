#!/usr/bin/env python3
"""Screens an ambience take before it is mastered: band energy, level steadiness, tonal peaks, birds.

Usage: python3 scripts/audio/screen.py FILE [FILE ...] [--json]
       python3 scripts/audio/screen.py --selftest

Needs numpy (not part of the stdlib-only mastering tools, so it is not in any make gate). Each file is
decoded to 48 kHz and folded to mono by amplitude, then measured four ways:
  band       share of power below 300 Hz, 300 Hz-8 kHz and above 8 kHz (Welch, 8192-point Hann), and
             the 1-6 kHz band against the full band (what laptop speakers reproduce).
  level      RMS of 1 s windows on a 0.25 s hop, wrapping round the end as a loop does: the spread
             (max - min) and the deepest dip under the median.
  tones      peaks over a median-filtered floor (+-150 Hz) in the mean spectrum of the quietest quarter
             of 8192-point frames, where a stationary hum stands clearest. Reports each 200 Hz multiple
             to 2 kHz (the series ElevenLabs loop-mode takes carried) and the strongest other peaks.
  birds      tonal bursts in 2-6 kHz: each 1024-point frame is whitened by the per-bin median over the
             take, so stationary hum and noise sit near 0 dB, and a frame whose strongest bin stands
             BIRD_CREST_DB over the frame's median bin is a chirp frame. Frames 100 ms apart or less
             join one event, and three or more events in a take read as birds. Calibrated 2026-10-14 on
             the round-one meadow-bed sources, whose prompts asked for birds (a 9, b 13, c 11 events),
             against surf and white noise (0 each). Tonal water bubbles (river-babble) also trip it.
  floor      one-shots only (--floor): the floor criterion shared with make audio-check
             (audio_lib.floor_report): the quietest interior 50 ms windows against the loudest, and the
             floor's band split under 1.5 kHz, 1.5-9 kHz and over 9 kHz. A file named after a cues.json cue
             with a `cut` leaves that cut's fades out of the interior.
--selftest runs the measures on synthetic controls with known answers and exits non-zero on a miss.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

import audio_lib as A

RATE = 48000
BIRD_BAND = (2000.0, 6000.0)
BIRD_CREST_DB = 18.0
TONE_DB = 10.0


def decode(path: str) -> np.ndarray:
    probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries",
                            "stream=channels", "-of", "csv=p=0", path], capture_output=True, check=True)
    ch = int(probe.stdout.decode().strip())
    raw = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", path, "-ar", str(RATE), "-f", "f32le",
                          "-acodec", "pcm_f32le", "-"], capture_output=True, check=True).stdout
    x = np.frombuffer(raw, dtype=np.float32).astype(np.float64)
    return x.reshape(-1, ch).mean(axis=1) if ch > 1 else x


def frames(x: np.ndarray, n: int, hop: int) -> np.ndarray:
    count = 1 + (len(x) - n) // hop
    idx = np.arange(n)[None, :] + hop * np.arange(count)[:, None]
    return x[idx] * np.hanning(n)[None, :]


def band(x: np.ndarray) -> dict:
    spec = np.abs(np.fft.rfft(frames(x, 8192, 4096), axis=1)) ** 2
    psd = spec.mean(axis=0)
    f = np.fft.rfftfreq(8192, 1 / RATE)
    total = psd[f >= 20].sum()
    low, mid, high = psd[(f >= 20) & (f < 300)].sum(), psd[(f >= 300) & (f < 8000)].sum(), psd[f >= 8000].sum()
    return {"below300Pct": round(100 * low / total, 1), "mid300to8kPct": round(100 * mid / total, 1),
            "above8kPct": round(100 * high / total, 1),
            "midVsFullDb": round(10 * np.log10(mid / total), 1),
            "speech1to6kVsFullDb": round(10 * np.log10(psd[(f >= 1000) & (f < 6000)].sum() / total), 1)}


def level(x: np.ndarray) -> dict:
    win, hop = RATE, RATE // 4
    wrapped = np.concatenate([x, x[:win]])
    starts = np.arange(0, len(x), hop)
    db = np.array([10 * np.log10(np.mean(wrapped[s:s + win] ** 2) + 1e-20) for s in starts])
    med = float(np.median(db))
    return {"spreadDb": round(float(db.max() - db.min()), 1), "deepestDipDb": round(med - float(db.min()), 1),
            "dipAtSeconds": round(float(starts[int(db.argmin())] / RATE), 2)}


def tones(x: np.ndarray) -> dict:
    fr = frames(x, 8192, 4096)
    rms = np.sqrt((fr ** 2).mean(axis=1))
    quiet = fr[rms <= np.quantile(rms, 0.25)]
    psd = (np.abs(np.fft.rfft(quiet, axis=1)) ** 2).mean(axis=0)
    f = np.fft.rfftfreq(8192, 1 / RATE)
    half = int(150 / (RATE / 8192))
    pad = np.pad(psd, half, mode="edge")
    floor = np.array([np.median(pad[i:i + 2 * half + 1]) for i in range(len(psd))])
    prom = 10 * np.log10(psd / floor)
    series = {}
    for k in range(1, 11):
        i = int(round(200 * k / (RATE / 8192)))
        series[str(200 * k)] = round(float(prom[i - 2:i + 3].max()), 1)
    band_ok = (f >= 100) & (f <= 12000)
    local = (prom > np.roll(prom, 1)) & (prom >= np.roll(prom, -1)) & band_ok & (prom > TONE_DB)
    near200 = np.abs(f - 200 * np.round(f / 200)) <= 2 * RATE / 8192
    others = sorted(((float(prom[i]), float(f[i])) for i in np.nonzero(local & ~near200)[0]), reverse=True)[:5]
    return {"series200HzDb": series, "max200HzSeriesDb": max(series.values()),
            "otherPeaks": [{"hz": round(hz), "db": round(db, 1)} for db, hz in others]}


def birds(x: np.ndarray) -> dict:
    n, hop = 1024, 512
    spec = np.abs(np.fft.rfft(frames(x, n, hop), axis=1)) ** 2
    f = np.fft.rfftfreq(n, 1 / RATE)
    sel = (f >= BIRD_BAND[0]) & (f <= BIRD_BAND[1])
    b = spec[:, sel]
    white = b / (np.median(b, axis=0)[None, :] + 1e-30)
    crest = 10 * np.log10(white.max(axis=1) / (np.median(white, axis=1) + 1e-30))
    hits = np.nonzero(crest > BIRD_CREST_DB)[0]
    gap = int(0.1 * RATE / hop)
    events = 0 if len(hits) == 0 else 1 + int(np.sum(np.diff(hits) > gap))
    minutes = len(x) / RATE / 60
    return {"chirpFrames": int(len(hits)), "events": events, "eventsPerMin": round(events / minutes, 1),
            "maxCrestDb": round(float(crest.max()), 1), "p99CrestDb": round(float(np.quantile(crest, 0.99)), 1),
            "birds": events >= 3}


def floor(path: str) -> dict:
    """The make audio-check floor criterion on one file; a cue's `cut` fades are left out of the interior."""
    stem = Path(path).stem
    cue = next((c for c in A.Layout(A.REPO).registry()["cues"] if c["id"] == stem), None)
    cut = (cue or {}).get("cut") or {}
    return A.floor_report(Path(path), cut.get("fadeInSeconds", 0.0), cut.get("fadeOutSeconds", 0.0))


def screen(x: np.ndarray) -> dict:
    return {"band": band(x), "level": level(x), "tones": tones(x), "birds": birds(x)}


def selftest() -> int:
    rng = np.random.default_rng(7)
    t = np.arange(30 * RATE) / RATE
    noise = rng.standard_normal(len(t)) * 0.05
    hum = noise + sum(0.02 * np.sin(2 * np.pi * 200 * k * t) for k in range(1, 6))
    chirps = noise.copy()
    for start in np.arange(1.0, 29.0, 2.0):
        s = int(start * RATE)
        tt = np.arange(int(0.12 * RATE)) / RATE
        chirps[s:s + len(tt)] += 0.1 * np.sin(2 * np.pi * (3000 * tt + 8000 * tt ** 2)) * np.hanning(len(tt))
    dipped = noise.copy()
    dipped[10 * RATE:13 * RATE] *= 10 ** (-10 / 20)
    rumble = np.convolve(rng.standard_normal(len(t)), np.ones(400) / 400, mode="same") * 0.5 + noise * 0.05
    checks = {
        "noise: no tone": screen(noise)["tones"]["max200HzSeriesDb"] < 6,
        "noise: no birds": not birds(noise)["birds"],
        "noise: steady": level(noise)["deepestDipDb"] < 1,
        "hum: 200 Hz series found": tones(hum)["max200HzSeriesDb"] > 15,
        "chirps: birds found": birds(chirps)["birds"] and birds(chirps)["events"] >= 12,
        "dip: 10 dB dip found": 8 < level(dipped)["deepestDipDb"] < 11,
        "rumble: below-300 dominates": band(rumble)["below300Pct"] > 80,
        "noise: flat band share": 25 < band(noise)["mid300to8kPct"] < 40,
    }
    for name, ok in checks.items():
        print(f"{'ok  ' if ok else 'FAIL'} {name}")
    return 0 if all(checks.values()) else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("files", nargs="*")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--floor", action="store_true", help="also measure the one-shot floor criterion")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    out = {}
    for path in args.files:
        out[path] = screen(decode(path))
        if args.floor:
            out[path]["floor"] = floor(path)
        if not args.json:
            r = out[path]
            b, lv, tn, bd = r["band"], r["level"], r["tones"], r["birds"]
            print(f"{path}\n  band  <300 {b['below300Pct']}%  300-8k {b['mid300to8kPct']}%  >8k {b['above8kPct']}%"
                  f"  (300-8k vs full {b['midVsFullDb']} dB)\n  level spread {lv['spreadDb']} dB, deepest dip "
                  f"{lv['deepestDipDb']} dB at {lv['dipAtSeconds']} s\n  tones 200 Hz series {tn['series200HzDb']}"
                  f"\n        others {tn['otherPeaks']}\n  birds {bd}")
            if args.floor:
                fl = r["floor"]
                print(f"  floor {fl['floorBelowPeakDb']} dB under the peak window, bands {fl['floorBandPct']}"
                      f"  {'; '.join(fl['problems']) or 'pass'}")
    if args.json:
        print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
