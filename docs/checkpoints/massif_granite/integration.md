# massif_granite: integration notes (Task 9 Step 3)

Status: landed on `mega-tiles` (manifest entry, generator, walker, scatter, grass, boids and
thumbnail rows all in), pending the owner's in-engine approval. The owner answered A54 on
2026-10-29: Q1 O1 (rock and scree take a unique bake, and the kit gate's texel floor is scoped to
massif_granite at 20 px/m) and Q2 O1 (the lower-slope pines are a runtime scatter rule, never
baked). It answered A55 the same day: Q1 O1 (taller grassy flanks: form pass 2, then form pass 3
below, which the orchestrator's ruling of 2026-10-30 redirected to raising the grass alone), Q2 O1
and Q3 O1 (pine density and rock colour kept). Review sheet: `form-pass-3-sheet.jpg` (script
`form_pass2_sheet.py` with argument 3; pass 2's is `form-pass-2-sheet.jpg`, the form pass's
`review-sheet.jpg`, script `review_sheet.py`), stills from
`review_render.py`, numbers from `form_measure.py`, `silhouette_iou.py` and `rock_colour.py`,
reference cameras in `fit/`, measured values in `form/`.

## Form pass 3: the grass alone rises (2026-10-30)

Pass 2 (below) missed A55 Q1 O1's intent: at the play camera it read like the form pass, the
outer ring was still a flat green band, and its 39.66 m top broke A53's top of about 36 m. The
orchestrator's ruling: "the rock itself stays" means the rock's shape and colour, not how far it
stands out of the grass. So pass 3 leaves the rock exactly as the form pass built it (`slope_h`,
`mass`, the slab seeds, the volume's ceiling, the moss band and the atlas column all back to
`1ed817f`) and raises only the grass, which buries the rock's lower part: a rock emerging from a
grassy hill, as ref-2 reads. Review sheet: `form-pass-3-sheet.jpg` (`form_pass2_sheet.py ... 3`:
ref-2, pass 3 and the original form pass at ref-2's fitted camera, the play and low cameras,
top-down, and the overlay against ref-2). Numbers: `form/form-measure.json` (pass 3),
`form/original-form-measure.json` (the form pass, re-measured), `form/pass-2-form-measure.json`,
the IoU files `form/iou*.json`, `form/original-iou*.json` and `form/pass-2-iou*.json`, and
`form/colour.json`.

- **The flank** (`_massif.flank`, `grass_h`): a function of the distance to the flower outline,
  with the outline's creases rounded by a 3 m polynomial smooth min of the 18 edge distances
  (`soft_outline_dist`), so no hip ridge runs in from the outline's corners. It is
  7.5 smoothstep(0, 15 m) + 2.5 smoothstep(0, 30 m) metres: 0 with no slope at every outer edge,
  easing in over the first 2-3 m, about 7.5 m by 15 m in and still rising toward the summit. It
  joins the form pass's slopes (faded at the rim as before) in a smooth max whose reach fades out
  at the rim. The rock's foot is where its fixed surface meets that ground. The fans' apexes are
  taken against the new ground, so the scree still starts at the rock.
- **Lattice**: the spacing classes come from the ground less the flank, whose own points take the
  gentle class (1.8 m at LOD0, 2.8 m at LOD1; at LOD2 the mid class's 4.2 m on the outer 6 m and
  8.4 m inside, to keep LOD2 under 15% of LOD0). The fans add points along their sides, where the
  lens thins out over the coarser lattice (a 12 cm chord dip at 60 deg without them).
- **Prop anchors** stay 6 m in from each ring cell's outer edge, on the ground, now 4.44 m up (pass
  2's assert that the ground there is flat is dropped; the kit gate needs only their `extras.kind`).
- **form_measure.py** now samples the profile with a ray at each exact point along the bearing
  (the 0.25 m map's nearest cell read up to 13 cm of false drops on the steeper flank; re-measured
  that way, the form pass has none) and reports `rimBand`. Pass 2's numbers below are its own
  map-sampled ones.

| Target | Original (form pass) | Pass 3 |
|---|---|---|
| Rock foot, where the rock meets grass (36 bearings from the centre) | median 4.24 m, 0.76-7.62, 2 of 36 at 7 m or more | median 9.34 m, 7.44-10.72 (IQR 8.9-10.1); 36 of 36 over 7 m, 24 in 8-10 m; lowest at the spine tips (210 deg 7.44 m, 30 deg 8.22 m) |
| Ring ground at tile level (under 0.10 m) | out to 13.7 m from the rim, 54% of the ring's ground | out to 1.56 m (p99 1.22 m), 5.9% of the ring's ground |
| Ground height by radius (min-max, median) | r 20: 1.8-3.8 (2.7); r 25: 0.4-2.4 (1.0); r 30: -0.1-0.4 (0.04) | r 20: 7.1-9.4 (8.1); r 25: 3.1-8.6 (5.7); r 30: 0.2-7.0 (3.3) |
| Profile monotonic, rim inward, outside the rock | 36 of 36 within 5 cm | 35 of 36 (110 deg drops 6.6 cm 0.25 m from the rock, where a cap face passes under it) |
| Top | 34.63 m | 34.57 m (the same rock; the LOD0 decimation differs by 6 cm) |
| Silhouette IoU vs ref-2 (best of six bearings) | 0.875 | 0.877 |
| Silhouette IoU vs ref-4 (best of six) | 0.814 (above the rim 0.785) | 0.831 (above the rim 0.795) |
| Rock coverage (top-down) | 934 m², 20.1% | 544 m², 11.7% (ref-2's estimate: 15.5%) |
| Spines | 6, reach 18.6-27.4 m | 6 (30/75/135/215/265/335 deg), reach 14.3-20.3 m |
| Steepest grass or scree triangle (gradient) | 0.93 (43 deg) | 1.075 (47 deg), the baked LOD0 ground, from `walker_climbs_the_massif_flanks_to_the_rock_foot` |
| LOD tris | 11,843 / 5,208 / 1,441 | 11,468 / 5,019 / 1,594 |

The buried lower rock is the cost of the ruling: the rock's plan shrinks from 20.1% of the
footprint to 11.7% (ref-2 estimates 15.5%), the spines end 14-20 m out instead of 19-27 m, and
the visible rock surface is 2,611 m² (3,781 m²). A lower flank trades the two: at a 7 m foot the
model gives about 13.5%.

The rock's colour at ref-2's camera is the form pass's (`form/colour.json`: luma mean 0.582 against
0.577, lichen green 0.074 against 0.072; pass 2's is `form/pass-2-colour.json`). The baked build
measures the same as the geometry-only build the numbers above came from (same tris, top, foot and
IoU).

Runtime for pass 3: `MASSIF_PEAK` is 34.57 m and `MASSIF_ROCK_FOOT` is `walk_parts.py`'s output
for the pass 3 measure (20 parts, 543 m², reaching 20.3 m). The new test
`walker_climbs_the_massif_flanks_to_the_rock_foot` checks that the steepest grass or scree triangle
on the baked LOD0 ground is a gradient of 1.075 (47 deg), under its 1.5 bound and the walker's
limit of 4 (`MAX_STEP` 0.4 m per 0.1 m `SUBSTEP`), and walks in from 0.6 m inside the rim on twelve
bearings (each ring cell's middle and each notch between two ring cells). No step is refused, and
each walk stops at the rock's foot more than 7 m up. `walker_walks_around_a_massif` now aims a
little north of the centre and measures the slide from the first contact with the foot (over 2 m;
it slides 4.0 m). The spined foot is notched between the spines, so a walk aimed south, as the old
test did, wedges in a notch after 0.2 m. That is correct, but it says nothing about sliding. The
pine, ring-grass and ring-density tests pass unchanged.

## Form pass 2: the raised flanks (2026-10-29), superseded by pass 3

The owner answered A55 on 2026-10-29: Q1 O1, taller grassy flanks rising about 8-10 m from the
rim to the rock foot, as in ref-2, so the mountain fills the footprint at the play camera; the rock
stays, and so do the pine density (Q2 O1) and the rock colour (Q3 O1). Review sheet:
`form-pass-2-sheet.jpg` (script `form_pass2_sheet.py`; ref-2, this pass and the previous pass at
ref-2's fitted camera, the play and low cameras, top-down, and the silhouette overlay against
ref-2). Numbers (renamed in pass 3): `form/pass-2-form-measure.json` (the previous pass's is
`form/original-form-measure.json`, re-measured in pass 3; the column pass's is in git history),
`form/pass-2-iou.json` (ref-4), `form/pass-2-iou-ref2.json` and `form/original-iou-ref2.json`;
pass 2's `form/colour.json` is in git history.

- **The lift** (`_massif.lift`): the whole landform, ground and rock mass alike, stands on 5 m
  within 15 m of the summit, falling smoothly (smoothstep) to 0 by 35 m. `slope_h` and every top
  in `mass` add it, so the mass is the previous pass's plus the lift and the rock keeps its plan:
  coverage, foot radius and spines are unchanged to within the decimation's noise. The rim's fade
  band (2.5-7 m in from the outline) still brings the ground to exactly z = 0 on all 18 edges.
- **Build changes it needed**: the volume's ceiling `VZ1` 38 -> 46 m (the top is 39.66 m); the
  LOD0/LOD1 ground spacing classes are taken from the ground less the lift outside the fade band
  (the lift is smooth enough for the flat-ring spacing), so the cap keeps its triangle count; the
  atlas's left column narrows from 0.25 to 0.20 so the LOD0 rock and scree stay above the 20 px/m
  floor (the rock's surface grew 2%); the low-face moss band moves up by the lift; and
  `export_clean` collapses any vertex the glTF exporter writes with a zero tangent (one fold
  vertex on LOD0's rock that Blender's own tangents passed) and exports again.

| Target | Previous pass | Form pass 2 |
|---|---|---|
| Rock foot height above tile level (36 bearings from the centre) | median 4.25 m (0.76-7.62, IQR 3.2-5.4) | median 9.01 m (2.32-12.57, IQR 7.4-10.6); 11 of 36 in 8-10 m; the low ones are the spine tips (25-27 m out at 30, 120-140 deg) |
| Ground height by radius (min-max, median) | r 15: 4.4-6.0 (5.0); r 20: 1.8-3.8 (2.7); r 25: 0.4-2.3 (1.0); r 30: -0.15-0.35 | r 15: 9.4-11.0 (9.9); r 20: 5.5-8.5 (6.7); r 25: 2.1-5.6 (3.3); r 30: 0.0-1.65 (0.56) |
| Profile monotonic, rim inward | 35 of 36 within 5 cm (worst 6.6 cm at 200 deg) | 34 of 36 within 5 cm (5.5 cm at 60 deg, 5.9 cm at 200 deg, both the scree lens); steepest at the foot on 33 |
| Outer rim | flat at z = 0, meadow cap profile | unchanged (border points forced to 0, normals +Z); every kit seam and edge gate passes |
| Rock coverage | 934 m², 20.07% | 939 m², 20.17% |
| Spines | 6 (30/80/135/215/265/355 deg) | 6 (30/75/135/215/265/350 deg), reach 18.6-27.4 m |
| Top | 34.63 m | 39.66 m |
| Silhouette IoU vs ref-2 (best of six bearings) | 0.875 | 0.843 |
| Silhouette IoU vs ref-4 (best of six cell bearings) | 0.814 (above the rim 0.785) | 0.794 (above the rim 0.678) |
| Rock AO (area-weighted at face centres) | 0.82 (1.6% under 0.1) | 0.82 (1.7% under 0.1) |
| LOD tris | 11,843 / 5,208 / 1,441 | 11,914 / 5,256 / 1,526 |
| LOD0 texel (rock / scree, px/m) | 20.44 / 20.44 | 20.80 / 20.81 |

The IoU fell against both references. The overlays show the loss at the summit, which now stands
5 m higher, above both drawings (ref-2's height is poorly conditioned at its fitted camera; ref-4 draws
about 32 m). Keeping the rock rigid (A55: "the rock itself stays") and the brief's "the top may
rise by roughly the flank lift" both put the top at 39.7 m; holding the old 34.6 m top would mean
shortening the rock by about 5 m. Colour: the shader is unchanged apart from the moss band, and the
baked rock albedo luma mean went 0.524 -> 0.510. `rock_colour.py`'s fixed crop at ref-2's camera
is not like for like any more, since the rock stands higher in the frame and the crop now holds
more of its lower, mossier faces: luma mean 0.55 (was 0.58), spread 0.14 (0.13), mustard 7.0%
(6.1%), lichen green 14.9% (7.2%), dark 7.1% (4.2%), moss 3.2% (0.1%).

A variant tried and rejected: the lift tapered against the rim (full 14 m in from the outline, 0
at the 2.5 m flat band) instead of the summit distance. It raised the outer ground (r 30 m median
0.56 -> 1.62 m, r 35 m 0 -> 0.55 m) but left the spine-tip feet low (minimum 2.6 m), moved the
IoU by under 0.01 (ref-2 0.842, ref-4 0.797), broke monotonicity on 7 bearings (worst 10.4 cm,
since the outline distance is not monotonic along rays from the centre) and exposed a loose rock
fragment on the east ring cell beyond a spine tip.

Runtime: `MASSIF_PEAK` 34.63 -> 39.66 (`massif_peak_is_the_lod0_top`; the boids' band, the pick
planes and the thumbnail frame read it, and `PEAK_REACH`/`PEAK_FALLOFF` are plan reaches, unchanged);
`MASSIF_ROCK_FOOT` regenerated by `walk_parts.py` over the new `form-measure.json` (26 convex parts,
988 m²). Grass blades and ring scatter already took their height from each ring cell's piece mesh
(`scatter::member_tris`); new tests check it: `massif_ring_blades_sit_on_the_piece_ground` (every
97th ring blade within 1 cm of the LOD0 mesh's top, over a fifth of them more than 2 m up) and
`massif_pines_stand_on_the_lower_slopes` (each pine on its cell's ground, within `stand`'s 0.3 m
low ring, most of them over 2 m up). The picker thumbnails and the in-engine shots are stale until
the orchestrator re-runs `make thumbs` and `make shots-massif`.

## The form pass (2026-10-29)

The column-rock pass read as a tight cluster of basalt-like prisms on an almost flat dome. This
pass builds the big mass first and carves the slabs second:

- **Mass** (`mass`): a summit block (q = 1.15 shoulders, a rounded top) with three lower tops and
  six ridge spines from the summit down into the ring cells. Clefts along the six gully bearings
  cut the tops toward the grass from about 3 m off the summit, so the buttresses stand apart over
  scree-filled clefts.
- **Slabs** (`rock_volume`): the mass becomes a signed-distance volume (0.25 m voxels) between its
  top and 3 m under the grass. 74 slab facets are carved into its outer layer. Each is a plane at a
  seed on the mass, its cell stretched 2.8 times along the fall line, tilted up to 7 degrees and
  lifted up to 0.8 m. Joints 0.6 m deep run along the cell borders. Each voxel takes the cell of
  its foot on the mass surface, so a cell is a column through the outer layer. OpenVDB (Blender's bundled `openvdb` module) meshes the volume. Seven
  smoothing passes round the edges, and triplanar grain displaces the surface.
- **Ground**: the grass slopes rise from the flat ring to the rock foot, concave (steepest at the
  foot), monotonic along every ray from the summit, with grass ribs below the spines. The scree
  lens thins monotonically outward from each cleft. The flower outline stays flat at y = 0.
- **LODs**: LOD0 is the high poly decimated (its buried faces cut first). LOD1 decimates LOD0 and
  keeps its UVs. LOD2 is the volume meshed at 1 m and decimated, because decimating LOD0 (its UV
  seams and open rim) stalled at about 830 tris with a 7-9 m error. Degenerate faces and
  zero-tangent vertices are collapsed into a neighbour, not cut, so the shell stays closed.
- **UVs**: the islands are cut by smart project on a copy smoothed five times, and the real rock is
  unwrapped angle-based along those seams. Smart project on the real rock filled 19% of its block.
  Taking the smoothed copy's UVs folded triangles, whose zero tangents cost about 65 holes. The
  bake's 8 px gutter is now kept in the maps, because a flat fill there drew a dark line along
  every seam.
- **Colour and AO**: the lichen comes in patches, with mustard lichen, black lichen spots, darker
  joints, and dark moss in the clefts and in patches on the faces below about 12 m. The AO distance is 1.2 m (it was 2.5 m; the boulders use 0.5 m), so it darkens
  the joints and cleft walls, not whole clefts.

### Measured targets

The reference estimates come from cameras fitted to each image's outline corners and skirt
bottoms (`fit/cam_fit.py`: square pixels, principal point at the centre). ref-2's camera sits at
27.9 degrees elevation and 183 m (17 px mean residual over 20 points). It is not the 65 degree play
camera. ref-4's camera sits at 13.4 degrees and 105 m (25 px residual). Both images are AI
generated, so their outlines are only roughly a true flower. The rock foot was traced in ref-2
(30 points, `fit/ref2.json`) and back-projected onto the ground (`fit/ref_fit.py`,
`form/ref2-foot-estimate.json`). The back of ref-2's rock is hidden, so the traced front span
(281-306 degrees) is scaled to 360. ref-2's exposed rock covers only about 17% of the footprint.
Its look of a mountain filling the footprint comes from the grass slopes (target 3). The foot is
raised (4.5-6.4 m at ref-4's camera, median 4.3 m here), so the z = 3-6 m estimates are the fair
basis: this pass is +16% against z = 3 m and +3% against z = 6 m (it would be +30% against the
homography-only z = 0 m figure, which is robust to camera error but biased low).

| Target | ref-2 / ref-4 estimate | Form pass | Column pass |
|---|---|---|---|
| 1. Rock coverage (top-down, of 4,656 m²) | 805 m², 17.3% (foot at z = 3 m; 720 m² / 15.5% at z = 0, 906 m² / 19.5% at z = 6) | 934 m², 20.1% (+16%); scree 355 m² more | 359 m², 7.7% |
| 1. Rock foot radius from the centre (median over 72 bearings) | 16.5 m (16.6 at z = 0, 17.0 at z = 6) | 16.6 m (mean 17.2, max 27.4) | 10.8 m (max 16.3) |
| 2. Ridge spines (foot-radius peaks 4 m above their flanks) | about 5 visible, tips 17-27 m | 6, at 30/80/135/215/265/355 deg reaching 24.9/18.6/27.4/23.6/22.4/18.6 m, all into the ring cells (> 13.86 m) | 4, 14-16 m |
| 3. Ground profile, rim to rock foot (36 bearings from the centre) | slope rising to a foot about 4.5-6.4 m up (ref-4's camera) | 35 bearings rise monotonically within 5 cm; one 6.6 cm dip at 200 deg (the scree lens seen from the centre, not the summit); steepest at the foot on 36 of 36; foot 0.8-7.6 m up (median 4.3) | 9 bearings with dips up to 0.34 m; steepest at the foot on 26 |
| 4. Top height | drawn about 32 m (ref-4), 24-29 m (ref-2) | 34.63 m | 37.9 m |
| 5. Silhouette IoU at ref-4's camera (best of the six cell bearings) | - | 0.81 whole piece, 0.79 above the back rim (0.80-0.81 and 0.72-0.79 over the six) | - |
| 6. Rock AO (area-weighted at face centres) | boulders about 0.9 | 0.82 (1.6% of the area under 0.1) | 0.59 |

Target 5's largest mismatches (`silhouette-overlay.png` in the sheet): the summit stands about 21
rows (at 896 rows) above ref-4's, since this pass is 34.6 m against ref-4's drawn 32 m; ref-4's
ring outline is wider on both flanks than any true flower (its back cells are drawn wide); and the
left flank of ref-4's rock comes lower and wider at the rim.

Colour beside ref-2 (`rock_colour.py`, the front rock face at ref-2's camera, `form/colour.json`):
luma mean 0.58 against 0.52, spread 0.13 against 0.24, mustard 6.1% against 5.6%, lichen green
7.2% against 2.7%, dark 4.2% against 21%, moss 0.1% against 0.7% (the moss sits lower, on the
faces the cliff-foot still shows). ref-2's dark share comes mostly from its side light and cleft
shadow; the stills use the kit sun. The colour reaches ref-2's lichen and mustard, not its
contrast.

## Owner questions (through the orchestrator)

- **Q1, peak height.** The approved refs as drawn measure about 32 m (ref-4 at its fitted camera)
  and 24-29 m (ref-2, poorly conditioned in height); A53 Q2 and the brief say about 36 m. This
  pass tops out at 34.63 m. O1 keep 34.6 m; O2 raise it to about 36 m (the silhouette IoU against
  ref-4 falls); O3 lower it toward 32 m (closer to both drawings). Evidence: the silhouette overlay
  on the sheet, where the blue summit is ours standing above ref-4's.
- **Q2, rock contrast.** The rock's luma spread is 0.13 against ref-2's 0.24, and its dark share
  4% against 21%. Most of ref-2's dark comes from its side light, which the kit sun does not give.
  O1 judge it from the orchestrator's in-engine shot before any shader change; O2 darken the
  joints, moss and weathering streaks further now.

## The patch

Landed: `mega-tiles` merged the asset branch (11437e3) and applied this patch (165f491); the
section below records what it carried. Its `walk.rs`, `play.rs` and `scatter.rs` hunks were
diffed against `mega-tiles` 4300fb8. It carried the O1 landing:
- `scripts/blender/kit/massif_granite.py`, the thin generator (`_massif.build`), now tracked with
  its manifest entry.
- The manifest entry: pack `hero`, kind `hero`, slots `massif_granite_cap` (grass, masks),
  `_rock` and `_scree` (unique, masks), `_skirt` (soil, tint 2.5).
- `kit_gate.rs`: un-ignores `footprint_base_border_is_the_flower_outline` and adds
  `MASSIF_TEXEL_MIN_PX_PER_M = 20.0` to `texel_min` (owner A54 Q1 O1).
- `tests/hooks/mod.rs`: `massif_granite_skirt` in `MASKS_EXEMPT` (the O4 skirt, as meadow's).
- `assets-source/kit/massif_granite/approvals.json`, a `pending` piece entry, and the friction
  log moved there from this folder.
- `walk.rs`: `FOOTPRINT_OBSTACLES` becomes (tile, part, convex parts) and the massif's row is
  `MASSIF_ROCK_FOOT`, the rock's measured foot as 28 convex fans from the anchor (982 m², from
  `walk_parts.py` over `form/form-measure.json`), in place of the 1.6 x `HEX_RADIUS` hexagon.
  `walker_walks_around_a_massif` now checks the walker never overlaps any part and slides along
  the foot (toward -z, where the foot recedes west of the anchor).
- `play.rs`: `MASSIF_PEAK` 36.0 -> 34.63 (the boids' peak, the pick planes and the thumbnail
  frame read it). On landing, the placeholder mesh and `MASSIF_PEAK_RADIUS` were removed
  (Task 9 Step 5).
- `scatter.rs`: `excluded_slot` takes `massif_granite_rock` and `massif_granite_scree` by exact
  name. No `NOT_GROUND` row (the rock and scree stay walker ground) and no `GROUND_BOXES` row
  (that table is for rectangular building colliders).

Before it landed, on a scratch `mega-tiles` worktree with the three code files patched, `cargo test -p hex-play
--lib -- walk:: scatter:: boids:: play::` passed 241 (`walker_walks_around_a_massif` and
`no_runtime_scatter_on_a_massif_yet`, since removed when the ring scatter landed, among them) and `cargo clippy -p hex-play --all-targets --
-D warnings` was clean.

Verified on the form-pass GLB with the patch applied temporarily, plus the KTX2, GLB and compress
report from a hand-run `kit-compress.mjs piece massif_granite --class hero --stage --out` into
scratch: `cargo test -p hex-play --test kit_gate` passed 72 and failed only
`provenance_complete_and_hashes_match` (no provenance yet). The footprint test, the geometry
budgets at the 20 px/m floor, the texel, hooks, KTX2 and LOD-slot tests and the orphan check
passed. The landing then ran `make kit-build PIECE=massif_granite`, `make kit-compress PIECE=massif_granite`,
`make kit-determinism PIECE=massif_granite`,
`make kit-provenance PIECE=massif_granite REFS=assets-source/refs/massif_granite`,
`make kit-check` and `make checkall`.

## Runtime items

| Item | Status | Detail |
|---|---|---|
| Walker collider (`walk.rs`) | landed | No `OBSTACLES` row: it takes the collider's hull, and the hull of the flower covers the six notches of the outer neighbours. `FOOTPRINT_OBSTACLES`' row is the rock's measured foot (`MASSIF_ROCK_FOOT`, 20 convex parts, 543 m², reaching 20.3 m, just into every ring cell; form pass 3); the grass slopes and scree fans are walkable. Each ring cell stands on its own member's ground (`walk::TileSurface`), and ring scatter colliders are keyed by the ring cell under them. `prop_kind`: not applicable |
| Scatter (`scatter.rs`) | landed (slots and ring rules) | `excluded_slot` keeps scatter off `massif_granite_rock` and `massif_granite_scree` (exact names; a `_rock` suffix would catch other pieces' slots). No `NOT_GROUND` row, which would also drop them from the walker's ground. `FOOTPRINTS` and `GROUND_BOXES`: not applicable (a base, not a prop). Ring scatter per ring cell follows the Meadow rules, and Task 9 Step 5's `ring_scatter_matches_meadow_density` checks it. Pines (A54 Q2 O1): a spruce or pine rule (`tree_forest`, `sapling_spruce`) on the cap slot between about 17 m and 28 m from the centre (the grass slopes below the rock foot), in small groups, off the scree. Landed in Task 9 Step 5: the per-ring-cell scatter scaled to a meadow's density per m² of free ground, `scatter::PINE_BAND` and the two drift rules in `data/tiles/massif_granite.ron`. A neighbouring field's wall on a ring cell's outer edge clips the ring scatter and grass: the anchor's fence-map entry gathers every cell's walls (`boundaries::local_segments_from`) |
| Ring grass | handled, no row | `grass::grass_slots` takes `massif_granite_cap` (tileable `grass`, not foliage), and `grass::style`'s default is meadow's. Blades grow on the ring and the grass slopes. The rock is the topmost surface wherever it covers the ground. The build is seven units, one a cell (`grass::build_units`), spread over frames under the per-frame budget |
| Boids peak height | landed | `MASSIF_PEAK` 34.57 m, the LOD0 top (form pass 3; 39.66 m in pass 2, 34.63 m in the form pass) |
| `kit_scene.rs` showcase, `tile_counts` | done (Task 9 Step 5) | One placement at anchor (-3, -2), rotation 0, with its six ring cells and every cell beside them left free; `tile_counts` counts placement cells, so +1 (39 to 40) |
| Car height | not applicable | no roads |
| Sim nodes | not applicable | none |
| Picker thumbnail frame | done (T10a, `thumbs::camera_for`) | the footprint pose backs off to 320 m with the single-tile pitch and field of view; plan extent x ±41.57 m, z ±40.0 m about the anchor; 34.57 m high since form pass 3 (the thumbnails need a `make thumbs` re-run), skirt 4 m below |
| Draw budget | measured | 4 slots in every LOD (cap, rock, scree, skirt): 4 draws per pass per massif, 8 in the 5 m dither band; meadow_base draws 2 |
| Sound | the orchestrator's | under the standing audio approval |
| Task 7 placeholder | done (Task 9 Step 5) | The cone (`massif_placeholder_mesh`, `MASSIF_PEAK_RADIUS`, `SlopeMeshes::massif_peak`) is removed. The per-cell meadow prisms stay as the fallback while the hero pack streams |
| `data/pending-pieces.ron` | done (Task 9 Step 5) | `massif_granite` removed |
| 16 sampled textures | precedent, not measured | the piece mixes unique, tileable and masks slots in one material, as `road_tunnel_portal` does; no web-smoke run |

## Piece facts

- Kind `hero` (LOD0 12,000 ceiling, hero KTX2 class, streamed `hero` pack). Scripted route, so no
  `.blend` in the store. Pivot: the centre cell's centre. Six `anchor-prop-<n>` sockets, one per
  ring cell, 6 m in from its outer edge, on the ground (4.44 m up since form pass 3). GLB
  determinism: identical across two fresh Blender runs for the form pass (`make kit-determinism`,
  twice, sha256 2c39c773...). Pass 3 failed the check on 2026-10-31: two fresh runs differed in
  one LOD0 rock TANGENT x, -0.7455 against -0.7456 (GLB element 3665 of the rock primitive, 2
  bytes of the 935,760-byte stage GLB). Fixed on 2026-11-13. The cause was Blender 5.2.2's
  MikkTSpace (`Mesh.calc_tangents`, which the glTF exporter calls). It runs threaded on the joined
  LOD0 body (11,468 tris), and its float sums follow the thread schedule: 40 calls on one mesh
  in one process gave 14 different tangent arrays, up to 1.8e-7 apart on about a third of the
  corners. The 7,148-tri rock alone, LOD1 and LOD2 never varied. The exporter rounds tangents to
  4 decimals in float32, and that x (-0.74554998) lands on the tie -7455.5, so the schedule chose
  the digit. A stage-by-stage hash of every mesh across two fresh runs found positions,
  topology, edges, every attribute (`custom_normal` and `UVMap` among them) and the corner
  normals byte-identical at every stage. Only the full-precision tangents differed, from the
  LOD0 join on. So OpenVDB, decimate, the collapses and the loose-face drop the
  `drop_degenerate` docstring blamed were not involved. Fix: `_massif.export_single_thread` saves
  the scene and exports it in a child Blender run with `-t 1` (2026-11-16: that mechanism moved
  into the shared exporter — `export.export_glb(..., threads=1)` re-runs the export single-threaded.
  `kit-build` and `kit-determinism` select it for every piece through `HEXW_EXPORT_THREADS=1`,
  while callers outside that build path retain in-process export by default. The piece-local copies are gone —
  so the determinism here is now general, not a massif special case). The bake keeps every thread. With
  one thread, three fresh processes gave the same full-precision LOD0 tangents (40 of 40 calls
  each). The fix removes the cause, not just this flip: with the sums in a fixed order, the
  tangents depend on the mesh alone, and the mesh was already deterministic. `make
  kit-determinism` passed three runs in a row (sha256 04e459f6...), and a mutation adding 1e-4
  to that tangent in run b failed it. The new GLB is byte-identical to b06ac09's outcome-b
  build, which is also what the threaded export gives on a call without the flip. The baked and
  skip-bake stage GLBs are identical, and so are the four PNGs and the bake report. The runtime
  GLB that C3 approved held outcome a, so exactly one of its components changes: that rock
  TANGENT x, -0.7455 to -0.7456 (9.996e-5). The KTX2 files and the six-view renders are unchanged.
- Tris (form pass 3): LOD0 11,468 (rock 7,148, cap 3,145, scree 599, skirt 576), LOD1 5,019
  (43.8%), LOD2 1,594 (13.9%). The LOD floor scales with the outline, not the cell count: the ring
  is 18 outer edges x 16 = 288 points (no interior edges), so LOD2's fixed cost is 286 cap + 576
  skirt = 862 tris, and LOD0 must be at least 5,747 at 15%. (Form pass: 11,843 / 5,208 / 1,441;
  pass 2: 11,914 / 5,256 / 1,526.)
- Rock high poly: 109,722 vertices, 219,168 tris (the grass buries more of the volume than in
  pass 2's 402,004). Rock LOD error (distance from each LOD's vertices to the high poly's surface,
  max / p99, and back from the high poly's visible vertices): LOD0 0.17 / 0.08 m (back 1.15 /
  0.14); LOD1 0.27 / 0.16 m (back 1.66 / 0.37); LOD2 0.58 / 0.44 m (back 2.88 / 1.36). Pass 2's
  regressions (LOD1 1.41 m, LOD2 back 10.07 m) are gone: LOD1 is under the form pass's 0.36 m and
  LOD2's back maximum is near its 2.61 m. Ground: worst height error 0.32 m (LOD0), 0.60 m (LOD1),
  1.90 m (LOD2).
- Maps: albedo 2048², normal 2048², ORM 512², masks 1024². KTX2 hero class: BC7 6,089,370 B, ASTC
  6,166,759 B (budget 11,500,000); GLB 583,532 B. The rock bake's cage is 0.15 m and its rays
  0.3 m (normal misses 0.03%). The scree LODs bake at 1.0 m / 2.0 m and rock LOD2 at
  0.9 m / 2.2 m. AO distance 1.2 m. The atlas's left column is 0.25 wide again (the form pass's;
  pass 2 narrowed it to 0.20).
- Texel density (per-face median, the gate's measure): LOD0 rock 24.78 px/m and scree 24.77 px/m,
  24% over the 20 px/m floor (less visible rock shares the same atlas). LOD1's rock shares LOD0's
  islands at 23.8 px/m; LOD1's scree and LOD2 bake onto their own left-column blocks (scree 21.8,
  LOD2 7.8 px/m, drawn from 150 m and 400 m). The rock carries 2,611 m² of unique surface (3,852 m²
  in pass 2, 3,781 m² in the form pass).
- AO: visible rock (area-weighted at face centres) mean 0.80, 1.4% of the area under 0.1; rock
  texels mean 0.81, p05 0.28. In engine it darkens only ambient light. Ground masks measure
  2.77 px/m against meadow's 32. Meadow's AO is flat (mean 0.9998), and the ring's AO has mean
  0.984 and p05 0.97 (lower only at the rock foot).
