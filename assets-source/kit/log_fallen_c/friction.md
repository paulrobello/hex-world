# log_fallen_c friction

Third log_fallen variant on `kit/_log.py`, driven from the generator without editing `_log.py`.

- **`_log`'s atlas caps the radius at 0.265 m** (design, 2 min). The two half-length body islands
  are laid side by side at 2 pi r0 x 1.06 each, plus three gaps, in a 3.71 m atlas (1024 px at
  276 px/m), so a thicker log does not fit. The variant reads apart through its plan crook, its moss
  and its open mouth, not its girth (r0 0.26). No skill change: piece-local; noted in the generator.
- **A plan bend with slope at the tip pushed the rim off its planar island** (3 min, one build). With
  `bend` frequency 2.2 the axis had its steepest slope at x = half_l, so the rim ring drifted
  sideways past the rim's square island (the per-face UV assert caught it). Fix: one C-shaped crook,
  frequency pi / (2 half_l), slope 0 at both ends. Skill text (pitfalls, next to the lathed-prop
  rule): "A family helper that centres its end-cap islands on the axis at the ends assumes the axis
  is straight there: give a plan bend zero slope at both ends (sin(pi x / 2 half_l)), or widen the
  cap islands."
- **LOD2 has no hollow wall, so its floor is the mouth ring** (3 min, one build). `_log`'s hollow
  floor island is 0.3 m square, sized for log_fallen's 0.6 R mouth; at 0.76 R the LOD2 floor fan
  (mouth ring itself) spans about 0.42 m and overflowed. Fix: a `Log` subclass in the generator widens
  the floor island to 0.46 m in place and re-runs the gap check, pinned by asserting `build()` still
  constructs `Log(spec)`. No skill change: the existing per-face UV assert did its job.
- **`_log`'s rot ramp runs on object x, which misses a tilted mouth** (12 min, two bakes). The
  hollow's rot fades in 0.08-0.3 m behind half_l, tuned to log_fallen's square-on mouth. This mouth's
  lower lip juts 0.12 m past half_l, so the sunlit lower inner wall baked pale wood (inner-wall mean
  albedo 0.32). A first wrapper that shifted the ramp deeper made it worse. Fix: a wrapper around
  `_log.wood_high_material` that keeps the material and links only its rot factor to a radial mask
  (inside 0.195-0.225 m of the bent axis, toward the tip): inner-wall albedo 0.16, floor 0.06, AO 0.07.
  Found from per-island albedo/AO/roughness means, not the EEVEE still, whose up-facing inner wall
  showed sky specular. Skill text (pitfalls, "Cycles bakes"): "When a still shows a pale patch, read
  the baked albedo, AO and roughness per UV island before changing the shader: an island mean
  separates a wrong bake from sky specular on an up-facing face."
- **The first kit spot sat on the grass rim** (6 min, one shot run). A spot picked for shade only
  (offset (-10, -6.5)) put the log across the forest_base rim band and against the trunk. Fix: a probe
  over forest_base's `zone()` and `relief()` that keeps every sample along the log on the floor
  zone, 1.3 m off the trunk and clear of the other pieces, ranked by a rough canopy-shadow test:
  (-8.5, -6.0), yaw 130, y -0.121. Skill text (project.md, "Fast loops"): "Place a kit-scenario prop
  with a script over the base's own zone() and relief(), sampling the whole footprint, not the
  centre alone."
- **Two shot cameras sat inside the spruce's branches** (3 min, one re-run). Under-canopy close-ups
  need the camera below about 1 m and on the open side of the trunk. No skill change: piece-local.
- **The dense play forest hides floor props from play distance** (0 min). The play-scenario overview
  shows the canopy, not the logs; the kit forest at (-3, 1) carries the play-distance view of the
  variant. No skill change; noted on the review sheet.
