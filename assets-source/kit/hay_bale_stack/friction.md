# hay_bale_stack friction (2026-10-19, built with hay_bale_round from kit/_hay.py)

1. **Hidden-face cull by inflated boxes opened slits.** Dropping every face whose vertices all lay inside
   a neighbouring bale's box inflated by 3 cm also removed the bevel faces that close the grooves
   between bales, so the first LOD0 showed see-through slits between the top and bottom rows. Fix: an
   exact inside test against each neighbour's closed low mesh (`BVHTree.find_nearest`, a vertex is
   inside when (p - hit) · n < -4 mm), and only faces wholly inside are dropped. 5 min. Proposed
   pitfalls.md text, under "Leaves, foliage and LODs":
   > **Cull a stacked prop's hidden faces against the neighbours' real surfaces, not their boxes.**
   > An inflated bounding-box test removed the bevels between the hay bales and opened slits;
   > `_hay.drop_hidden` tests every vertex against each neighbour's closed mesh (BVH nearest point and
   > its normal) and drops only faces wholly inside, plus faces facing down at the ground.
2. **Straw read as wood grain.** The first bake's flake bands and dark fibre streaks looked like planks
   in the EEVEE still. Fix: fibre darkening 0.75 to 0.4 and flake gaps narrower and lighter on the
   square bales only. 3 min (one rebake). No skill change: a look call caught by the textured still the
   skill already asks for.
3. **The 1.7 m eye-level shot is a kit-scenario camera, not walk mode.** Walk mode is play-only and the
   kit layout is where the pieces are placed, so `hay-eye` is a camera at y = 1.7 m. 0 min. No skill
   change; noted for the reviewer.
