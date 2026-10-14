#!/usr/bin/env python3
"""Generate ElevenLabs sources for cues in assets-source/audio/cues.json (PRD §8.1, D21).

Usage: python3 scripts/audio/generate_elevenlabs.py <cue-id> [<cue-id> ...] [--reason "why a retry"]

Uses the elevenlabs skill's module (~/.claude/skills/elevenlabs/scripts/generate.py) for key
resolution, payload validation and file naming, and posts the request itself because the skill's
CLI discards the response headers, which carry the generation id provenance needs.

Every POST is appended to assets-source/audio/generation-log.json before the next one starts, failed
ones included, and the script refuses to post once the log holds `generationCap` attempts. Calls run
sequentially. The subscription is queried before and after each call to record its credit cost.
A cue that already has a source is a retry: the previous take moves to <cue>/rejected/ and --reason
is required.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import importlib.util
import json
import shutil
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CUES = ROOT / "assets-source/audio/cues.json"
LOG = ROOT / "assets-source/audio/generation-log.json"
SKILL = Path.home() / ".claude/skills/elevenlabs/scripts/generate.py"
SUBSCRIPTION_API = "https://api.elevenlabs.io/v1/user/subscription"
OUTPUT_FORMAT = "mp3_44100_128"  # Starter tier: pcm_* needs Pro+, mp3_48000_192 needs Creator+.
MODEL = "eleven_text_to_sound_v2"


def load_skill():
    spec = importlib.util.spec_from_file_location("elevenlabs_generate", SKILL)
    if spec is None or spec.loader is None:
        sys.exit(f"elevenlabs skill not found at {SKILL}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def subscription(key: str) -> dict:
    """Account usage, or None values when the endpoint rate-limits (HTTP 429 seen mid-batch).

    The character-cost response header is the per-call charge; this count lags it.
    """
    req = urllib.request.Request(SUBSCRIPTION_API, headers={"xi-api-key": key})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.load(resp)
    except urllib.error.URLError:
        data = {}
    return {k: data.get(k) for k in ("tier", "character_count", "character_limit", "status")}


def now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("cues", nargs="+")
    parser.add_argument("--reason", default=None, help="required when a cue already has a source")
    args = parser.parse_args()

    registry = json.loads(CUES.read_text())
    by_id = {c["id"]: c for c in registry["cues"]}
    cap = registry["generationCap"]
    log = json.loads(LOG.read_text()) if LOG.exists() else {"cap": cap, "attempts": []}

    skill = load_skill()
    key = skill.get_api_key()
    if not key:
        sys.exit("ELEVENLABS_API_KEY not found in ~/.env or ~/.claude/.env; generation stopped")
    skill_sha = hashlib.sha256(SKILL.read_bytes()).hexdigest()

    for cue_id in args.cues:
        cue = by_id.get(cue_id)
        if cue is None or cue.get("provider") != "elevenlabs":
            sys.exit(f"{cue_id}: not an elevenlabs cue in {CUES.relative_to(ROOT)}")
        if len(log["attempts"]) >= cap:
            sys.exit(f"generation cap reached: {len(log['attempts'])}/{cap} attempts logged; stopping")
        gen = cue["generation"]
        cue_dir = ROOT / "assets-source/audio" / cue["kind"] / cue_id
        source = cue_dir / f"{cue_id}.{skill.ext_for_format(OUTPUT_FORMAT, 'sfx')}"
        previous = json.loads((cue_dir / "generation.json").read_text()) if (cue_dir / "generation.json").exists() else None
        retry = source.exists()
        if retry:
            if not args.reason:
                sys.exit(f"{cue_id}: a source exists; pass --reason to spend a retry")
            rejected = cue_dir / "rejected"
            rejected.mkdir(exist_ok=True)
            take = previous["attempt"] if previous else 0
            shutil.move(str(source), rejected / f"{cue_id}.attempt{take}{source.suffix}")
            if previous:
                previous["rejectedReason"] = args.reason
                write_json(rejected / f"{cue_id}.attempt{take}.generation.json", previous)

        ns = argparse.Namespace(text=gen["text"], duration_seconds=gen["durationSeconds"],
                                prompt_influence=gen["promptInfluence"], loop=gen["loop"], model=MODEL)
        payload = skill.build_sfx_payload(ns)
        before = subscription(key)
        attempt = {"n": len(log["attempts"]) + 1, "cueId": cue_id, "requestedAt": now(),
                   "reason": args.reason if retry else "first take",
                   "creditsBefore": before["character_count"]}
        log["attempts"].append(attempt)
        write_json(LOG, log)  # durable before the POST, so a crash still counts the attempt

        url = f"{skill.SFX_API}?output_format={urllib.parse.quote(OUTPUT_FORMAT)}"
        req = urllib.request.Request(url, data=json.dumps(payload).encode(), method="POST",
                                     headers={"xi-api-key": key, "Content-Type": "application/json",
                                              "Accept": "audio/mpeg, application/json"})
        try:
            with urllib.request.urlopen(req, timeout=300) as resp:
                audio = resp.read()
                headers = {k.lower(): v for k, v in resp.headers.items() if k.lower() != "set-cookie"}
                status = resp.status
        except urllib.error.HTTPError as err:
            attempt.update(status="failed", httpStatus=err.code, error=err.read().decode("utf-8", "replace")[:500])
            attempt["creditsAfter"] = subscription(key)["character_count"]
            write_json(LOG, log)
            sys.exit(f"{cue_id}: HTTP {err.code}: {attempt['error']}")
        if "json" in headers.get("content-type", ""):
            attempt.update(status="failed", httpStatus=status, error=audio[:500].decode("utf-8", "replace"))
            write_json(LOG, log)
            sys.exit(f"{cue_id}: expected audio, got {attempt['error']}")

        after = subscription(key)
        cue_dir.mkdir(parents=True, exist_ok=True)
        source.write_bytes(audio)
        # The sound-generation endpoint returns no history-item-id or request-id (observed
        # 2026-10-14): x-trace-id is the only per-request identifier, character-cost the charge.
        generation_id = headers.get("history-item-id") or headers.get("request-id") or headers.get("x-trace-id")
        cost = headers.get("character-cost")
        attempt.update(status="ok", httpStatus=status, generationId=generation_id,
                       creditsAfter=after["character_count"],
                       creditsCharged=int(cost) if cost is not None else None,
                       subscriptionDelta=(after["character_count"] - before["character_count"]
                                          if after["character_count"] is not None and before["character_count"] is not None else None),
                       file=str(source.relative_to(ROOT)))
        write_json(LOG, log)
        record = {
            "cueId": cue_id,
            "provider": "ElevenLabs",
            "endpoint": "POST /v1/sound-generation",
            "model": MODEL,
            "subscription": f"ElevenLabs {str(after['tier'] or before['tier'] or 'starter').title()} (paid, owner account; tier from GET /v1/user/subscription)",
            "outputFormat": OUTPUT_FORMAT,
            "prompt": gen["text"],
            "parameters": payload,
            "generationId": generation_id,
            "responseHeaders": headers,
            "generatedAt": attempt["requestedAt"],
            "attempt": attempt["n"],
            "creditsCharged": attempt["creditsCharged"],
            "sourceFile": source.name,
            "tool": {"elevenlabsSkill": str(SKILL).replace(str(Path.home()), "~"), "elevenlabsSkillSha256": skill_sha},
        }
        write_json(cue_dir / "generation.json", record)
        print(f"{cue_id}: attempt {attempt['n']}/{cap}, {len(audio)} bytes, "
              f"credits {attempt['creditsCharged']}, id {generation_id}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
