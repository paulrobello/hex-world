#!/usr/bin/env python3
"""make audio-master: provider source -> 48 kHz master -> Opus/WebM + AAC/m4a, provenance, manifest.

Usage: python3 scripts/audio/master.py [--root DIR] [--cue ID ...]

Per cue in <root>/assets-source/audio/cues.json (PRD §8, §11.3, §11.4):
  1. Decode the untouched source to 48 kHz float at the cue's channel count (amplitude fold).
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
"""

from __future__ import annotations

import argparse
import array
import json
import math
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
SOURCE_EXTS = (".wav", ".flac", ".aiff", ".aif", ".mp3", ".m4a", ".ogg")
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


def find_source(layout: A.Layout, cue: dict) -> Path | None:
    for ext in SOURCE_EXTS:
        p = layout.cue_dir(cue) / f"{cue['id']}{ext}"
        if p.exists():
            return p
    return None


REQUIRED_GENERATION = ("provider", "model", "subscription", "generationId")


def generation_record(layout: A.Layout, cue: dict) -> dict:
    path = layout.cue_dir(cue) / "generation.json"
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
    source = find_source(layout, cue)
    if source is None:
        if cue["provider"] == "suno":
            return {"id": cue["id"], "kind": cue["kind"], "class": cue["class"], "status": "pending-source",
                    "promptFile": cue.get("promptFile"), "events": cue["events"]}
        raise ValueError(f"{cue['id']}: no source in {layout.rel(layout.cue_dir(cue))}")
    gen = generation_record(layout, cue)
    src_info = A.probe(source)
    samples, _ = A.decode(source, channels)
    before = measure(layout, f"{cue['id']}.source", samples, channels)
    steps: list[dict] = [{"step": "decode", "to": f"{A.RATE} Hz float, {channels} ch",
                          "fold": A.downmix(src_info["channels"], channels) if src_info["channels"] != channels else None}]

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
        samples, info = trim_oneshot(samples, channels)
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
            bitrate = cls.get("bitrates", {}).get(ext, {}).get(str(channels)) or reg["bitrates"][ext][str(channels)]
            args = encode(master, out, ext, channels, bitrate)
            info = A.probe(out)
            decoded, _ = A.decode(out, channels)
            runtime.append({**A.file_record(layout, out), "codec": info["codec"], "bitrate": bitrate,
                            "channels": info["channels"], "sampleRate": A.RATE, "durationSeconds": info["durationSeconds"],
                            "decodedFrames": len(decoded) // channels, "loudness": A.loudness(out),
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
                       "record": A.file_record(layout, layout.cue_dir(cue) / "generation.json")},
        "source": {**A.file_record(layout, source), **{k: src_info[k] for k in ("codec", "sampleRate", "channels", "durationSeconds")}},
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
    prov_path.write_text(json.dumps(prov, indent=2) + "\n")
    lim = next((s for s in steps if s["step"] == "loudness"), {})
    mode = lim.get("mode", "")
    if mode == "limited":
        mode += f" {lim['requiredPeakReductionDb']} dB"
    webm = runtime[0]["loudness"]
    print(f"mastered {cue['id']:16} {cue['class']:5} {frames / A.RATE:6.2f}s  "
          f"{before['integrated']:6.1f} -> {webm['integrated']:6.1f} LUFS  TP {webm['truePeak']:5.1f} dBTP  {mode}"
          f"{'  DEVIATION' if deviation else ''}")
    return {"id": cue["id"], "kind": cue["kind"], "class": cue["class"], "status": "mastered",
            "events": cue["events"], "playback": cue.get("playback") or cls["playback"], "channels": channels, "loop": loop,
            "durationSeconds": round(frames / A.RATE, 6), "decodedBytes": frames * channels * 4,
            "targetLufs": target,
            "files": {r["path"].rsplit(".", 1)[1]: {"path": r["path"].removeprefix("assets/audio/"), "sha256": r["sha256"],
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
    old = {c["id"]: c for c in json.loads(manifest_path.read_text())["cues"]} if manifest_path.exists() else {}
    entries, errors = [], []
    for cue in reg["cues"]:
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
        "cues": entries,
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
