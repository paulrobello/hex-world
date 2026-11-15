# road_tunnel_bore friction (2026-11-14, open tunnels)

Each entry gives what happened, the fix and the minutes lost. Sibling asset agents were running, so proposed skill
text is written here for the orchestrator to fold into `.claude/skills/blender-asset/SKILL.md`.

1. **The committed GLB can't be the "before" of a diff (10 min).** `assets/kit/p1/<piece>/<piece>.glb` is
   meshopt-compressed (EXT_meshopt_compression), and Blender's glTF importer cannot read it. The closed portal's
   "before" geometry was rebuilt with `HEXW_SKIP_BAKE=1` from the unwrapped helper into `target/`.
   Proposed skill text: "To diff a regenerated piece against its committed look, rebuild the committed geometry into
   `target/` (`HEXW_SKIP_BAKE=1`, the committed generator, the stage's baked maps). Do not import
   `assets/kit/p1/*.glb`: it is meshopt-compressed and Blender cannot read it."

2. **EEVEE stills draw back faces the game culls (15 min).** The portal's `eevee_still.py` turns back-face culling
   off. The runtime keeps StandardMaterial's back-face culling on every slot without `alphaCutoff`
   (`kit_material.rs`). A before/after diff of the closed portal therefore showed 1,579 eye-level pixels inside the
   15 mm voussoir joints: the back side of the new back patch, a face the game never draws. With culling as the
   runtime does it (`docs/checkpoints/road_tunnel_bore/cull_runtime.py` as a pre-script), the diff is max 2/255 with
   0 pixels over 8.
   Proposed skill text: "An 'exterior unchanged' diff renders with the runtime's culling. Cull back faces on every
   slot without `alphaCutoff` (pre-script `docs/checkpoints/road_tunnel_bore/cull_runtime.py`), or report both
   numbers."

3. **Always-on light has an existing path, but no piece had used it (5 min).** `maps.emissive` (an sRGB
   `<name>.emissive.png` beside the albedo, which kit-compress picks up) is bound with factor WHITE. It is not
   exposure-scaled and is independent of masks B (P5 night). Brightness is capped at the map's 1.0, so the glow
   budget lives in the texture.
   Proposed skill text: "Always-lit features (tunnel lamps) use `maps.emissive` at factor 1. Tune brightness in the
   map, never with masks B, which is reserved for P5 night."

4. **Coarse shell LOD poked through (10 min).** LOD2's outer shell had 3 segments over the 0.45 m-thick extrados.
   Chord sag near the springings (curvature radius b²/a, about 2.1 m) was about 0.45 m, so the lining showed through
   the shell. This was caught in the LOD2-alone still. 5 segments fixed it (+4 tris, LOD2 114).
   Proposed skill text: "A coarse outer shell over a curved inner surface keeps chord sag under its thickness at the
   tightest curvature: sag ≈ L² / (8R). Look at every LOD alone before the sheet."

5. **kit-check needs the cited sheet committed (2 min).** `provenance_notes_cite_only_committed_files` fails until
   `docs/checkpoints/<piece>/sheet.jpg`, which an approvals entry cites, is tracked by git.
   Proposed skill text: "Commit the sheet together with the approvals entry that cites it, then run `make kit-check`."

6. **The brief's gate path named a `make manifest` target (1 min).** That target does not exist. The manifest is
   validated by `make kit-check` (kit_gate).
   Proposed skill text (dispatch template): "manifest: edit `assets/kit/manifest.json`; `make kit-check` validates
   it."

7. **`make ui-e2e` fails at `e2e/ui.mjs` without this branch's changes (25 min).**
   - **The failure.** 20 failures at "slot key prints what the slot needs", and onward, plus the console error
     `TypeError: Cannot read properties of undefined (reading 'toLowerCase')`. `make` stops there.
   - **Not this branch.** Swapping `dist/`'s kit manifest and portal GLB and masks back to 170094d gave the same 20
     failures. The only other change in the wasm is the kit scenario's layout, which the play scenario never reads.
   - **The other scripts, run one by one.** ui-motion, persist and audio fail with the same TypeError (audio also
     with `reading 't'` at `audio.mjs:199`). photo-fx, saves-ui, debug and version pass.
   - **Not done.** The HUD bug was not traced: it is outside this piece's ownership, so it goes to the orchestrator.
   - **No skill change.**
8. **The worktree hook refused compound shell (5 min).** A heredoc python command and a git command with
   redirection were refused. The fix is scripts under `target/scratch/`, each run as one plain command. No skill
   change: the dispatch brief already states the rule.
