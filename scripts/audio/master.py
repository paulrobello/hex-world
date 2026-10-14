#!/usr/bin/env python3
"""make audio-master: provider source -> 48 kHz master -> Opus/WebM + AAC/m4a, provenance, manifest.

Usage: python3 scripts/audio/master.py [--root DIR] [--cue ID ...]

Per cue in <root>/assets-source/audio/cues.json (PRD §8, §11.3, §11.4):
  1. Decode the untouched source to 48 kHz float at the cue's channel count (amplitude fold). Music
     sources are git-ignored and read from the local store (audio_lib.Layout.stores: $HEX_MUSIC_STORE or
     this tree's assets-source/audio/music, then the main checkout's), and provenance records them by
     store key, SHA-256 and size. A cue with `sourceCue` masters another cue's source (a picked review
     candidate). A loop's `loopEndSeconds` drops the take's ending (a fade-out or final chord). A per-cue `filter`
     (high-pass and a harmonic notch comb, for a provider hum) runs next, with circular context on
     loops so the wrap stays continuous. A per-cue `cut` takes a faded excerpt as a one-shot, and a
     per-cue `gate` then expands everything under a threshold set relative to the excerpt's loudest
     50 ms window downward, so the noise between and after the calls drops away.
  2. One-shots: trim leading and trailing silence, keeping the attack and decay, never below 0.5 s.
     Loops: a sample-seamless source (ElevenLabs loop mode) is rotated so head and tail sit at the
     same level; any other loop (music) is repaired by folding the tail over the head.
  3. Two-pass EBU R128: measure, then apply the class gain. When the true peak would pass the master
     ceiling, a 4x-oversampled lookahead limiter takes the excess (loops are limited with circular
     context so the wrap stays continuous). A cue needing more than MAX_LIMIT_DB is mastered linearly
     to the ceiling instead and its loudness shortfall is recorded as a deviation for the owner.
  4. Enforce the master true-peak ceiling by a measured linear trim.
  5. Encode Opus in WebM and AAC in m4a (bit-exact, no metadata), measure both, and write
     <cue>/provenance.json, assets/audio/manifest.json and assets-source/audio/review.html.
A music cue with no source yet is listed as pending; an ElevenLabs cue without one is an error.
A `retired` cue is skipped: it stays in cues.json only so its source and generation record can still be
named by `sourceCue`.
"""

from __future__ import annotations

import argparse
import array
import json
import math
import os
import subprocess
import sys
from pathlib import Path

import audio_lib as A
import review

MIN_ONESHOT_S = 0.5
LEAD_DB = -45.0  # leading silence: below the cue's peak by this much
TAIL_DB = -60.0  # trailing silence: below the cue's peak by this much
PRE_ROLL_S = 0.003
FADE_IN_S = 0.001
FADE_OUT_S = 0.015
MAX_LIMIT_DB = 14.0
LIMIT_MARGIN_DB = 0.5  # limiter ceiling sits this far under the master ceiling (sample vs true peak)
ENCODE_MARGIN_DB = 0.2  # encoded true peak must sit this far under the contract ceiling
LOOP_PAD_S = 1.0
DEFAULT_LOOP_CROSSFADE_S = 0.5
SCRIPTS = ("audio_lib.py", "master.py", "review.py")


def db_to_gain(db: float) -> float:
    return 10 ** (db / 20)


def scale(samples: array.array, db: float) -> array.array:
    g = db_to_gain(db)
    return array.array("f", (s * g for s in samples))


def measure(layout: A.Layout, name: str, samples: array.array, channels: int) -> dict:
    path = layout.work / "tmp" / f"{name}.wav"
    A.write_wav(path, samples, channels)
    return A.loudness(path)


def peak_db(samples: array.array) -> float:
    return 20 * math.log10(max(max(map(abs, samples)), 1e-9))


def trim_oneshot(samples: array.array, channels: int) -> tuple[array.array, dict]:
    frames = len(samples) // channels
    env = [max(abs(samples[f * channels + c]) for c in range(channels)) for f in range(frames)]
    peak = max(env)
    lead_t, tail_t = peak * db_to_gain(LEAD_DB), peak * db_to_gain(TAIL_DB)
    first = next(f for f in range(frames) if env[f] >= lead_t)
    last = next(f for f in range(frames - 1, -1, -1) if env[f] >= tail_t)
    start = max(0, first - int(PRE_ROLL_S * A.RATE))
    end = min(frames, last + 1 + int(FADE_OUT_S * A.RATE))
    end = min(frames, max(end, start + int(MIN_ONESHOT_S * A.RATE)))
    out = array.array("f", samples[start * channels:end * channels])
    n = end - start
    fade_in, fade_out = int(FADE_IN_S * A.RATE), int(FADE_OUT_S * A.RATE)
    for f in range(min(fade_in, n)):
        for c in range(channels):
            out[f * channels + c] *= f / fade_in
    for i in range(min(fade_out, n)):
        f = n - 1 - i
        for c in range(channels):
            out[f * channels + c] *= i / fade_out
    return out, {"step": "trim", "leadFramesRemoved": start, "tailFramesRemoved": frames - end,
                 "leadThresholdDb": LEAD_DB, "tailThresholdDb": TAIL_DB, "preRollSeconds": PRE_ROLL_S,
                 "fadeInSeconds": FADE_IN_S, "fadeOutSeconds": FADE_OUT_S, "minSeconds": MIN_ONESHOT_S}


def rotate_loop(samples: array.array, channels: int) -> tuple[array.array, dict]:
    """Move the wrap of a sample-seamless loop to where head and tail levels match.

    The old wrap is continuous by construction, so it becomes an ordinary interior sample step.
    """
    mono = A.mono_fold(samples, channels)
    win = int(A.LOOP_WINDOW_S * A.RATE)
    levels = A.window_db(mono, win)
    n, edge = len(levels), A.LOOP_EDGE_WINDOWS
    best_k, best = 0, None
    for k in range(n):
        head = max(levels[(k + i) % n] for i in range(edge))
        tail = max(levels[(k - 1 - i) % n] for i in range(edge))
        score = abs(head - tail)
        if best is None or score < best - 1e-9:
            best_k, best = k, score
    offset = best_k * win
    i = offset * channels
    out = array.array("f", samples[i:]) + array.array("f", samples[:i])
    return out, {"step": "rotate-loop", "offsetFrames": offset, "headTailLevelDeltaDb": round(best, 2)}


def crossfade_loop(samples: array.array, channels: int, seconds: float) -> tuple[array.array, dict]:
    """Equal-power fold of the tail over the head; the result wraps continuously."""
    total = len(samples) // channels
    fade = int(seconds * A.RATE)
    if fade <= 0 or fade * 2 >= total:
        raise ValueError(f"a {seconds}s loop crossfade does not fit {total} frames")
    kept = total - fade
    out = array.array("f", samples[:kept * channels])
    for f in range(fade):
        angle = (f + 0.5) / fade * (math.pi / 2)
        hg, tg = math.sin(angle), math.cos(angle)
        for c in range(channels):
            out[f * channels + c] = samples[f * channels + c] * hg + samples[(kept + f) * channels + c] * tg
    return out, {"step": "crossfade-loop", "seconds": seconds, "framesRemoved": fade}


def filter_chain(spec: dict) -> str:
    """ffmpeg -af chain for a cues.json `filter`: optional high-pass and low-pass, then a notch at each harmonic.

    {"highpassHz": 150, "lowpassHz": 9500, "stages": 2,
     "harmonicNotch": {"fundamentalHz": 200, "harmonics": 48, "widthHz": 6}}
    `stages` repeats each 2-pole pass (12 dB/octave per stage).
    """
    parts = []
    stages = int(spec.get("stages", 1))
    if spec.get("highpassHz"):
        parts += [f"highpass=f={spec['highpassHz']}:poles=2"] * stages
    if spec.get("lowpassHz"):
        parts += [f"lowpass=f={spec['lowpassHz']}:poles=2"] * stages
    notch = spec.get("harmonicNotch") or []
    done: set[float] = set()
    for series in notch if isinstance(notch, list) else [notch]:
        f0, width = float(series["fundamentalHz"]), float(series["widthHz"])
        for k in range(1, int(series["harmonics"]) + 1):
            if f0 * k < A.RATE / 2 - width and f0 * k not in done:
                done.add(f0 * k)
                parts.append(f"bandreject=f={f0 * k:g}:width_type=h:width={width:g}")
    if not parts:
        raise ValueError(f"filter {spec} names no stage")
    return ",".join(parts)


def apply_filter(layout: A.Layout, name: str, samples: array.array, channels: int, spec: dict,
                 loop: bool) -> tuple[array.array, dict]:
    """Runs the cue's filter. Loops get circular context on both sides, so the IIR start-up transient
    falls in the discarded pad and the wrap stays continuous."""
    chain = filter_chain(spec)
    frames = len(samples) // channels
    pad = min(int(LOOP_PAD_S * A.RATE), frames // 2) if loop else 0
    body = (array.array("f", samples[(frames - pad) * channels:]) + samples
            + array.array("f", samples[:pad * channels])) if pad else samples
    src = layout.work / "tmp" / f"{name}.prefilter.wav"
    dst = layout.work / "tmp" / f"{name}.filtered.wav"
    A.write_wav(src, body, channels)
    out = subprocess.run(["ffmpeg", "-nostdin", "-y", "-v", "error", "-i", str(src), "-af", chain,
                          "-c:a", "pcm_f32le", str(dst)], capture_output=True)
    if out.returncode != 0:
        raise RuntimeError(f"{name}: filter failed: {out.stderr.decode().strip()}")
    filtered, _ = A.decode(dst, channels)
    if len(filtered) != len(body):
        raise RuntimeError(f"{name}: filter changed the length ({len(body) // channels} -> {len(filtered) // channels})")
    return (array.array("f", filtered[pad * channels:(pad + frames) * channels]),
            {"step": "filter", "spec": spec, "chain": chain, "circularPadFrames": pad})


def cut(samples: array.array, channels: int, spec: dict) -> tuple[array.array, dict]:
    """A faded excerpt for a one-shot taken from a longer bed: {startSeconds, seconds, fadeInSeconds, fadeOutSeconds}."""
    frames = len(samples) // channels
    start = int(spec["startSeconds"] * A.RATE)
    n = int(spec["seconds"] * A.RATE)
    if start < 0 or n <= 0 or start + n > frames:
        raise ValueError(f"cut {spec} does not fit {frames / A.RATE:.2f} s of source")
    out = array.array("f", samples[start * channels:(start + n) * channels])
    fin, fout = int(spec["fadeInSeconds"] * A.RATE), int(spec["fadeOutSeconds"] * A.RATE)
    for f in range(min(fin, n)):
        g = math.sin((f + 0.5) / fin * math.pi / 2) ** 2
        for c in range(channels):
            out[f * channels + c] *= g
    for i in range(min(fout, n)):
        g = math.sin((i + 0.5) / fout * math.pi / 2) ** 2
        for c in range(channels):
            out[(n - 1 - i) * channels + c] *= g
    return out, {"step": "cut", **spec, "frames": n}


def gate(samples: array.array, channels: int, spec: dict) -> tuple[array.array, dict]:
    """Downward expander on a one-shot: {thresholdDbBelowPeak, ratio, rangeDb, attackMs, releaseMs}.

    The threshold sits `thresholdDbBelowPeak` under the loudest FLOOR_WINDOW_S RMS window of the input,
    so it follows the take rather than an absolute level. Under it the gain falls by (ratio - 1) dB per dB,
    at most `rangeDb`; the gain opens with `attackMs` and closes with `releaseMs` (one-pole, in dB).
    """
    mono = A.mono_fold(samples, channels)
    frames = len(mono)
    peak_db = max(A.window_db(mono, int(A.FLOOR_WINDOW_S * A.RATE)))
    thr_db = peak_db - float(spec["thresholdDbBelowPeak"])
    ratio, floor_db = float(spec["ratio"]), -float(spec["rangeDb"])
    det = int(0.01 * A.RATE)
    sq = [0.0] * (frames + 1)
    for i, v in enumerate(mono):
        sq[i + 1] = sq[i] + v * v
    up = math.exp(-1 / (float(spec["attackMs"]) / 1000 * A.RATE))
    down = math.exp(-1 / (float(spec["releaseMs"]) / 1000 * A.RATE))
    out = array.array("f", samples)
    g = floor_db
    closed = 0
    for f in range(frames):
        lo, hi = max(0, f - det // 2), min(frames, f + det // 2 + 1)
        rms = math.sqrt(max(sq[hi] - sq[lo], 0.0) / (hi - lo))
        env_db = 20 * math.log10(rms) if rms > 1e-9 else -180.0
        want = 0.0 if env_db >= thr_db else max(floor_db, (env_db - thr_db) * (ratio - 1))
        coef = up if want > g else down
        g = want + (g - want) * coef
        closed += want < 0
        lin = 10 ** (g / 20)
        for c in range(channels):
            out[f * channels + c] *= lin
    return out, {"step": "gate", **spec, "peakWindowDb": round(peak_db, 2), "thresholdDb": round(thr_db, 2),
                 "closedPct": round(100 * closed / max(frames, 1), 1)}


def limit(layout: A.Layout, name: str, samples: array.array, channels: int, gain_db: float,
          ceiling_db: float, loop: bool) -> array.array:
    """Gain then a 4x-oversampled lookahead limiter. Loops get circular context on both sides."""
    frames = len(samples) // channels
    pad = min(int(LOOP_PAD_S * A.RATE), frames // 2) if loop else 0
    body = (array.array("f", samples[(frames - pad) * channels:]) + samples
            + array.array("f", samples[:pad * channels])) if pad else samples
    src = layout.work / "tmp" / f"{name}.prelimit.wav"
    dst = layout.work / "tmp" / f"{name}.limited.wav"
    A.write_wav(src, body, channels)
    chain = (f"volume={gain_db:.4f}dB,aresample={A.RATE * 4},"
             f"alimiter=limit={db_to_gain(ceiling_db - LIMIT_MARGIN_DB):.6f}:attack=1:release=50:level=false:latency=true,"
             f"aresample={A.RATE}")
    out = subprocess.run(["ffmpeg", "-nostdin", "-y", "-v", "error", "-i", str(src), "-af", chain,
                          "-c:a", "pcm_f32le", str(dst)], capture_output=True)
    if out.returncode != 0:
        raise RuntimeError(f"{name}: limiter failed: {out.stderr.decode().strip()}")
    limited, _ = A.decode(dst, channels)
    if len(limited) // channels != len(body) // channels:
        raise RuntimeError(f"{name}: limiter changed the length ({len(body) // channels} -> {len(limited) // channels})")
    return array.array("f", limited[pad * channels:(pad + frames) * channels])


def level(layout: A.Layout, cue: dict, samples: array.array, channels: int, target: float,
          ceiling: float) -> tuple[array.array, dict]:
    name = cue["id"]
    pass1 = measure(layout, f"{name}.pass1", samples, channels)
    gain = target - pass1["integrated"]
    excess = pass1["truePeak"] + gain - ceiling
    record = {"step": "loudness", "pass1": pass1, "targetLufs": target, "gainDb": round(gain, 2),
              "requiredPeakReductionDb": round(max(excess, 0.0), 2)}
    if excess <= 0:
        record["mode"] = "linear"
        return scale(samples, gain), record
    if excess > MAX_LIMIT_DB:
        record["mode"] = "linear-to-ceiling"
        record["deviation"] = (f"reaching {target} LUFS needs {excess:.1f} dB of peak reduction, over the "
                               f"{MAX_LIMIT_DB} dB limiter allowance; mastered linearly to the ceiling instead")
        return scale(samples, ceiling - pass1["truePeak"]), record
    mono = A.mono_fold(samples, channels)
    win = int(0.02 * A.RATE)
    peaks = [max(map(abs, mono[i:i + win])) for i in range(0, len(mono) - win + 1, win)] or [max(map(abs, mono))]
    over = sum(1 for p in peaks if 20 * math.log10(max(p, 1e-9)) + gain > ceiling - LIMIT_MARGIN_DB)
    g = gain
    for attempt in range(4):
        out = limit(layout, name, samples, channels, g, ceiling, cue["loop"])
        got = measure(layout, f"{name}.pass2", out, channels)
        if abs(target - got["integrated"]) <= 0.2:
            break
        g += target - got["integrated"]
    record.update(mode="limited", limiterInputGainDb=round(g, 2), limiterPasses=attempt + 1,
                  limiterCeilingDb=ceiling - LIMIT_MARGIN_DB,
                  windowsOverCeilingPct=round(100 * over / len(peaks), 1))
    return out, record


def settle(layout: A.Layout, name: str, samples: array.array, channels: int, target: float,
           ceiling: float) -> tuple[array.array, dict]:
    """Final linear trim toward the target, never past the master true-peak ceiling (skill: loudnorm trap)."""
    m = measure(layout, f"{name}.settle", samples, channels)
    trim = min(target - m["integrated"], ceiling - m["truePeak"])
    if abs(trim) < 0.05:
        return samples, {"step": "settle", "trimDb": 0.0}
    return scale(samples, trim), {"step": "settle", "trimDb": round(trim, 2)}


def encode(master: Path, out: Path, ext: str, channels: int, bitrate: str) -> list[str]:
    out.parent.mkdir(parents=True, exist_ok=True)
    codec = ["-c:a", "libopus", "-b:a", bitrate, "-vbr", "on", "-compression_level", "10",
             "-application", "audio", "-frame_duration", "20"] if ext == "webm" else \
            ["-c:a", "aac", "-b:a", bitrate, "-movflags", "+faststart"]
    args = ["ffmpeg", "-nostdin", "-y", "-v", "error", "-i", str(master), "-map_metadata", "-1",
            "-fflags", "+bitexact", "-flags:a", "+bitexact", "-ac", str(channels), "-ar", str(A.RATE),
            *codec, str(out)]
    res = subprocess.run(args, capture_output=True)
    if res.returncode != 0 or res.stderr.strip():
        raise RuntimeError(f"{out}: encode failed: {res.stderr.decode().strip()}")
    return args


def source_dir(layout: A.Layout, reg: dict, cue: dict) -> tuple[Path, str]:
    """Where a cue's source and generation record live: its own folder, or its `sourceCue`'s."""
    sid = cue.get("sourceCue") or cue["id"]
    other = next((c for c in reg["cues"] if c["id"] == sid), None)
    if other is None:
        raise ValueError(f"{cue['id']}: sourceCue {sid} is not in cues.json")
    return layout.cue_dir(other), sid


def find_source(layout: A.Layout, reg: dict, cue: dict) -> Path | None:
    folder, sid = source_dir(layout, reg, cue)
    if folder.parent.name in A.STORE_KINDS:
        return layout.find_stored_source(sid)
    for ext in A.SOURCE_EXTS:
        p = folder / f"{sid}{ext}"
        if p.exists():
            return p
    return None


def source_record(layout: A.Layout, reg: dict, cue: dict, source: Path) -> dict:
    """A committed source by repo path; a store-kept (git-ignored) one by store, key, hash and size."""
    folder, sid = source_dir(layout, reg, cue)
    if folder.parent.name in A.STORE_KINDS:
        return A.store_record(source, sid)
    return A.file_record(layout, source)


def trim_loop_end(samples: array.array, channels: int, seconds: float) -> tuple[array.array, dict]:
    """Drops a take's ending (a fade-out or a final chord), so the crossfade folds groove over groove."""
    frames = len(samples) // channels
    end = int(round(seconds * A.RATE))
    if end <= 0 or end > frames:
        raise ValueError(f"loopEndSeconds {seconds} does not fit {frames / A.RATE:.2f} s of source")
    return (array.array("f", samples[:end * channels]),
            {"step": "trim-loop-end", "loopEndSeconds": seconds, "framesRemoved": frames - end})


REQUIRED_GENERATION = ("provider", "model", "subscription", "generationId")


def generation_record(layout: A.Layout, reg: dict, cue: dict) -> dict:
    path = source_dir(layout, reg, cue)[0] / "generation.json"
    if not path.exists():
        raise ValueError(f"{cue['id']}: {layout.rel(path)} missing (template: audio/music/generation.template.json)")
    gen = json.loads(path.read_text())
    missing = [k for k in REQUIRED_GENERATION if not gen.get(k)]
    if not (gen.get("prompt") or cue.get("promptFile")):
        missing.append("prompt")
    if missing:
        raise ValueError(f"{cue['id']}: {layout.rel(path)} lacks {', '.join(missing)}")
    unfilled = A.placeholders({k: v for k, v in gen.items() if k != "responseHeaders"})
    if unfilled:
        raise ValueError(f"{cue['id']}: {layout.rel(path)} still has template placeholders in {', '.join(unfilled)}")
    return gen


def license_statement(gen: dict) -> str:
    return (f"Generated with the owner's paid {gen['subscription'].split(' (')[0]} subscription. Per PRD D21 the "
            "owner holds the rights to this audio, so it is committed to the public Hex World repository and "
            "served from hex-world.pardev.net. Rights basis: the owner's D21 attestation; the provider's "
            "current terms were not re-verified by the mastering script.")


def master_cue(layout: A.Layout, reg: dict, cue: dict, tools: dict, script_hashes: dict) -> dict:
    cls = reg["classes"][cue["class"]]
    target, ceiling = float(cls["lufs"]), float(reg["masterTruePeak"])
    channels = cue["channels"]
    source = find_source(layout, reg, cue)
    if source is None:
        if cue["provider"] == "suno":
            return {"id": cue["id"], "kind": cue["kind"], "class": cue["class"], "status": "pending-source",
                    "promptFile": cue.get("promptFile"), "events": cue["events"]}
        raise ValueError(f"{cue['id']}: no source in {layout.rel(source_dir(layout, reg, cue)[0])}")
    gen = generation_record(layout, reg, cue)
    src_info = A.probe(source)
    samples, _ = A.decode(source, channels)
    before = measure(layout, f"{cue['id']}.source", samples, channels)
    steps: list[dict] = [{"step": "decode", "to": f"{A.RATE} Hz float, {channels} ch",
                          "fold": A.downmix(src_info["channels"], channels) if src_info["channels"] != channels else None}]
    if cue.get("sourceCue"):
        steps[0]["sourceCue"] = cue["sourceCue"]
    if cue.get("loopEndSeconds"):
        if not cue["loop"]:
            raise ValueError(f"{cue['id']}: loopEndSeconds is for loop cues")
        samples, info = trim_loop_end(samples, channels, float(cue["loopEndSeconds"]))
        steps.append(info)
    if cue.get("filter"):
        samples, info = apply_filter(layout, cue["id"], samples, channels, cue["filter"], cue["loop"])
        steps.append(info)

    loop = None
    if cue["loop"]:
        mono = A.mono_fold(samples, channels)
        report = A.loop_report(mono, 0, len(mono))
        seamless = not any(p.startswith("wrap step") for p in report["problems"])
        steps.append({"step": "source-loop-check", **{k: v for k, v in report.items() if k != "problems"},
                      "seamless": seamless})
        if seamless and not cue.get("loopCrossfadeSeconds"):
            samples, info = rotate_loop(samples, channels)
            steps.append(info)
            samples, info = level(layout, cue, samples, channels, target, ceiling)
            steps.append(info)
        else:
            samples, info = level(layout, cue, samples, channels, target, ceiling)
            steps.append(info)
            samples, info = crossfade_loop(samples, channels, cue.get("loopCrossfadeSeconds") or DEFAULT_LOOP_CROSSFADE_S)
            steps.append(info)
    else:
        samples, info = cut(samples, channels, cue["cut"]) if cue.get("cut") else trim_oneshot(samples, channels)
        steps.append(info)
        if cue.get("gate"):
            samples, info = gate(samples, channels, cue["gate"])
            steps.append(info)
        samples, info = level(layout, cue, samples, channels, target, ceiling)
        steps.append(info)
    samples, info = settle(layout, cue["id"], samples, channels, target, ceiling)
    steps.append(info)

    frames = len(samples) // channels
    master = layout.work / cue["kind"] / f"{cue['id']}.wav"
    if cue["loop"]:
        loop = {"startSample": 0, "endSample": frames, "sampleRate": A.RATE,
                "startSeconds": 0.0, "endSeconds": round(frames / A.RATE, 6)}

    # Lossy encoding can overshoot the master peak by more than the 1 dB margin (Opus on a low
    # rumble measured -0.5 dBTP from a -2.0 master), so the encoded files are measured and the
    # master trimmed linearly by the overshoot until both encodes hold the contract ceiling.
    contract = float(reg["truePeakCeiling"])
    for _ in range(4):
        A.write_wav(master, samples, channels)
        runtime = []
        for ext in ("webm", "m4a"):
            out = layout.runtime_file(cue, ext)
            bitrate = (cue.get("bitrates", {}).get(ext) or cls.get("bitrates", {}).get(ext, {}).get(str(channels))
                       or reg["bitrates"][ext][str(channels)])
            args = encode(master, out, ext, channels, bitrate)
            info = A.probe(out)
            decoded, _ = A.decode(out, channels)
            runtime.append({**A.file_record(layout, out), "codec": info["codec"], "bitrate": bitrate,
                            "channels": info["channels"], "sampleRate": A.RATE, "durationSeconds": info["durationSeconds"],
                            "decodedFrames": len(decoded) // channels, "loudness": A.loudness(out),
                            **({"floor": A.floor_report(out, (cue.get("cut") or {}).get("fadeInSeconds", 0.0),
                                                        (cue.get("cut") or {}).get("fadeOutSeconds", 0.0))}
                               if cue.get("pool") else {}),
                            "command": " ".join(a if " " not in a else f"'{a}'" for a in args).replace(str(layout.root) + "/", "")})
        worst = max(r["loudness"]["truePeak"] for r in runtime)
        over = worst - (contract - ENCODE_MARGIN_DB)
        if over <= 0:
            break
        samples = scale(samples, -over)
        steps.append({"step": "encode-overshoot-trim", "encodedTruePeakDb": worst, "trimDb": round(-over, 2)})
    master_lufs = A.loudness(master)
    for r in runtime:
        steps.append({"step": f"encode-{r['path'].rsplit('.', 1)[1]}", "bitrate": r["bitrate"]})

    deviation = next((s.get("deviation") for s in steps if s.get("deviation")), None)
    prov = {
        "cueId": cue["id"], "kind": cue["kind"], "class": cue["class"], "version": 1,
        "use": cue["use"], "events": cue["events"], "loop": loop,
        "playback": cue.get("playback") or cls["playback"], "channels": channels,
        "provider": {"name": gen["provider"], "endpoint": gen.get("endpoint"), "model": gen["model"],
                     "subscription": gen["subscription"], "outputFormat": gen.get("outputFormat")},
        "prompt": gen.get("prompt"),
        "promptFile": ({"path": cue["promptFile"], "sha256": A.sha256(layout.root / cue["promptFile"])}
                       if cue.get("promptFile") and (layout.root / cue["promptFile"]).exists() else None),
        "parameters": gen.get("parameters"),
        "generation": {"id": gen["generationId"], "generatedAt": gen.get("generatedAt"), "attempt": gen.get("attempt"),
                       "creditsCharged": gen.get("creditsCharged"),
                       "record": A.file_record(layout, source_dir(layout, reg, cue)[0] / "generation.json")},
        "source": {**source_record(layout, reg, cue, source),
                   **{k: src_info[k] for k in ("codec", "sampleRate", "channels", "durationSeconds")}},
        "loudness": {"targetLufs": target, "toleranceLu": reg["lufsTolerance"], "truePeakCeilingDb": reg["truePeakCeiling"],
                     "masterTruePeakDb": ceiling, "before": before, "master": master_lufs,
                     "after": {r["path"].rsplit(".", 1)[1]: r["loudness"] for r in runtime}},
        "processing": steps,
        "master": {"path": layout.rel(master), "sha256": A.sha256(master), "frames": frames, "sampleRate": A.RATE,
                   "format": "WAV float32 (target/, not committed; rebuilt by make audio-master)"},
        "runtime": runtime,
        "decodedBytes": frames * channels * 4,
        "toolVersions": {**tools, "scripts": script_hashes},
        "license": license_statement(gen),
        "deviation": deviation,
        "approvals": {"technical": "make audio-check", "creative": "pending owner audition (review.html)",
                      "rights": "owner attestation, PRD D21", "owner": "pending"},
    }
    prov_path = layout.cue_dir(cue) / "provenance.json"
    prov_path.parent.mkdir(parents=True, exist_ok=True)
    prov_path.write_text(json.dumps(prov, indent=2) + "\n")
    lim = next((s for s in steps if s["step"] == "loudness"), {})
    mode = lim.get("mode", "")
    if mode == "limited":
        mode += f" {lim['requiredPeakReductionDb']} dB"
    webm = runtime[0]["loudness"]
    print(f"mastered {cue['id']:16} {cue['class']:5} {frames / A.RATE:6.2f}s  "
          f"{before['integrated']:6.1f} -> {webm['integrated']:6.1f} LUFS  TP {webm['truePeak']:5.1f} dBTP  {mode}"
          f"{'  DEVIATION' if deviation else ''}")
    return {"id": cue["id"], "kind": cue["kind"], "class": cue["class"],
            "status": "candidate" if cue.get("candidateFor") else "mastered",
            **({"candidateFor": cue["candidateFor"]} if cue.get("candidateFor") else {}),
            **({"pool": cue["pool"]} if cue.get("pool") else {}),
            "events": cue["events"], "playback": cue.get("playback") or cls["playback"], "channels": channels, "loop": loop,
            "durationSeconds": round(frames / A.RATE, 6), "decodedBytes": frames * channels * 4,
            "targetLufs": target,
            "files": {r["path"].rsplit(".", 1)[1]: {"path": os.path.relpath(layout.root / r["path"], layout.runtime),
                                                     "sha256": r["sha256"],
                                                     "bytes": r["bytes"]} for r in runtime},
            "provenance": layout.rel(prov_path)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--root", type=Path, default=A.REPO)
    parser.add_argument("--cue", action="append", default=None, help="master only these cues")
    args = parser.parse_args()
    layout = A.Layout(args.root.resolve())
    reg = layout.registry()
    tools = A.tool_versions()
    here = Path(__file__).resolve().parent
    script_hashes = {f"scripts/audio/{n}": A.sha256(here / n) for n in SCRIPTS}
    manifest_path = layout.runtime / "manifest.json"
    prev = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    old = {c["id"]: c for c in prev.get("cues", []) + prev.get("candidates", [])}
    entries, errors = [], []
    for cue in reg["cues"]:
        if cue.get("retired"):
            continue
        if args.cue and cue["id"] not in args.cue:
            if cue["id"] in old:
                entries.append(old[cue["id"]])
            continue
        try:
            entries.append(master_cue(layout, reg, cue, tools, script_hashes))
        except (ValueError, RuntimeError) as err:
            errors.append(str(err))
    manifest = {
        "version": 1,
        "note": "Generated by make audio-master from assets-source/audio/cues.json. Paths are relative to assets/audio/.",
        "sampleRate": A.RATE,
        "decodedBudgetBytes": reg["decodedBudgetBytes"],
        "formats": ["webm", "m4a"],
        "cues": [e for e in entries if e["status"] != "candidate"],
        "candidates": [e for e in entries if e["status"] == "candidate"],
    }
    layout.runtime.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    review.write(layout, reg)
    pending = [e["id"] for e in entries if e["status"] == "pending-source"]
    if pending:
        print(f"pending (no source yet): {', '.join(pending)}")
    if errors:
        print(f"FAIL ({len(errors)}):")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(f"wrote {layout.rel(manifest_path)} and {layout.rel(layout.source / 'review.html')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
