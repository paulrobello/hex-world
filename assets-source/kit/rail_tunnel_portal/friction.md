# rail_tunnel_portal friction log

1. **Five-course panels overflow the atlas at the road's 2.4 m strip (about 10 min).** The rail face is
   5.57 m tall. Twelve courses with the road's rows (foot band, two panels, top band) make each panel
   five courses (2.32 m) tall. A 2.4 m by 2.32 m panel is 619 x 599 px, and the first build failed with
   `atlas overflow at cop_cfront`. A pure-Python replica of `_tunnel_portal.pack` swept PW 2.4, 2.2, 2.0
   and 1.8 against the coping width. Only PW 1.8 fits (skyline 1013 of 1024). Fix: `tp.PW = 1.8`. The
   wall's stone pattern now repeats every 1.8 m instead of 2.4 m (owner question).
   *Proposed skill text (pitfalls):* "Before changing a shared-island piece's section, replay the
   helper's `pack()` on the new island sizes in plain Python; an overflow costs a full build to find."

2. **LOD budget tight at 1,500 (about 10 min).** The first passing geometry was 1,519 tris at LOD0, and
   LOD1 was over half of LOD0. The fixes were mound fall columns 5 to 4 (LOD0) and 3 to 2 (LOD1), LOD1
   fern `frond_segs` 1, and LOD1 coping 7 to 6 segments after the ring-cut fix (item 4) moved LOD1 to
   740 against a 739.5 cap. Final: 1,491 / 735 / 207.
   *No skill change:* the budget check already names the LOD and the ratio.

3. **The face panels were built at the road's face, 3.8 m behind the rail face (about 20 min).**
   `_tunnel_portal.FaceMap.__init__(self, s_c, h_c, x=X_F)` binds `X_F` when the class is defined, so
   setting `tp.X_F` later moved every other part but not the face cells. Every gate passed: tri counts,
   texel density, the train sweep (the face was beyond the back plane, so the sweep skipped it) and the
   mound checks. From the front the face looked right. Only an oblique EEVEE still showed the turf
   through a recessed face, and a probe of the -x stone faces found them at x 11.656 (the road's
   APOTHEM - 2.2). Fix: `tp.FaceMap.__init__.__defaults__ = (tp.X_F,)`, plus a generator assertion that
   no face-plane stone face lies behind X_F.
   *Proposed skill text (pitfalls):* "When a piece reuses an approved helper by overriding its module
   globals, grep the helper for default arguments and class attributes bound to those globals
   (`def f(..., x=X_F)`). They are frozen at import and silently keep the old value. Look at one
   oblique still before trusting a front view."

4. **The road's strip cut opens wedges beside a tall springing (about 25 min).** The road cuts the face
   cells near the ring along one chord per 0.4 m strip of `keepout(s)`. `keepout` jumps from -inf to
   the springing height at |s| = A + RING. With a 0.6 m springing the wedge that chord leaves is
   hidden. With the rail's 2.4 m springing it was a visible gap beside each jamb, from the ground to
   mid-jamb. LOD1 also had gaps beside the keystone, and LOD2 left the keystone's crown uncovered
   (its 5-division keystone stops 15 cm under the coping). Fix in `_tunnel_portal_rail.panel_cell`: a
   face cell cut by the ring follows the LOD's own ring polylines exactly (voussoir tops, the keystone
   top), with vertical steps at the jambs' outer edges and the keystone's top corners. Clamp crossings
   are inserted. One strip per cell (`LOD[0|1]["strip"] = 100`). A LOD2 crown cap. A ray-cast census
   of the face plane (25 mm grid, every LOD) now finds only the 15 mm voussoir joint slits the road
   also has.
   *Proposed skill text (pitfalls):* "A piece whose silhouette cuts a wall face: cut the face along
   the cutting part's own polyline per LOD, not along a sampled curve per strip, then ray-cast the
   face plane on a grid for holes at every LOD."

5. **The panel's v wrap failed on a fractional-pixel period (about 10 min).** The full bake failed
   `seam_errors` with panel normal v 0.027 (limit 0.02) and AO v 0.064 (limit 0.06). u passed at 0.001.
   `seam_errors` compares the top gutter with the bottom rows one *rounded* period apart. The
   five-course panel is 598.78 px at TEXEL 258, so the comparison slips 0.22 px across the course
   joints (the road's slipped 0.09 px). Fix: `tp.TEXEL = round(PH * TEXEL) / PH` (258.097 px/m, a
   599 px period). The next bake measured normal v 0.00007 and AO v 0.036.
   *Proposed skill text (pitfalls):* "A wrapping island's period should be a whole number of pixels
   (period x TEXEL integral). The seam check rounds it, and a fractional period fails the wrap test
   on joint-heavy edges."

6. **Heredocs refused by the worktree hook (2 min).** `python3 - <<'EOF'` was refused. Scripts go
   through the Write tool into the scratchpad, as SKILL.md "Worktree sessions" says.
   *No skill change:* already documented.

7. **A new grass slot turns `make checkall` red until a runtime list gains it (about 10 min).**
   Landing the manifest entry made `play::grass::tests::grass_slots_are_the_grass_sets_minus_foliage`
   (`crates/hex-play/src/play/grass.rs`) fail: the turf slot is `tileable: grass` and not foliage,
   and the test pins the full list. The fix is a runtime edit (with `NOT_GROUND` and `GROUND_PROPS`
   in `scatter.rs`), so it went into `integration.patch`. The asset branch alone is red until the
   orchestrator applies the patch in the same merge.
   *Proposed skill text (piece tables):* "A slot with `tileable: grass` that is not foliage needs a
   row in the `grass.rs` grass-slot test list, and a ground hero needs `GROUND_PROPS` (and its
   card slots `NOT_GROUND`) in `scatter.rs`. Without the list row `make checkall` fails, so verify
   the integration patch with `checkall` and tell the orchestrator the branch is red without it."
