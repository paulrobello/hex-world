# train_carriage_b friction log

1. **Base needed the rail branch.** Main did not have the rail network commit, so the worktree merged
   `worktree-agent-ae73eccce4fd0ffe0`. Two conflicts: `crates/hex-app/src/thumbs.rs` (the stage count;
   main's hero-prop rotations took it to 22 + 99 and the rail tiles to 27 + 102, merged 27 + 120 = the
   sum of the merged `want` list, `cargo test -p hex-app --bin hex-world thumbs` passes) and the binary
   `docs/checkpoints/thumbs/contact-sheet.png` (took the rail branch's; `make thumbs` was not run, so it
   predates the oak rotations). About 3 minutes. No skill change: the orchestrator's landing resolves it.
2. **The veranda's back wall read as a hole.** Charcoal like the carriage ends, under the roof's
   overhang and facing away from the kit sun, it rendered near black in the shade-side close-up, so the
   veranda looked like an opening into the body. Painted it the body colour (only COLOR_0 and the hooks
   change) and rebuilt: one extra build, compress and provenance cycle, about 8 minutes. Proposed text
   for `pitfalls.md`, "Look-dev and engine shots": "A recessed wall under an overhang that faces away
   from the kit sun (a veranda's back wall) reads as a hole in a dark colour: give it the body paint or
   a mid tone, and look at a shade-side close still before the first bake."
3. **hex-play does not depend on hex-sim.** A test tying `hex_sim::rail::CONSIST` to the carriages'
   bake reports cannot live in hex-play; split it into `trains::the_carriage_designs_share_one_envelope`
   (hex-play: both bake reports give the same length, width, axles, wheels and buffer faces) and
   `the_brake_third_runs_last_both_ways` (hex-sim `tests/rail.rs`). About 3 minutes. No skill change.
4. **The brief's verify command misses two crates.** `cargo test -p hex-play -p hex-sim -p hex-core
   --lib` does not compile hex-app (whose `sim.rs` match gains an arm) or run hex-sim's integration
   tests, so the patch was also checked with `cargo test -p hex-sim --test rail`, `cargo test -p hex-app
   --bin hex-world sim`, `cargo test -p hex-render --lib kit_scene`, clippy `-D warnings` and fmt on the
   four crates. No lost time; proposed `dispatch.md` text for consist or other Rust-enum patches: "verify
   with `make checkall`, or at least every crate that matches on the enum".
5. **Aiming at moving trains.** Liveries draw only in `scenario=play`, where trains move. Under
   `--clock fixed` the same command sequence puts every train at the same place each run, so one probe
   run with `dump_state` every 300 frames (`play.train_list`: front x/z, speed, waiting) gave the
   positions, and every camera in `docs/checkpoints/train_carriage_b/play.json` framed on the first
   real run (a train at 6 m/s moves 0.1 m a frame, so a few frames of command drift do not matter; a
   train waiting at a junction stands still for seconds). Proposed text for `pitfalls.md`, "Look-dev and
   engine shots": "Moving vehicles in play shots: run the layout once under `--clock fixed` with
   `dump_state` every few hundred frames, read `play.train_list` (or `cars`), then aim each `camera`
   where the vehicle will be; positions repeat run to run for the same command sequence."
