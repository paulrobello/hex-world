"""Shared helpers for make audio-master and make audio-check (PRD §8, §10, §11.3).

Stdlib only, plus ffmpeg/ffprobe on PATH. Master and check measure through the same functions here,
so they cannot disagree about what a file measures.
"""

from __future__ import annotations

import array
import hashlib
import json
import math
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
RATE = 48000  # Opus decodes at 48 kHz, so every master and runtime file uses it.


class Layout:
    """Where one audio tree lives. The repo is the default root; the self-test uses a temp root."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.source = root / "assets-source/audio"
        self.runtime = root / "assets/audio"
        self.work = root / "target/audio-master"
        self.registry_path = self.source / "cues.json"

    def registry(self) -> dict:
        return json.loads(self.registry_path.read_text())

    def cue_dir(self, cue: dict) -> Path:
        return self.source / cue["kind"] / cue["id"]

    def runtime_file(self, cue: dict, ext: str) -> Path:
        return self.runtime / cue["kind"] / f"{cue['id']}.{ext}"

    def rel(self, path: Path) -> str:
        return path.resolve().relative_to(self.root.resolve()).as_posix()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def file_record(layout: Layout, path: Path) -> dict:
    return {"path": layout.rel(path), "sha256": sha256(path), "bytes": path.stat().st_size}


def run(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True)


def probe(path: Path) -> dict:
    out = run(["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)])
    if out.returncode != 0:
        raise RuntimeError(f"{path}: ffprobe failed: {out.stderr.decode().strip()}")
    data = json.loads(out.stdout)
    stream = next(s for s in data["streams"] if s["codec_type"] == "audio")
    return {
        "codec": stream["codec_name"],
        "sampleRate": int(stream["sample_rate"]),
        "channels": int(stream["channels"]),
        "durationSeconds": round(float(data["format"]["duration"]), 4),
        "bitRate": int(data["format"].get("bit_rate", 0)),
    }


def decode(path: Path, channels: int | None = None) -> tuple[array.array, int]:
    """Full decode to interleaved float32 at 48 kHz. Raises on any decoder error."""
    have = probe(path)["channels"]
    want = channels if channels is not None else have
    args = ["ffmpeg", "-nostdin", "-v", "error", "-i", str(path)]
    if want != have:
        args += ["-af", downmix(have, want)]
    args += ["-ar", str(RATE), "-f", "f32le", "-acodec", "pcm_f32le", "-"]
    out = run(args)
    if out.returncode != 0 or out.stderr.strip():
        raise RuntimeError(f"{path}: decode errors: {out.stderr.decode().strip()}")
    samples = array.array("f")
    samples.frombytes(out.stdout)
    return samples, want


def downmix(have: int, want: int) -> str:
    """Amplitude (not power-preserving) fold of stereo to mono, or mono duplicated to stereo.

    aformat=channel_layouts=mono is power-preserving and lifts peaks about 3 dB, and a stereo fold
    expression applied to a mono input silently costs 6 dB, so the input layout is probed first.
    """
    if (have, want) == (2, 1):
        return "pan=mono|c0=0.5*c0+0.5*c1"
    if (have, want) == (1, 2):
        return "pan=stereo|c0=c0|c1=c0"
    raise ValueError(f"no fold from {have} to {want} channels")


def write_wav(path: Path, samples: array.array, channels: int, fmt: str = "f32") -> None:
    """Write interleaved float32 samples as a WAV through ffmpeg (f32 work files, s16 masters)."""
    codec = {"f32": "pcm_f32le", "s16": "pcm_s16le"}[fmt]
    path.parent.mkdir(parents=True, exist_ok=True)
    out = subprocess.run(
        ["ffmpeg", "-nostdin", "-y", "-v", "error", "-f", "f32le", "-ar", str(RATE), "-ac", str(channels),
         "-i", "-", "-fflags", "+bitexact", "-flags:a", "+bitexact", "-map_metadata", "-1",
         "-c:a", codec, str(path)],
        input=samples.tobytes(), capture_output=True)
    if out.returncode != 0:
        raise RuntimeError(f"{path}: write failed: {out.stderr.decode().strip()}")


def loudness(path: Path) -> dict:
    """EBU R128 integrated loudness (LUFS), loudness range (LU) and true peak (dBTP, 4x oversampled).

    Integrated loudness gates on 400 ms blocks, so a file shorter than 0.4 s reads -70 LUFS; the master
    never trims a cue below MIN_ONESHOT_SECONDS for that reason.
    """
    out = run(["ffmpeg", "-nostdin", "-v", "info", "-i", str(path), "-af", "ebur128=peak=true",
               "-f", "null", "-"]).stderr.decode()
    summary = out[out.rfind("Summary:"):]
    found = {
        "integrated": re.search(r"I:\s*(-?[\d.]+|-inf) LUFS", summary),
        "lra": re.search(r"LRA:\s*(-?[\d.]+) LU", summary),
        "truePeak": re.search(r"True peak:\s*\n\s*Peak:\s*(-?[\d.]+|-inf)", summary),
    }
    if any(v is None for v in found.values()):
        raise RuntimeError(f"{path}: ebur128 summary not found")
    return {k: float(v.group(1)) for k, v in found.items()}


def mono_fold(samples: array.array, channels: int) -> array.array:
    if channels == 1:
        return samples
    return array.array("f", ((samples[i] + samples[i + 1]) * 0.5 for i in range(0, len(samples) - 1, 2)))


def window_db(samples: array.array, window: int) -> list[float]:
    """RMS level in dBFS of consecutive windows of a mono signal."""
    levels = []
    for start in range(0, len(samples) - window + 1, window):
        chunk = samples[start:start + window]
        rms = math.sqrt(sum(s * s for s in chunk) / window)
        levels.append(20 * math.log10(rms) if rms > 1e-9 else -180.0)
    return levels


LOOP_WINDOW_S = 0.1
LOOP_EDGE_WINDOWS = 5
LOOP_STEP_CONTEXT_S = 0.5
DROPOUT_DB = -24.0
WRAP_LEVEL_DB = 6.0


def loop_report(mono: array.array, start: int, end: int) -> dict:
    """The three ways a loop fails audibly (create-game-audio-assets skill), measured on [start, end).

    wrapStep: the jump from the last looped sample back to the first, against the p99.9 of the
    sample-to-sample steps within LOOP_STEP_CONTEXT_S on either side of the seam.
    wrapLevel: head versus tail RMS (an audible restart even with a tiny step).
    dropouts: interior windows more than DROPOUT_DB under the loudest window.
    """
    body = mono[start:end]
    ctx = int(LOOP_STEP_CONTEXT_S * RATE)
    around = list(body[-ctx:]) + list(body[:ctx])
    steps = sorted(abs(around[i] - around[i - 1]) for i in range(1, len(around)) if i != ctx)
    p999 = steps[int(len(steps) * 0.999)]
    seam = abs(body[0] - body[-1])
    levels = window_db(body, int(LOOP_WINDOW_S * RATE))
    peak = max(levels)
    head = max(levels[:LOOP_EDGE_WINDOWS])
    tail = max(levels[-LOOP_EDGE_WINDOWS:])
    dropouts = [(round(i * LOOP_WINDOW_S, 1), round(db, 1))
                for i, db in enumerate(levels[LOOP_EDGE_WINDOWS:-LOOP_EDGE_WINDOWS], LOOP_EDGE_WINDOWS)
                if db - peak < DROPOUT_DB]
    problems = []
    if seam > p999:
        problems.append(f"wrap step {seam:.5f} exceeds the p99.9 step {p999:.5f} around the seam")
    if abs(head - tail) > WRAP_LEVEL_DB:
        problems.append(f"head {head:.1f} dB vs tail {tail:.1f} dB differ by more than {WRAP_LEVEL_DB} dB")
    if dropouts:
        problems.append(f"interior dropout more than {-DROPOUT_DB:.0f} dB under peak at {dropouts[:4]}")
    return {"wrapStep": round(seam, 6), "stepP999": round(p999, 6), "headDb": round(head, 1),
            "tailDb": round(tail, 1), "minWindowDb": round(min(levels), 1), "problems": problems}


def side_ratio_db(samples: array.array) -> float:
    """Side-to-mid energy of interleaved stereo in dB. Above 0 dB a mono fold loses over half its power."""
    mid = side = 0.0
    for i in range(0, len(samples) - 1, 2):
        m = samples[i] + samples[i + 1]
        s = samples[i] - samples[i + 1]
        mid += m * m
        side += s * s
    if side <= 1e-12:
        return -120.0
    if mid <= 1e-12:
        return 120.0
    return 10 * math.log10(side / mid)


PLACEHOLDER = re.compile(r"<[^<>]+>")


def placeholders(value: object, path: str = "") -> list[str]:
    """Dotted paths of strings still holding a template placeholder such as `<Suno song id>`."""
    if isinstance(value, dict):
        return [p for k, v in value.items() for p in placeholders(v, f"{path}.{k}" if path else k)]
    if isinstance(value, list):
        return [p for i, v in enumerate(value) for p in placeholders(v, f"{path}[{i}]")]
    return [path] if isinstance(value, str) and PLACEHOLDER.search(value) else []


def tool_versions() -> dict:
    ff = run(["ffmpeg", "-hide_banner", "-version"]).stdout.decode().splitlines()
    libs = {m.group(1): m.group(2).replace(" ", "")
            for line in ff for m in [re.match(r"\s*(libavcodec|libavformat|libavfilter)\s+([\d. ]+)/", line)] if m}
    opus = run(["pkg-config", "--modversion", "opus"])
    return {
        "ffmpeg": ff[0].split()[2] if ff else None,
        **libs,
        "libopus": opus.stdout.decode().strip() if opus.returncode == 0 else None,
        "aacEncoder": "ffmpeg native aac",
        "python": ".".join(map(str, __import__("sys").version_info[:3])),
    }
