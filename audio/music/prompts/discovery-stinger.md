# discovery-stinger

- **Use:** a short stinger when a combo recipe unlocks a new tile type (P3). It plays over whatever gameplay cue is running, so keep it short and in D major.
- **Length:** 5–8 seconds. **Key:** D major. **Loop:** no.
- **Role:** a delighted "aha!", built from the rising five-note motif played quickly and resolving with a shimmer.

Suno makes full songs, so generate a short piece and trim the best 5–8 s in mastering. Generating 2–3 takes makes it easier to find a clean moment.

## Style
```
magical discovery fanfare, short whimsical flourish, glockenspiel and celesta run, harp glissando, pizzicato strings, soft brass swell, felt piano, bright and delightful, D major, instrumental, very short, clean resolved ending, soft dynamics
```

## Exclude
```
vocals, choir, drums, edm, epic trailer, heavy brass, dark, long intro
```

## Lyrics / structure
```
[Instrumental]
[Intro: quick rising five-note glockenspiel motif]
[Hook: harp glissando into a warm soft brass and string chord in D major]
[Outro: celesta shimmer, clean resolved ending]
[End]
```

## Chosen take
Record the take you keep in `assets-source/audio/music/discovery-stinger/generation.json`, copied from [`../generation.template.json`](../generation.template.json): the Suno song ID and link, model version, subscription and date. `make audio-master` reads that file into the cue's provenance (D21) and refuses a source without it. See [`../README.md`](../README.md).
