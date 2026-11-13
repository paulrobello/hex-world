# peak_granite friction log

The family's entries (helper staleness, the LOD floor, the rim band, the review renderer, the
lint base) are in `assets-source/kit/hill_meadow/friction.md`.

1. **Buried rock reached the hex border.** The mass is smooth-maxed under the grass, so its
   buried flanks are meshed too. The first peak's buried rock reached 13.99 m, under the rim and
   close enough to cross the skirt, and it spent triangles nobody sees. Three causes: the spines'
   sides sat only 0.5 m under the grass at their tips, the cleft's keep factor lifted the tops'
   buried skirts back up to the grass, and the tops' skirts fell 0.6 m per m. Fix: spine sides 4 m
   deep with a steeper tip falloff, the cleft lift only above the grass, top skirts falling 1.5 m
   per m. The buried reach fell to 10.06 m (3.86 m inside the rim) with the visible rock
   unchanged. Lost: about 20 min. Proposed `pitfalls.md` text, Landforms: "A rock mass
   smooth-maxed under a heightfield ground is meshed whole: probe the buried reach as well as the
   visible one, and keep every buried flank several metres under the grass well inside the rim,
   or it crosses the skirt and wastes the LOD budget."
2. **LOD1 over half of LOD0.** The first build failed `LOD1 1858 > 0.5 x LOD0 3676`: the ring and
   skirt are LOD-invariant, so halving the rock's target leaves LOD1 above 50%. Fix: rock targets
   1900/680/120. Lost: 5 min. No skill change: `gates.md` states the ceiling.
3. **A narrow cone does not read as ref-1.** The first passes made one steep cone. Ref-1's
   silhouette is a summit with separate lower pinnacles, so pass 4 placed six tops with clefts
   between them and spines down the flanks. Lost: two form passes, about 30 min. No skill change:
   piece-local.
4. **The steepest grass is steeper than the massif's.** The cap and scree's steepest LOD0 triangle
   is gradient 1.53 (the massif's test holds 1.5); the hill's is 1.26. Both are far under the
   walker's limit of 4, so the landform walker test holds them under 2. Lost: 2 min. No skill
   change.
