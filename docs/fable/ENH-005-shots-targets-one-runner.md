# ENH-005: One runner for the scripted shots targets

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15, architecture finding ARC-011(d) |
| **Impact** | Low to medium: a new capture target is a manifest row, and every target gets the black-frame retry |
| **Effort** | M |
| **Depends on** | ARC-011 (Makefile batch), DOC-007 (targets table) |
| **Owner decision needed** | Whether migrated targets keep 1280x800 or move to the runner's 1600x1000 |

## Goal

The scripted native capture targets share one recipe driven by a manifest, so adding one is a data row, the Make targets table can be generated, and every target has the same black-frame check and retry.

## Current state (at `edbc7ad`)

- `Makefile` defines 34 `shots-*` targets across lines 261-834.
- 18 of them follow one hand-copied shape with a `<NAME>_SHOTS` and `<NAME>_DOCS` variable pair (for example `shots-play` at 490, `shots-windmill-play` at 505, `shots-farm-play` at 520):
  ```make
  mkdir -p target/shots $(X_DOCS)
  rm -f target/shots/<prefix>-*.png target/shots/<prefix>-*.json
  bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(X_DOCS)/<script>.json --size 1280x800 --seed 1 --fps off --tilt off [--clock fixed --water_clock fixed] $(ARGS)
  bash scripts/shots/run.sh --jpegs $(X_DOCS) $(X_SHOTS)
  cp target/shots/<state dumps> $(X_DOCS)/
  ```
- A newer generic runner exists: `make shots-piece SCRIPT=<json> OUT=<dir> [ARGS=...] [ENV=...]` (`Makefile:334`, `scripts/shots/piece.py`). It builds once, runs the script under caffeinate at 1600x1000 with tilt-shift off, re-runs once on a black, missing or timed-out frame, and writes JPEG copies, state dumps and `<stem>-shots.json` (host load before and after) into `OUT`. Four targets already call it, in five invocations (`shots-elevation`, `shots-open-tunnels` twice, `shots-raised-rail`, `shots-massif`).
- The remaining targets are irregular (a shell script per feature, web captures through `e2e/`, multi-step water and lookdev sets) and stay as they are.
- Eight targets are missing from `docs/agent-operability.md` (DOC-007), which is what happens when the table is maintained by hand.

## Design

1. **Manifest**: `scripts/shots/targets.json`, one row per migrated target:
   ```json
   { "name": "windmill-play",
     "about": "Windmill tile in play: two frames 1 s apart and a blocked walk",
     "runs": [ { "script": "docs/checkpoints/windmill/windmill-play.json",
                 "out": "docs/checkpoints/windmill",
                 "args": "--scenario play --seed 1 --fps off --clock fixed --water_clock fixed" } ] }
   ```
2. **One pattern rule**: `shots-%:` looks the name up and calls `piece.py` once per run. Explicit rules take precedence over pattern rules in GNU make, so every irregular target keeps working unchanged and migration can go one target at a time.
3. **Runner**: `scripts/shots/target.py <name> [-- extra args]` reads the manifest and invokes `piece.py`'s existing entry point per run. It adds no capture logic of its own.
4. **Generated docs**: `scripts/shots/targets.py --markdown` prints the table rows for the migrated targets; `docs/agent-operability.md` carries them between two marker comments, and a unit test fails when the block is stale.

## Steps

1. Read `scripts/shots/piece.py` for how it sets the window size and which outputs it writes. Decide the size question with the owner: `piece.py` shoots 1600x1000, the 18 old targets shoot 1280x800. Keeping 1280x800 needs a `--size` pass-through (add a `size` field to the row, defaulting to the runner's own); moving to 1600x1000 changes every re-shot frame's dimensions. Default if unanswered: keep each target's current size.
2. Add `scripts/shots/target.py` and `scripts/shots/targets.json` with the four targets that already call `shots-piece`. Add the pattern rule and delete those four explicit targets. Check `make -n shots-<name>` prints the same `piece.py` command as before for each.
3. Add unit tests to `scripts/shots/test_piece.py` (already in `make shots-test`): the manifest parses, names are unique, every `script` path exists, every `out` is under `docs/checkpoints/`.
4. Migrate the 18 old-shape targets in three batches of six. For each: add the row, delete the explicit target and its two variables, and compare outputs. The old recipe copies named state dumps; `piece.py` writes its own set, so list the files each produces and confirm every file the old target committed is still produced under the same name (rename in the row, not in `docs/checkpoints/`).
5. For one target per batch, re-shoot in a booked load-sensitive window and compare with the committed frames: same file names, same dimensions (if the size was kept), and PSNR above 40 dB against the committed JPEG for a fixed-clock target. Do not commit re-shot frames unless they differ visibly; `git checkout -- docs/checkpoints/<feature>` after the comparison.
6. Remove the migrated names from `.PHONY` (pattern targets cannot be `.PHONY` by name; mark the pattern rule's recipe as always-run with a `FORCE` prerequisite).
7. Generate the doc block and add the staleness test. Update `CLAUDE.md` ("Checkpoint evidence ... via the `make shots-*` targets") to name the manifest, and `.claude/skills/blender-asset/` where it tells agents how to add a shots target.

## Files to touch

`Makefile`, `scripts/shots/targets.json` (new), `scripts/shots/target.py` (new), `scripts/shots/test_piece.py`, `docs/agent-operability.md`, `CLAUDE.md`, `.claude/skills/blender-asset/`.

## Verify

- `make shots-test` passes, including the manifest and doc-staleness tests.
- For every migrated target, `make -n shots-<name>` resolves through the pattern rule and names the same script, output folder and flags as the deleted recipe (keep a before/after `make -n` diff in the PR description).
- `grep -c '_SHOTS *=' Makefile` is 0.
- Booked window: three re-shot targets (one per batch) produce the committed file names; fixed-clock frames compare above 40 dB.
- `make checkall-ci` passes.

## Rollback

Each batch is its own commit that deletes explicit targets and adds rows; reverting a batch restores those targets, and the pattern rule coexists with them.

## Out of scope

The web capture targets under `e2e/`, the irregular native targets, and `make thumbs`.
