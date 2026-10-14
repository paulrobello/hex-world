#!/usr/bin/env python3
"""make audio-check-selftest: prove audio-master and audio-check on known synthetic controls.

Builds a tree in target/audio-selftest with three synthetic cues authored sample by sample at known
amplitudes (a one-shot with silence to trim, a loop that is seamless by construction, a stereo music
cue that takes the crossfade path), masters it and requires audio-check to pass. Then it copies the
passing tree three times and breaks one thing in each, requiring audio-check to exit non-zero with
the matching message (the bed also carries a cues.json `filter` notch at its 247.5 Hz tone, which must
drop by 20 dB or more against the 110 Hz tone while the loop still passes, and a `cut` cue must master
to exactly its excerpt length):
  hot        the one-shot's WebM re-encoded 9 dB hotter (loudness, true peak, hash);
  seam       the loop's WebM with a 0.3 step over its last 5 ms (wrap step);
  phase      the music's WebM with the right channel inverted (mono fold).
"""

from __future__ import annotations

import array
import json
import math
import random
import shutil
import subprocess
import sys
from pathlib import Path

import audio_lib as A

HERE = Path(__file__).resolve().parent
ROOT = A.REPO / "target/audio-selftest"
TAU = 2 * math.pi


def hit() -> array.array:
    rng = random.Random(7)
    n, lead, body = int(1.2 * A.RATE), int(0.1 * A.RATE), int(0.8 * A.RATE)
    out = array.array("f", [0.0] * n)
    for i in range(body):
        t = i / A.RATE
        env = math.exp(-t * 9)
        out[lead + i] = env * (0.35 * math.sin(TAU * 220 * t) + 0.15 * (rng.random() * 2 - 1))
    return out


def bed() -> array.array:
    n = 6 * A.RATE  # every frequency below has a whole number of cycles in 6 s, so the wrap is seamless
    return array.array("f", ((0.12 * math.sin(TAU * 110 * i / A.RATE) + 0.08 * math.sin(TAU * 165 * i / A.RATE)
                              + 0.05 * math.sin(TAU * 247.5 * i / A.RATE)) * (0.75 + 0.25 * math.sin(TAU * 0.5 * i / A.RATE))
                             for i in range(n)))


def music() -> array.array:
    n = 12 * A.RATE
    out = array.array("f", [0.0] * (2 * n))
    for i in range(n):
        t = i / A.RATE
        fade = min(1.0, t / 0.3)
        chord = sum(math.sin(TAU * f * t + p) for f, p in ((196.0, 0), (246.94, 1), (293.66, 2))) * 0.08 * fade
        swell = 1 + 0.2 * math.sin(TAU * 0.25 * t)
        out[2 * i] = chord * swell
        out[2 * i + 1] = chord * swell * 0.9 + 0.01 * math.sin(TAU * 392 * t)
    return out


def build() -> None:
    if ROOT.exists():
        shutil.rmtree(ROOT)
    layout = A.Layout(ROOT)
    repo = A.Layout(A.REPO).registry()
    cues = [
        {"id": "synth-hit", "kind": "sfx", "class": "sfx", "channels": 1, "loop": False, "use": "self-test",
         "events": ["tilePlaced"], "provider": "synthetic"},
        {"id": "synth-bed", "kind": "ambience", "class": "loop", "channels": 1, "loop": True, "use": "self-test",
         "events": [], "provider": "synthetic",
         "filter": {"harmonicNotch": {"fundamentalHz": 247.5, "harmonics": 1, "widthHz": 6}}},
        {"id": "synth-music", "kind": "music", "class": "music", "channels": 2, "loop": True,
         "loopCrossfadeSeconds": 1.0, "use": "self-test", "events": [], "provider": "suno",
         "promptFile": "audio/music/prompts/synth-music.md"},
        {"id": "synth-cut", "kind": "ambience", "class": "sfx", "channels": 1, "loop": False, "use": "self-test",
         "events": [], "provider": "synthetic",
         "cut": {"startSeconds": 1.0, "seconds": 3.0, "fadeInSeconds": 0.5, "fadeOutSeconds": 0.5}},
    ]
    reg = {**{k: v for k, v in repo.items() if k != "cues"}, "cues": cues}
    layout.source.mkdir(parents=True)
    layout.registry_path.write_text(json.dumps(reg, indent=2) + "\n")
    prompt = ROOT / "audio/music/prompts/synth-music.md"
    prompt.parent.mkdir(parents=True)
    prompt.write_text("# synth-music\n\nSelf-test fixture: a synthetic chord, not a Suno prompt.\n")
    for cue, samples, ch in ((cues[0], hit(), 1), (cues[1], bed(), 1), (cues[2], music(), 2), (cues[3], bed(), 1)):
        d = layout.cue_dir(cue)
        A.write_wav(d / f"{cue['id']}.wav", samples, ch, fmt="s16")
        (d / "generation.json").write_text(json.dumps({
            "cueId": cue["id"], "provider": "synthetic (audio-check self-test)", "model": "scripts/audio/selftest.py",
            "subscription": "none (self-test fixture)", "generationId": f"selftest-{cue['id']}",
            "prompt": "sample-by-sample synthetic control signal"}, indent=2) + "\n")


def tone_db(samples: array.array, hz: float) -> float:
    """Level of one frequency over the whole signal (single-bin DFT), dB."""
    re = im = 0.0
    w = TAU * hz / A.RATE
    for i, s in enumerate(samples):
        re += s * math.cos(w * i)
        im += s * math.sin(w * i)
    return 20 * math.log10(max(math.hypot(re, im) / len(samples), 1e-12))


def run(script: str, root: Path) -> tuple[int, str]:
    out = subprocess.run([sys.executable, str(HERE / script), "--root", str(root)], capture_output=True, text=True)
    return out.returncode, out.stdout + out.stderr


def reencode(root: Path, rel: str, samples: array.array, ch: int, bitrate: str) -> None:
    tmp = root / "target/selftest-mutation.wav"
    A.write_wav(tmp, samples, ch)
    res = subprocess.run(["ffmpeg", "-nostdin", "-y", "-v", "error", "-i", str(tmp), "-c:a", "libopus", "-b:a", bitrate,
                          str(root / rel)], capture_output=True)
    if res.returncode != 0:
        raise SystemExit(res.stderr.decode())


def main() -> int:
    build()
    code, out = run("master.py", ROOT)
    print(out)
    if code != 0:
        print(f"SELFTEST FAIL: master exited {code} on the synthetic tree")
        return 1
    code, out = run("check.py", ROOT)
    print(out)
    if code != 0:
        print(f"SELFTEST FAIL: audio-check exited {code} on the freshly mastered synthetic tree")
        return 1

    bed_master, _ = A.decode(ROOT / "target/audio-master/ambience/synth-bed.wav")
    src, notched = bed(), (tone_db(bed_master, 247.5) - tone_db(bed_master, 110))
    drop = (tone_db(src, 247.5) - tone_db(src, 110)) - notched
    cut_frames = len(A.decode(ROOT / "target/audio-master/ambience/synth-cut.wav")[0])
    print(f"--- filter: synth-bed 247.5 Hz tone {drop:.1f} dB down against 110 Hz (need >= 20) -> "
          f"{'OK' if drop >= 20 else 'SELFTEST FAIL'}")
    print(f"--- cut: synth-cut masters to {cut_frames} frames (need {3 * A.RATE}) -> "
          f"{'OK' if cut_frames == 3 * A.RATE else 'SELFTEST FAIL'}")
    if drop < 20 or cut_frames != 3 * A.RATE:
        return 1

    cases = []
    hot = ROOT.with_name("audio-selftest-hot")
    master, _ = A.decode(ROOT / "target/audio-master/sfx/synth-hit.wav")
    cases.append((hot, "assets/audio/sfx/synth-hit.webm",
                  array.array("f", (max(-1.0, min(1.0, s * math.pow(10, 9 / 20))) for s in master)), 1, "64k",
                  ["LUFS is more than", "exceeds the -1.0 dBTP ceiling", "hash or size does not match"]))
    seam = ROOT.with_name("audio-selftest-seam")
    loop, _ = A.decode(ROOT / "target/audio-master/ambience/synth-bed.wav")
    tail = int(0.005 * A.RATE)
    broken = array.array("f", loop[:-tail]) + array.array("f", (s + 0.3 for s in loop[-tail:]))
    cases.append((seam, "assets/audio/ambience/synth-bed.webm", broken, 1, "64k", ["loop: wrap step"]))
    phase = ROOT.with_name("audio-selftest-phase")
    stereo, _ = A.decode(ROOT / "target/audio-master/music/synth-music.wav", 2)
    inverted = array.array("f", (-s if i % 2 else s for i, s in enumerate(stereo)))
    cases.append((phase, "assets/audio/music/synth-music.webm", inverted, 2, "128k", ["side energy"]))

    failed = False
    for root, rel, samples, ch, bitrate, expect in cases:
        if root.exists():
            shutil.rmtree(root)
        shutil.copytree(ROOT, root)
        reencode(root, rel, samples, ch, bitrate)
        code, out = run("check.py", root)
        missing = [e for e in expect if e not in out]
        verdict = "OK" if code != 0 and not missing else "SELFTEST FAIL"
        failed |= verdict != "OK"
        print(f"--- forced failure '{root.name}' ({rel}): audio-check exit {code} -> {verdict}")
        print("\n".join(line for line in out.splitlines() if line.startswith(("FAIL", "  -"))))
        if missing:
            print(f"    expected messages not found: {missing}")
    # An untouched generation.template.json must not pass as provenance: master refuses it.
    template = ROOT.with_name("audio-selftest-template")
    if template.exists():
        shutil.rmtree(template)
    shutil.copytree(ROOT, template)
    (template / "assets-source/audio/music/synth-music/generation.json").write_text(
        (A.REPO / "audio/music/generation.template.json").read_text().replace("<cue-id>", "synth-music"))
    code, out = run("master.py", template)
    ok = code != 0 and "template placeholders" in out
    failed |= not ok
    print(f"--- forced failure 'audio-selftest-template' (unfilled generation.json): audio-master exit {code} -> "
          f"{'OK' if ok else 'SELFTEST FAIL'}")
    print("\n".join(line for line in out.splitlines() if line.startswith(("FAIL", "  -"))))
    if failed:
        return 1
    print("\nselftest OK: the synthetic tree passes, and each forced failure exits non-zero with its message")
    return 0


if __name__ == "__main__":
    sys.exit(main())
