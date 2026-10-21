# Seam checkpoint: road_straight edge/center (owner-approved 2026-10-13, see approvals.json)

Prepared by the authoring agent (claude-code / claude-opus-5-5) on 2026-10-13. No approval is recorded. The controller presents these files to the owner.

## Files to review
Only the Blender renders are committed. The in-engine shots and crops the owner reviewed were local output and were not kept; regenerate the shots with the commands below (the crops were cut from them by hand).

| What | Path |
|---|---|
| Native seam close-up (1600×1000) and overview | not committed: `make shots-kit` (`kit-seam-closeup`, `kit-overview`) |
| Web seam close-up (1280×800) and overview | not committed: `make web-shots-kit` |
| Center piece, three-quarter render | `assets-source/kit/road_straight_center/renders/three_quarter.jpg` |
| Edge piece, three-quarter render | `assets-source/kit/road_straight_edge/renders/three_quarter.jpg` |
| 4× crops of the joint at x = 13.856, the verge choice (C7), the kerb shadow fix and the shadows-off diagnostic | not committed (crops of the shots above) |

Fix round 1 regenerated every shot and crop. The 1-minute host load was 14.64 before and 13.95 after the native run, and 13.69 before and 13.07 after the web run. Runs whose after-reading exceeded 15 were discarded and rerun.

## What the crops show
The crop boxes come from projecting the joint (x = 13.856) through the `scripts/shots/kit.json` seam camera. The projected dash ends (x = 12.471 and 15.242) land on the dash ends visible in the shot, which confirms the projection.

- **Bed (crown-dash).** The asphalt runs through the joint with no line, step or shading change. Luminance 0.1 m either side of the joint is 51.9 / 48.5 at t = −1.5 and 37.1 / 37.5 at t = +1.5, which is within the texture's own variation. World-space tiling continues across the rotated piece.
- **Dash phase.** The joint sits in the middle of a gap. The dashes end 1.386 m before it and start 1.386 m after it, so the rot-0 and rot-3 edge pieces mirror each other exactly.
- **Kerbs (kerb-near, kerb-verge-far).** The kerb top is continuous, with luminance 171.1-171.2 on all four samples across the joint (near) and 167.7-170.0 (far). There is no gap or step.
- **Verge and meadow contact (verge-meadow-near).** The verge grass and the meadow grass use the same world-space grass set, and the verge slope runs into the cap below it. The contact line is not visible at 4×, and there is no crack.
- **Web against native.** The web crops match the native ones: same texture phase, same dashes, same kerb.
- **Kerb walls are clean.** The first round showed a sawtooth shadow-acne band on the sun-facing kerb walls. It came from the kit's first shadow cascade reaching 120 m, which makes a shadow texel about 0.1 m, close to the 0.12 m kerb height. Fix round 1 ends the kit scenario's first cascade at 25 m (`lookdev::KIT_FIRST_CASCADE_FAR_BOUND`) and keeps Bevy's default bias (depth 0.02, normal 1.8). Luminance along the wall now varies by a standard deviation of 0.46, against 3.36 before. The kerb also casts a visible 3 cm contact shadow on the asphalt, attached to its base; before the change it cast none. Raising the depth bias to 0.2 at 120 m also removed the band, but it detached that contact shadow, so I rejected it. The P0 scenarios keep the 120 m cascade.

## Profile and future bends (C1)
`scripts/blender/kit/_road.py` holds one cross-section, `PROFILE`, as (t, z) points. It is identical in every LOD and every piece, and mirror-symmetric about t = 0:

(−4.6, −0.30), (−4.0, 0.06), (−3.25, 0.06), (−3.25, 0.20), (−3.0, 0.20), (−3.0, 0.08), (−1.5, 0.10), (0, 0.11), (1.5, 0.10), (3.0, 0.08), (3.0, 0.20), (3.25, 0.20), (3.25, 0.06), (4.0, 0.06), (4.6, −0.30)

- t is measured from `hexgeo.lane_frame(edge, 0.0, 6.0)["center"]` along the lane tangent. `lane_frame(k)` is `lane_frame(0)` rotated by k (pinned by `test_hexgeo`).
- A future `road_bend_60` sweeps the same `PROFILE` along its arc, from the lane frame of one edge to the lane frame of another. Its section at either edge midpoint is therefore this section, and it meets a straight edge piece or another bend at any edge without a new profile.
- The kit gate's `check_edge_profile` checks the 15 points at x = APOTHEM in LOD0, LOD1 and LOD2. Each LOD's points match exactly, and the profile is mirror-symmetric.
- The dash period P = 2·APOTHEM/5 puts a gap centred on every edge midpoint of a straight road. A bend cannot reuse P unchanged, because its centre line between two edge midpoints is not a multiple of P long. The rule, recorded in `_road.py`: a bend whose centre-line arc between the two edge midpoints is L long uses n = round(L/P) dashes at period L/n, centred at (k + 0.5)·L/n from either end. Each end then finishes with half a gap, so the gap stays centred on both edge midpoints and the dashes continue into any straight or bent neighbour. A straight edge-to-edge run (L = 2·APOTHEM, n = 5) gives back P exactly.
- The kit gate also checks that the centre piece's sections at x = +6 and x = −6 equal the edge piece's section at x = 6 (mirrored for −6) in every LOD (`check_center_edge_joint`).
