# Hex World audio sources

This folder holds the provider sources, generation records, provenance and the cue registry for every sound in Hex World (PRD §8, §11.3, §11.4, D13, D21). The runtime files live in `assets/audio/`.

## Layout

```
assets-source/audio/
  cues.json                      registry: every cue's class, channels, loop flag, events, prompt
  generation-log.json            every ElevenLabs POST, failed ones included (cap: generationCap)
  review.html                    owner audition page, rewritten by make audio-master
  {sfx,ambience,music}/<cue-id>/
    <cue-id>.{mp3,wav}           untouched provider output (the source master); music sources are git-ignored (local store)
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
| `make audio-check` | Validates every runtime file. It checks loudness against the class target (±1 LU) and true peak (≤ −1 dBTP). For loops it checks the wrap step, the head-to-tail level and interior dropouts, at the manifest loop points, on the decoded WebM and m4a. For one-shot pool variants it checks the floor criterion (below). It checks the mono fold of stereo cues, provenance completeness and hashes, the decoded total against 96 MB, and that no runtime file is left without a cue. |
| `make audio-check-selftest` | Masters a synthetic tree in `target/audio-selftest` and requires a pass. It then requires a non-zero exit for each of three forced failures: a 9 dB hot file, a clicking loop seam, and an inverted stereo channel. |
| `python3 scripts/audio/generate_elevenlabs.py <cue-id>...` | Generates ElevenLabs sources from `cues.json` prompts, one sequential POST at a time. It logs every attempt and stops at `generationCap`. A cue that already has a source needs `--reason`, and its old take moves to `rejected/`. This costs credits. |
| `python3 scripts/audio/screen.py <file>...` | Screens a take before it is mastered. It reports the band energy split, level steadiness (1 s windows), tonal peaks (the 200 Hz series and others) and 2–6 kHz bird chirps. `--floor` adds the one-shot floor criterion, the same function `audio-check` runs. It needs numpy, so it is in no make gate. `--selftest` checks the measures on synthetic controls. |

The tools are python3 (stdlib only) and ffmpeg with libopus. They were built and verified with ffmpeg 9.0.2, libopus 1.6.1 and Python 3.14.8. `audio-check` is not part of `make checkall`, because CI has no ffmpeg.

## Local source store (music)

A Suno take is about 34 MB of WAV, so music sources are never committed. `.gitignore` excludes `assets-source/audio/music/*/*.{wav,mp3}`, and the main checkout's `assets-source/audio/music/<cue-id>/<cue-id>.wav` is the store. `generation.json` and `provenance.json` beside each source are committed. The scripts (`audio_lib.Layout.stores`) look for a source in `$HEX_MUSIC_STORE` when it is set, else in the tree's own `assets-source/audio/music`, then, from a linked git worktree, in the main checkout's (found through `git rev-parse --git-common-dir`). Provenance records the source as `{"store": "local:assets-source/audio/music", "key": "<cue-id>/<cue-id>.wav", "sha256", "bytes"}` with no host path. `make audio-check` verifies the hash and size wherever a store holds the source. Where none does (CI, a fresh clone), it prints "source in local store, not present" as a note and still checks the runtime files, loudness, loops and the generation record. ElevenLabs sources stay committed as before.

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

Per-cue fields in `cues.json` that change mastering:
- `filter`: `{"highpassHz": 150, "lowpassHz": 9500, "stages": 2, "harmonicNotch": {"fundamentalHz": 200, "harmonics": 48, "widthHz": 6}}` runs right after decode, before the loop handling or the cut. `stages` repeats each 2-pole pass (12 dB per octave per stage). `harmonicNotch` may be a list of series. Loops are filtered with one second of circular context on each side, so the wrap stays continuous. ElevenLabs loop-mode takes carry a stationary 200 Hz harmonic series, the hum the owner heard in `forest-birdsong`, measured 2026-10-14 at +35 dB over the noise floor in its quietest frames. A weaker comb on other multiples of 50 Hz (250, 300, 1850, 2150 and 2550 Hz, +14 to +16 dB) and broadband hiss stayed after the notch, and the owner still heard the noise, so the loop was retired. The `forest-birds` cuts avoid both by band: their calls hold under 4% of their energy below 2.8 kHz (centroids 4.5–6 kHz), and the source floor holds 58% of its power over 9 kHz, so they pass 2.8–9.5 kHz with two stages each way.
- `loopEndSeconds`: a loop's body ends here in the source, and everything after (a take's fade-out or final chord) is dropped before the loop check and the crossfade, so the fold joins groove to groove. The music loops set it with a 2 s `loopCrossfadeSeconds`, at the point after which 10 s of onsets best match the opening's (spectral-flux correlation), so the fold lands on the beat. Provenance records it as the `trim-loop-end` step.
- `cut`: `{"startSeconds", "seconds", "fadeInSeconds", "fadeOutSeconds"}` masters a faded excerpt of a longer take as a one-shot, in place of the silence trim (`meadow-bees`, the `forest-birds` cuts, and `discovery-stinger`, the first 7.5 s of a 3-minute Suno take). Several cues may cut one source, each with its own `sourceCue` and `cut`.
- `gate`: `{"thresholdDbBelowPeak", "ratio", "rangeDb", "attackMs", "releaseMs"}` runs on a one-shot after the cut or trim, before leveling. It is a downward expander: the threshold sits `thresholdDbBelowPeak` under the excerpt's loudest 50 ms window, so it follows the take rather than an absolute level, and under it the gain falls by `ratio - 1` dB per dB, to at most `rangeDb` down. Provenance records the absolute threshold and how much of the cue the gate held closed.
- `sourceCue`: masters another cue's source and generation record (each `forest-birds-N` names the retired `forest-birdsong`). `filter`, `cut`, `gate` and `bitrates` belong to the cue itself.
- `pool`: groups one-shot variants that the runtime picks among at random (`forest-birds`, `forest-leaves`, the car pools and the P3 farm, windmill, wheat, shop-bell and train pools). The manifest entry carries it. Every pool variant must pass the floor criterion. A take whose silence trim leaves no quiet interior (one call with its own silent tail, as `farm-sheep-2`) keeps the tail through a `cut`, and a rustle that only swells a little over its own bed (`wheat-rustle-2`) passes through a `gate`. Screen the mastered `assets/audio/<kind>/<cue>.webm` with `screen.py --floor` for the real number: the raw take still has the provider's silent head and tail.
- `retired`: the cue is not mastered, checked or shipped, and must not be in the manifest. It stays in the registry with its source, generation record, provenance and `rejected/` records, so the generation log and any `sourceCue` still resolve (`forest-birdsong`, `meadow-bed`, `meadow-bed-a` to `d`, retired by the owner 2026-10-14).
- `bitrates`: `{"webm": "96k"}` overrides the class encode bitrate for one cue. Opus at the 64 kbps loop default overshot the true peak of the hiss-heavy `meadow-bed-d` by 3.3 dB, and the trim that followed left it 2.7 LU under target. At 96 kbps it overshoots by 0.7 dB.
- `review`: review-page notes for a candidate. `round` groups candidates, with the newest round listed first. `roundNote` explains the round. `screening` holds the numbers from `scripts/audio/screen.py`, and `filterApplied` says why a filter was or was not used.
- `candidateFor`: a review candidate. It is mastered and checked like any cue. Its files go to `assets-source/audio/candidates/`, so the web build never ships them, and its manifest entry goes under `candidates`, which the game never reads. `candidates/<cue>-previous.{webm,m4a}` holds a cue's runtime files from before a re-master, for the review page's A/B.

## Floor criterion (one-shot pools)

The owner heard the hum and hiss under the loop beds as noise (2026-10-14), so every pool variant must settle to a quiet floor. `audio_lib.floor_report` measures the mono fold of the decoded WebM and m4a in 50 ms RMS windows. The interior leaves out the `cut` fades, which ramp to silence. The floor is the 10th-percentile interior window, and it must sit at least 40 dB under the loudest window. Its band split (under 1.5 kHz, 1.5–9 kHz, over 9 kHz) is reported, and the floor fails as a hiss when 50% or more of its power is over 9 kHz while it is still within 60 dB of the peak. `make audio-check` runs it on every cue with a `pool`, and `make audio-master` records it in each runtime record of the cue's provenance (`runtime[].floor`), which `review.html` shows.

The criterion does not see noise during a call. The `forest-birds` cuts come from a quiet take (−61 dBFS median), and their calls stand this far over the take's 2.8–9.5 kHz floor: `-1` +30 dB, `-2` +34 dB, `-3` about +19 dB, `-4` about +17 dB and `-5` about +18 dB. Variants 3 to 5 carry more hiss inside their notes.

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
| none (continuous) | `river-babble`, `surf` | loop | Per-tile emitters on river and beach/ocean tiles. They are positional and weighted by the focus mix (§8.1). The same loops also play as a quiet non-positional bed by the river and coast share of the view. Meadows and forests have no loop and no bed (owner, 2026-10-14: not every tile type needs a constant sound). |
| none (occasional) | `forest-birds-1`, `-2`, `-3` and `-5` (pool `forest-birds`; the owner dropped `-4`) | sfx | One birdsong phrase, positional at a forest tile within 3 hex steps of the focus, every 12–40 s at random, never the same variant twice in a row. |
| none (occasional) | `forest-leaves-1` and `-2` (pool `forest-leaves`; the owner dropped `-3`) | sfx | One leaf rustle, positional at a forest tile within 3 hex steps of the focus, every 20–60 s at random. |
| none (occasional) | `meadow-bees` | sfx | A 6 s bee-swarm one-shot on the ambience bus, positional at a wildflower meadow tile within 3 hex steps of the focus, every 45–120 s at random. It is an excerpt of a continuous hum, so it is exempt from the floor criterion. |
| none (occasional) | `farm-cow-1`, `-2` (pool `farm-cow`) and `farm-sheep-1`, `-2` (pool `farm-sheep`) | sfx | P3 (owner, 2026-10-19). One moo, and on its own timer one bleat, positional at a `farm` tile within 3 hex steps of the focus, each every 30–90 s at random, at level 0.25. |
| none (occasional) | `windmill-creak-1`, `-2` (pool `windmill-creak`) | sfx | A slow wooden shaft-and-sail creak, positional at a `windmill` tile within 3 hex steps of the focus, every 25–70 s at random, at level 0.3. |
| none (occasional) | `wheat-rustle-1`, `-2` (pool `wheat-rustle`) | sfx | A dry wheat-ears rustle in a gust, positional at a `wheat_field` tile within 3 hex steps of the focus, every 20–60 s at random, at level 0.3. These three rows match the tile id, since farm, windmill and wheat field share the meadow biome and scatter. |
| `carParked` at a bakery | `shop-bell-1`, `-2` (pool `shop-bell`) | sfx | A small shop-door bell, 1.2 s after a car parks at a spot whose cell (the event's `q`, `r`) holds a `bakery` tile, after its door. It plays at the spot at level 0.35 times the focus mix, only within 3 hex steps of the focus. It is skipped while muted or with the ambience bus at 0, and never repeats the last variant (`bellStep` and `SHOP_BELL` in `web/ui/src/audio-mix.ts`). The cues list no event: the runtime routes the park. |
| none yet | `train-whistle-1`, `-2` (pool `train-whistle`), `train-chuff-1` (pool `train-chuff`) | sfx | Mastered, not played: there is no rail tile or train in play yet. `hexWorld.audio.play()` sounds them non-positionally. The chuff is a 6 s pass-by one-shot, not a loop. |
| `carPulledOut` | `car-start-2` (pool `car-start`) | sfx | Owner request 2026-10-14. An engine start and pull-away, positional at the parking spot the car reverses out of (the event's `x`, `z`). |
| `carParked` | `car-door-2`, `-3` (pool `car-door`) | sfx | A car door closing at the spot where the car finished parking. |
| none (`play.car_list`) | `car-drive-2` (pool `car-drive`) | sfx | A soft pass-by, once per trip, at a car in its `road` phase. The panner and the focus-mix gain follow the car while it plays. |
| on demand | any pool or cue | | `hexWorld.audio.play(name)` plays a pool variant or a single cue now, positional at the nearest matching tile when there is one (`docs/agent-operability.md`). |
| none (biome mix) | `meadow-day`, `coast-breeze`, `hearth-town` | music | Streamed through a media element after the first gesture. They crossfade by the biome mix in view (6 s equal power, 30 s dwell). |
| photo mode open (`play.photo_mode`) | `photo-mode` | music | Takes over at once while photo mode is open, and the biome pick returns at once when it closes. |
| none yet | `title-theme`, `discovery-stinger` | music | Mastered, not played: the game has no title screen yet, and the recipe discovery moment is P3. `discovery-stinger` is decoded, so it is loaded with the other decoded cues. |

The car cues sit on the ambience bus, so the player's ambience level (10% by default) sets them with the birds and leaves. Each plays at its row level (start and door 0.4, drive 0.3) times the focus mix (`emitterGain`), only within 3 hex steps of the focus, and is skipped while muted or with the ambience bus at 0. At most 3 car voices sound at once and a car never has two. A trip event waits up to 2 s for a free voice and is then dropped. Pass-bys start at least 1.5 s apart, the loudest first. Each car plays at its own rate within 5% of 1, so the small pools vary. The car pools hold one or two variants because three takes failed mastering or the floor criterion; each retired cue's `retiredNote` records why. Neither `cues[].events` entry plays every variant: the runtime routes each trip event to one pick from its pool (`CAR_CUES` in `web/ui/src/audio-mix.ts`). `hexWorld.audio.state().cars` reports the voices, waiting events and recent car plays.

Events with no cue yet are `hoverChanged`, `toolChanged`, `inventoryToggled`, `photoModeToggled` and `actionRefused`. `riseStage` has no cue of its own: the runtime (`web/ui/src/audio.ts`) holds `place-thunk` until the tile's `settled` stage and drops it on `sink`.

## Rights (D21)

The owner holds paid ElevenLabs and Suno subscriptions and owns the rights to the generated audio. Each `provenance.json` records the provider, the subscription and that D21 statement. The basis is the owner's attestation in the PRD. The scripts do not re-verify the providers' current terms.
