#!/usr/bin/env python3
"""make audio-check: validate every runtime audio file against PRD §8.3, §10 and §11.4.

Usage: python3 scripts/audio/check.py [--root DIR]

For each cue in <root>/assets-source/audio/cues.json:
  - the WebM and m4a decode completely, at 48 kHz with the cue's channel count;
  - integrated loudness is within the tolerance of the class target and true peak is at or under
    the ceiling, measured with the same function make audio-master uses;
  - loops: the decoded runtime file spans the manifest loop points, and the wrap at those points
    passes the wrap-step, head-to-tail level and interior-dropout checks;
  - stereo cues survive a mono fold (side energy under mid energy);
  - provenance is complete and its source, generation-record and runtime hashes match the files;
  - the decoded size of every cue the runtime decodes (Web Audio float32 at 48 kHz) fits the budget;
  - one-shot pool variants (`pool`): the floor criterion of audio_lib.floor_report, on both decodes.
    meadow-bees has no pool and is exempt: it is an excerpt of a continuous hum by design.
A music cue with no source yet is reported as pending, not failed. A `retired` cue is skipped and must
not be in the manifest. Exits 1 on any failure.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import audio_lib as A

EXPECTED_CODEC = {"webm": "opus", "m4a": "aac"}
REQUIRED = {
    "provider.name", "provider.model", "provider.subscription", "generation.id", "source.sha256",
    "loudness.before", "loudness.after", "toolVersions.ffmpeg", "license", "approvals", "runtime",
}


def get(d: dict, dotted: str):
    for part in dotted.split("."):
        d = d.get(part) if isinstance(d, dict) else None
    return d


def check_cue(layout: A.Layout, reg: dict, cue: dict, manifest: dict, rows: list, failures: list) -> int:
    """Checks one cue; returns its decoded bytes when the runtime decodes it, else 0."""
    cid = cue["id"]
    fail = lambda msg: failures.append(f"{cid}: {msg}")  # noqa: E731
    prov_path = layout.cue_dir(cue) / "provenance.json"
    entry = manifest.get(cid)
    sources = [p for p in layout.cue_dir(cue).glob(f"{cid}.*")] if layout.cue_dir(cue).exists() else []
    if not prov_path.exists():
        if cue["provider"] == "suno" and not sources:
            rows.append((cid, cue["class"], "-", "pending: no source yet", "", "", "", ""))
            return 0
        fail("no provenance.json; run make audio-master")
        return 0
    prov = json.loads(prov_path.read_text())

    missing = [k for k in sorted(REQUIRED) if get(prov, k) in (None, "", [], {})]
    if not (prov.get("prompt") or prov.get("promptFile")):
        missing.append("prompt")
    if "D21" not in (prov.get("license") or ""):
        missing.append("license (D21 statement)")
    missing += [f"{p} (template placeholder)" for p in A.placeholders(
        {"provider": prov.get("provider"), "generation": prov.get("generation"), "prompt": prov.get("prompt")})]
    if missing:
        fail(f"provenance incomplete: {', '.join(missing)}")
    for label, rec in (("source", prov.get("source") or {}), ("generation record", get(prov, "generation.record") or {})):
        path = layout.root / rec.get("path", "")
        if not rec.get("path") or not path.is_file():
            fail(f"{label} file missing: {rec.get('path')}")
        elif A.sha256(path) != rec.get("sha256"):
            fail(f"{label} hash mismatch for {rec['path']}")
    want = "candidate" if cue.get("candidateFor") else "mastered"
    if entry is None or entry.get("status") != want:
        fail(f"not listed as {want} in assets/audio/manifest.json")

    cls = reg["classes"][cue["class"]]
    target, tol, ceiling = float(cls["lufs"]), float(reg["lufsTolerance"]), float(reg["truePeakCeiling"])
    loop = prov.get("loop")
    if cue["loop"] and not loop:
        fail("loop cue has no loop points in provenance")
    by_ext = {r["path"].rsplit(".", 1)[1]: r for r in prov.get("runtime", [])}
    decoded_bytes = 0
    for ext in ("webm", "m4a"):
        path = layout.runtime_file(cue, ext)
        rec = by_ext.get(ext)
        if not path.is_file():
            fail(f"missing runtime file {layout.rel(path)}")
            continue
        digest = A.sha256(path)
        if rec is None or rec.get("sha256") != digest or rec.get("bytes") != path.stat().st_size:
            fail(f"{ext}: provenance hash or size does not match {layout.rel(path)}")
        if entry and entry.get("files", {}).get(ext, {}).get("sha256") != digest:
            fail(f"{ext}: manifest hash does not match {layout.rel(path)}")
        try:
            info = A.probe(path)
            samples, ch = A.decode(path)
        except RuntimeError as err:
            fail(str(err))
            continue
        if info["codec"] != EXPECTED_CODEC[ext]:
            fail(f"{ext}: codec {info['codec']}, expected {EXPECTED_CODEC[ext]}")
        if info["sampleRate"] != A.RATE:
            fail(f"{ext}: sample rate {info['sampleRate']}, expected {A.RATE}")
        if ch != cue["channels"]:
            fail(f"{ext}: {ch} channels, expected {cue['channels']}")
        m = A.loudness(path)
        if abs(m["integrated"] - target) > tol:
            fail(f"{ext}: integrated {m['integrated']:.1f} LUFS is more than {tol} LU from the {target} {cue['class']} target")
        if m["truePeak"] > ceiling:
            fail(f"{ext}: true peak {m['truePeak']:.1f} dBTP exceeds the {ceiling} dBTP ceiling")
        frames = len(samples) // ch
        seam = ""
        if loop:
            start, end = loop["startSample"], loop["endSample"]
            if ext == "webm" and frames != end:
                fail(f"webm decodes to {frames} frames, the loop end is {end}: not sample-accurate")
            if frames < end:
                fail(f"{ext}: decodes to {frames} frames, short of the loop end {end}")
            else:
                report = A.loop_report(A.mono_fold(samples, ch), start, end)
                seam = f"wrap {report['wrapStep']:.4f}/{report['stepP999']:.4f} head {report['headDb']} tail {report['tailDb']}"
                for problem in report["problems"]:
                    fail(f"{ext} loop: {problem}")
        if cue.get("pool"):
            fades = cue.get("cut") or {}
            fl = A.floor_report(path, fades.get("fadeInSeconds", 0.0), fades.get("fadeOutSeconds", 0.0))
            b = fl["floorBandPct"]
            seam = (f"floor -{fl['floorBelowPeakDb']} dB under peak, <1.5k {b['below1500']}% "
                    f"1.5-9k {b['1500to9k']}% >9k {b['above9k']}%")
            for problem in fl["problems"]:
                fail(f"{ext} floor: {problem}")
        if ch == 2:
            side = A.side_ratio_db(samples)
            if side > 0:
                fail(f"{ext}: side energy {side:.1f} dB over mid; a mono fold loses more than half the power")
        if ext == "webm":
            decoded_bytes = frames * ch * 4
        rows.append((cid, cue["class"], ext, f"{frames / A.RATE:.2f}s", f"{m['integrated']:.1f}", f"{m['truePeak']:.1f}",
                     f"{path.stat().st_size / 1024:.1f}", seam))
    if cue.get("candidateFor"):
        return 0  # a review candidate is never loaded by the game
    return decoded_bytes if (cue.get("playback") or cls["playback"]) == "decode" else -decoded_bytes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--root", type=Path, default=A.REPO)
    args = parser.parse_args()
    layout = A.Layout(args.root.resolve())
    reg = layout.registry()
    manifest_path = layout.runtime / "manifest.json"
    raw = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    manifest = {c["id"]: c for c in raw.get("cues", []) + raw.get("candidates", [])}
    rows: list[tuple] = []
    failures: list[str] = []
    decoded = streamed = 0
    runtime_bytes = 0
    for cue in reg["cues"]:
        if cue.get("retired"):
            if cue["id"] in manifest:
                failures.append(f"{cue['id']}: retired but still listed in assets/audio/manifest.json")
            continue
        n = check_cue(layout, reg, cue, manifest, rows, failures)
        decoded += max(n, 0)
        streamed += max(-n, 0)
    for path in sorted(layout.runtime.rglob("*")):
        if path.suffix in (".webm", ".m4a"):
            runtime_bytes += path.stat().st_size
            cid = path.stem
            if not any(c["id"] == cid and c["kind"] == path.parent.name for c in reg["cues"]):
                failures.append(f"{layout.rel(path)}: runtime file with no cue in cues.json")

    header = ("CUE", "CLASS", "FILE", "LENGTH", "LUFS", "dBTP", "KB", "LOOP SEAM (step/p99.9, head/tail dB)")
    widths = [max(len(str(r[i])) for r in [header, *rows]) for i in range(len(header))]
    for row in [header, *rows]:
        print("  ".join(str(c).ljust(w) for c, w in zip(row, widths)).rstrip())
    budget = reg["decodedBudgetBytes"]
    print()
    print(f"runtime files: {runtime_bytes} bytes ({runtime_bytes / 1048576:.2f} MiB)")
    print(f"decoded (Web Audio float32 @ 48 kHz, cues with playback=decode): {decoded} bytes "
          f"({decoded / 1e6:.1f} MB) of {budget / 1e6:.0f} MB budget")
    print(f"streamed music (playback=stream, media element; not counted): {streamed / 1e6:.1f} MB if decoded; "
          f"everything decoded would be {(decoded + streamed) / 1e6:.1f} MB")
    if decoded > budget:
        failures.append(f"decoded total {decoded} bytes exceeds the {budget} byte budget (PRD §10)")
    print()
    if failures:
        print(f"FAIL ({len(failures)}):")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"PASS: {sum(1 for r in rows if r[2] in ('webm', 'm4a'))} runtime files meet loudness, true-peak, loop, "
          f"provenance and decoded-memory contracts")
    return 0


if __name__ == "__main__":
    sys.exit(main())
