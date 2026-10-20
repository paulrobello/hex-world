# fern_clump friction

- **The first arch read as a vase, not a shuttlecock** (10 min). Equal-length fronds with one
  elevation curve all ended at about the same height. Fix: an arch probe in plain Python (no Blender)
  over (start elevation, end elevation, length) per ring, then inner fronds steep and long (0.74 m
  tall) and outer ones short with their tips arching down to 0.17 m, about 1.3 m across. Skill text
  (pitfalls, "Leaves, foliage and LODs"): "Size a card plant's arch with a numeric probe of its path
  (height, reach, tip height per ring) before the first build; check the shape in a textured EEVEE
  still against the ref, since the gate cannot see it."
- **Reach includes the card's half width** (2 min). A reach assert on the centre-line path failed by
  0.06 m: the folded card adds half its width at the tip. Assert on the built mesh's vertices (done),
  never on the path. No skill change: piece-local.
- **Frond tile coverage sits near 0.3** (5 min). A once-pinnate frond is mostly gaps between pinnae;
  widening the pinnae from 0.36 to 0.44 of their spacing took coverage from 0.24 to 0.29-0.34, stable
  within 0.05 through mip 7 (the grass-tuft rule in pitfalls holds: stable matters more than 0.5).
  No skill change: covered by "Grass-tuft coverage sits near 0.5, and that is fine if it is stable".
