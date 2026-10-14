# Hex World — Suno music prompt pack

Generate each cue in Suno, choose the best take, and save it as
`assets-source/audio/music/<cue-id>/<cue-id>.wav`. Use a WAV export if your plan allows it, otherwise MP3.
`make audio-master` normalizes loudness, sets loop points, encodes the runtime files and writes provenance (D13, D21).
The step-by-step owner workflow is in [`../README.md`](../README.md).

## Shared direction (applies to every cue)

- **Feel:** a cozy, living miniature world seen from above: warm, curious, unhurried, a little magical. The world grows tile by tile, so the music should never compete with the placement sounds.
- **Palette:** felt piano, nylon guitar, celesta and glockenspiel, soft strings, a little woodwind (flute, clarinet), light hand percussion, and warm pads. No vocals and no heavy drums.
- **Crossfade-friendly:** the gameplay cues share a BPM band of 84–96 and related keys (D major, G major and A major), so the game can crossfade between them by biome mix (PRD §8.2). Keep each cue's intro and outro gentle.
- **Loops:** gameplay cues loop, so ask for a steady ending with no big finale and no fade-out. Trimming happens in mastering.
- **Mix target:** mastered to −18 LUFS integrated. Leave headroom: the prompts ask for soft dynamics, not loud masters.
- **Suno settings:** turn Instrumental ON. Paste "Style" into the Style field and "Exclude" into Exclude Styles. Paste "Lyrics / structure" into the Lyrics field (structure tags only, no words). Suggested advanced settings: Weirdness about 35%, Style Influence about 70%.
- **No artist names.** Suno rejects them and they cause licensing ambiguity.

## Cues

| Cue | File | Use | Length | BPM | Key |
|---|---|---|---|---|---|
| `title-theme` | [title-theme.md](title-theme.md) | Title / main menu | 2:00–2:30 | 88 | D major |
| `meadow-day` | [meadow-day.md](meadow-day.md) | Grass, meadow and forest dominant in view | 3:00–3:30 loop | 90 | G major |
| `coast-breeze` | [coast-breeze.md](coast-breeze.md) | Beach and ocean dominant | 3:00–3:30 loop | 84 | D major |
| `hearth-town` | [hearth-town.md](hearth-town.md) | Houses and roads dominant | 3:00–3:30 loop | 96 | A major |
| `photo-mode` | [photo-mode.md](photo-mode.md) | Photo mode (D7), free camera | 3:00 loop | 72 (free) | G major |
| `discovery-stinger` | [discovery-stinger.md](discovery-stinger.md) | Recipe unlocked (P3) | 0:05–0:08 | — | D major |

The P2 MVP needs at least 3 gameplay cues: `meadow-day`, `coast-breeze` and `hearth-town`. Night, season and weather variants are planned for P5.

## Tracking

Record the take you pick in `assets-source/audio/music/<cue-id>/generation.json`, copied from [`../generation.template.json`](../generation.template.json): the Suno song ID and link, the model version, the subscription, the date and a note. `make audio-master` copies that record into the cue's `provenance.json` and refuses a source without it.
