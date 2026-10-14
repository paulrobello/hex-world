# Hex World audio sources

This folder holds the provider sources, generation records, provenance and the cue registry for every sound in Hex World (PRD §8, §11.3, §11.4, D13, D21). The runtime files live in `assets/audio/`.

## Layout

```
assets-source/audio/
  cues.json                      registry: every cue's class, channels, loop flag, events, prompt
  generation-log.json            every ElevenLabs POST, failed ones included (cap: generationCap)
  review.html                    owner audition page, rewritten by make audio-master
  {sfx,ambience,music}/<cue-id>/
    <cue-id>.{mp3,wav}           untouched provider output (the source master)
    generation.json              provider, model, subscription, prompt, generation id, credits
    provenance.json              written by make audio-master
    rejected/                    earlier takes; the .mp3 stays local, the .generation.json is committed
assets/audio/
  manifest.json                  runtime index: per cue the files, hashes, loop points, playback mode
  <kind>/<cue-id>.webm           Opus in WebM (primary)
  <kind>/<cue-id>.m4a            AAC in MP4 (fallback, e.g. Safari)
```

## Commands

| Command | What it does |
|---|---|
| `make audio-master [CUE=<id>]` | Masters each cue that has a source. It writes the runtime files, `provenance.json`, `assets/audio/manifest.json` and `review.html`. The output is deterministic: an unchanged source re-encodes to the same bytes. |
| `make audio-check` | Validates every runtime file. It checks loudness against the class target (±1 LU) and true peak (≤ −1 dBTP). For loops it checks the wrap step, the head-to-tail level and interior dropouts, at the manifest loop points, on the decoded WebM and m4a. It checks the mono fold of stereo cues, provenance completeness and hashes, and the decoded total against 96 MB. |
| `make audio-check-selftest` | Masters a synthetic tree in `target/audio-selftest` and requires a pass. It then requires a non-zero exit for each of three forced failures: a 9 dB hot file, a clicking loop seam, and an inverted stereo channel. |
| `python3 scripts/audio/generate_elevenlabs.py <cue-id>...` | Generates ElevenLabs sources from `cues.json` prompts, one sequential POST at a time. It logs every attempt and stops at `generationCap`. A cue that already has a source needs `--reason`, and its old take moves to `rejected/`. This costs credits. |

The tools are python3 (stdlib only) and ffmpeg with libopus. They were built and verified with ffmpeg 9.0.2, libopus 1.6.1 and Python 3.14.8. `audio-check` is not part of `make checkall`, because CI has no ffmpeg.

## Classes and targets

| Class | Integrated loudness | Playback | Use |
|---|---|---|---|
| `music` | −18 LUFS | stream (media element) | Suno cues |
| `sfx` | −16 LUFS | decode (Web Audio buffer) | World one-shots: placement |
| `loop` | −16 LUFS | decode | Ambience emitters and beds, looped gapless |
| `ui` | −20 LUFS | decode | Interface feedback |

Every class has a −1 dBTP true-peak ceiling. Masters are made at −2 dBTP. When an encode still overshoots, the master is trimmed linearly until both encodes sit at least 0.2 dB under the ceiling. Every master and runtime file is 48 kHz, since Opus decodes at that rate. When a cue's true peak would pass the ceiling at its target gain, a 4×-oversampled lookahead limiter takes the excess. Each cue's `provenance.json` records how much was taken and how often (`processing[].requiredPeakReductionDb`, `windowsOverCeilingPct`). A cue that needs more than 14 dB of reduction is mastered linearly to the ceiling instead, with a `deviation` note, and `audio-check` fails it for the owner to decide.

Loops come in two kinds:
- A loop the provider already returns sample-seamless (ElevenLabs loop mode) is rotated so its head and tail sit at the same level.
- Any other loop has its tail folded over its head with an equal-power crossfade.

Either way the loop points are `[0, frames)` of the master, and the WebM decodes to exactly that many frames.

## Cue ids and game events

The runtime audio stream maps the events drained by `hex_world_events()` (`crates/hex-app/src/game.rs`, `GameEvent`) to these cues. The list is also in `assets/audio/manifest.json` (`cues[].events`).

| Event | Cue | Class | Notes |
|---|---|---|---|
| `tilePlaced` | `place-rumble`, then `place-thunk` | sfx | The rumble plays at the start of the staged rise (D15) and the thunk as the tile settles. Both are positional at the tile. |
| `placementRefused` | `ui-invalid` | ui | |
| `bulldozeRefused` | `ui-invalid` | ui | The same "not allowed" cue as a refused placement. |
| `undone` | `ui-undo` | ui | |
| `redone` | `ui-undo` | ui | Shares the undo cue. The runtime may pitch it up slightly. |
| `tileBulldozed` | `ui-bulldoze` | ui | |
| `ghostRotated` | `ui-rotate` | ui | `valid: false` may also play `ui-invalid` softly. That choice belongs to the runtime stream. |
| `tileSelected` | `ui-select` | ui | |
| none (continuous) | `river-babble`, `surf`, `forest-birdsong` | loop | Per-tile emitters on river, beach/ocean and forest tiles. They are positional and weighted by the focus mix (§8.1). |
| none (continuous) | `meadow-bed` | loop | The global bed for grass and meadow in view, non-positional. |
| none (biome mix) | `meadow-day`, `coast-breeze`, `hearth-town` | music | They crossfade by the biome mix in view. `photo-mode` and `title-theme` are pending, and `discovery-stinger` is pending for P3. |

Events with no cue yet are `hoverChanged`, `toolChanged` and `photoModeToggled`.

## Rights (D21)

The owner holds paid ElevenLabs and Suno subscriptions and owns the rights to the generated audio. Each `provenance.json` records the provider, the subscription and that D21 statement. The basis is the owner's attestation in the PRD. The scripts do not re-verify the providers' current terms.
