# Rail tunnel bore: gate findings (2026-11-15)

The full O1 build replaced the stub the same day. Its gate results are in the handback report and
`integration.md`. Findings from the O1 build:

- F1 tripped for real. With the track and lamp iron at masks R = 0, `kit-provenance`'s gate run failed
  `authored_masks_carry_snow_wetness_and_night_channels` for every rail and iron slot of the bore. The
  bore now writes R = 0.02 there (the shader's N.y ramp keeps the trace off the walls). The proposed
  `INDOOR` exemption still stands.
- F4: the 256 px/m texel floor caught the portal's open lining at 254.6 px/m median with three equal
  cells (1.85 m mapped to the 1.8 m panel). Fixed by laying whole panels back from edge 0 at true scale
  with a part panel at the soffit, which keeps every cell on the island's u wrap. Proposed
  `references/gates.md` line: "A lining that must end on a wrap at both ends lays whole islands back from
  the end that has to match and puts the remainder at the other end, instead of stretching equal cells."
- F5: a piece in `GROUND_PROPS` gives every LOD0 triangle to the walker and scatter, so state-dependent
  slots (`_open`, `_back`) need runtime filtering; recorded in `integration.md`.
- F6: the kit renders show both states at once (every slot). The closed-state proof needs its own
  render with the `_open` faces removed (`state_render.py`, the same cameras as
  `gate.render_six_views`; `diff_renders.py` compares it with the approved JPEGs at HEAD), which came out
  byte-identical to the approved JPEGs. Proposed `references/gates.md` line: "A piece with
  state slots proves its default state by rendering the six kit views with the other state's slots
  removed and diffing them against the approved renders."
- F7: the multi-piece EEVEE still (`tunnel_still.py`) imports the same GLB twice. Blender names the
  second import's objects `<name>.001`, so a filter on `name.endswith("_lod0")` or `"_collider"` hid
  every LOD of the second bore and portal and drew only their collider boxes. Proposed
  `references/gates.md` line: "A still that places a piece more than once matches objects and
  materials on `name.split('.')[0]`."

## Gate stub (superseded)

The stub (`scripts/blender/kit/rail_tunnel_bore.py` over `_rail_tunnel_bore.py`) builds the interior
only: the lining with the rail portal's opening, `_rail.track` on the floor and four lamps. It uses
placeholder maps and runs with `HEXW_SKIP_BAKE=1` (the generator refuses a real bake until the owner
rules on `options.md`). The point was to find which gate branches a tile-less prop drawn once per cover
cell trips. The temporary manifest entry was reverted and the stub artifacts were removed afterwards.
`stub-bake-report.json` is the build's bake report.

## Results

| Step | Exit | Tail |
|---|---|---|
| `HEXW_SKIP_BAKE=1 make kit-build PIECE=rail_tunnel_bore` | 0 | `tris per LOD [1224, 582, 116]`, stone and glass median 258.1 px/m, lamp lateral margin 0.695 m, section errors `[]` |
| `make kit-compress PIECE=rail_tunnel_bore` | 0 | `glb 104052 B, ktx bc7 8675 B, astc 8492 B` (emissive BC7_SRGB / ASTC_SRGB, 9 levels) |
| `make kit-check` with the temporary entry | 2 | `71 passed; 2 failed`: `provenance_complete_and_hashes_match` (no provenance) and `provenance_explains_every_slot_lod2_leaves_out` (reads `bake.lod2Drops` from the missing provenance) |
| `make kit-check` after the revert | 0 | `73 passed; 0 failed; 2 ignored` |

Both failures come from the missing provenance. A skip-bake stub cannot have provenance
(`gate_rejects_a_skip_bake_draft`), so neither is a real trip. No other test failed. Reading the gate
code confirms that the main per-piece checks really exercised the bore and did not skip it. Geometry,
budgets and texel density (`check_geometry`), the KTX2 checks (`check_textures`) and the hooks masks
test (`authored_masks_carry_snow_wetness_and_night_channels`) all loop over every manifest piece and
read its GLB, maps and compress report. None of them needs provenance.

## What the stub settles

- Budget: 1,224 / 582 / 116 tris fits the prop kind's 1,500 with LOD1 at 47.5% and LOD2 at 9.5%. The
  brief's 3,000 is not needed for the interior. O1's ridge (an estimated +300) still fits.
- Seams: the lining ring (24 points: 16 arch segments plus the course cuts) is identical at both ends
  and in every LOD. The track ends carry `_rail.section()` exactly, and no lamp vertex touches an end
  plane. That proves the bore-to-bore seam only. The bore-to-portal seam is not met: the portal's
  lining ring changes per LOD (22, 7 and 5 segments), and the extension that has to carry the bore's
  ring at edge 0 is not built (`integration.md`).
- Lamp spacing: 6.93 m (L / 4), not the brief's "about 8 m", because an even count per bore keeps the
  wall alternation correct across joints. L / 3 = 9.24 m would put two lamps on the same wall at each
  joint.
- Clearance: the lamps stand 0.695 m clear of the widest vehicle (train half width 1.48 m), lowest point
  2.0 m. The section is the portal's, so the train clearance is the portal's (engine 0.319 m radial,
  0.34 m at the top).
- Emissive: `kit-compress` encodes a piece emissive map as sRGB and `kit-check` accepts it. It has not
  been seen in the engine.
- LOD2 drops (sleepers, lamp iron) need `bake.lod2Drops` reasons. The generator writes them.

## Friction, with proposed skill text

- F1 (read from the gate code, not tripped): the hooks gate requires snow R > 0 on every masked slot,
  including an indoor lining that snow can never reach. The stub used R = 0.02 from the start, so the
  rule never fired. Proposed: an `INDOOR` list in the hooks gate (beside the glass and
  SUBMERGED exemptions) for bore linings. Proposed `references/gates.md` line: "Interior-only slots
  (tunnel bores, linings) are exempt from the snow R > 0 rule when listed in INDOOR. Until then use
  R = 0.02 and say so in the provenance notes."
- F2: nothing in the pipeline checks that a new tunnel or under-cover piece fits under the cover level.
  The conflict above surfaced only by measuring. Proposed `references/dispatch.md` line: "A piece drawn
  under raised cover (a bore, a culvert, a cellar) gets its section checked against the cover ground
  (one LEVEL_STEP, relief dips to -0.25 m) and the vehicles' envelopes before any modelling."
- F3: the worktree guard hook rejects a command containing the word `source` in a path (`mv ...
  artifacts/source`) as if it ran `source`. A destination folder named `source` needs a different name.
