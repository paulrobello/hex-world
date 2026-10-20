# rail_junction_center friction

- **Two swept ballast beds with a piecewise height fn left grass under the switch** (5 min). My first
  `bed_z(t)` interpolated the profile's half (t >= 0) from the sink inward and extrapolated the slope
  above BALLAST_TOP inside the shoulder, so each bed "hid" the other's flat top and both were clipped
  away near the toe. Fix: flat top for |t| <= SHOULDER_T, keep the straight's bed whole (it carries the
  edge-0 and edge-3 sections) and clip only the branch bed against it, strictly at the first profile
  point so the toe frame (equal heights all across) keeps its row. Skill text: none needed (piece-local).
- **Long timbers square to the main line leave the 8 m branch unsupported past the frog** (10 min).
  The bend's radial sleepers lean back over the timbers, so after the last timber the branch has a
  run with nothing under it. The timber count is chosen in the generator (`best_timbers`: smallest
  widest gap, 8 timbers, 1.67 m); shortening the blocked branch sleepers to just past the rails' feet
  freed none of them. A real fix is skewed or fanned timbers under the vee; left for the owner to ask.
  Proposed skill text (pitfalls, "Turning road pieces" sibling): "A turnout on a tight radius cannot
  hold square timbers under its branch: on the 8 m rail arc the timbers reach 4.8 m by the frog
  (s = 5.2 m), and the branch's radial sleepers clear them only 1.7 m later. Choose the timber count
  in the generator by the widest unsupported run (`_rail_junction.best_timbers`), not by eye."
- **`gates.md` named `crates/hex-render/tests/kit_gate.rs`** (1 min): already fixed on main by e768aee
  (it now names `crates/hex-play/tests/kit_gate.rs`); no change needed.
- **Hook refusals** (2 turns, about 3 min): a heredoc `python3 - <<EOF` and a `for v in …; do blender … $v`
  loop were refused, as SKILL.md "Worktree sessions" says; a `sed -i` line delete was refused by the
  jev shell guard. Writing the loop as a script file and using the Edit tool worked. No skill change:
  the rule is already there; adding "the jev guard also refuses `sed -i` deletes; use Edit" would
  save a turn.
- **`make checkall-ci` failed on fmt-check for a new kit-scene test** (3 min, one gate re-run).
  Proposed skill text (stages.md, before the gate): "rustfmt every `.rs` file you touched
  (`rustfmt --edition 2024 <file>`) before `make checkall-ci`; a fmt-check failure costs a full gate run."
- **Review shots at host load 63** (0 min lost): taken without waiting, as the skill says; every PNG
  read non-black on the first run.
- **No gate change needed for three rail lanes** (0 min): `joint_lanes` takes an untiled centre's
  declared `socket-lane-*` edges (0, 1, 3), and with no rail edge pieces there is no joint comparison;
  `make kit-provenance`'s gate run (48 passed) and `make kit-check` (53 passed) confirm it.
