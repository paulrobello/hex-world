# shrub_forest friction

1. **Leaf-cluster coverage below 0.5.** Leaves placed along twigs bunched and left gaps (0.41-0.46 per
   tile, stable down the mips but under the 0.5 floor); a bigger backing blob read as black scallops
   between cards. Fix: place leaf centres uniformly over the cluster disc and only orient them by the
   nearest twig, with a small, lighter backing blob: 0.53-0.55. Three rebuilds, about 8 minutes.
   Proposed pitfall text (Leaves, foliage and LODs): "A leaf-cluster tile holds coverage from even leaf
   placement over its disc, not from a dark backing blob: a large blob shows as flat dark scallops
   between cards in a dome (shrub_forest)."
2. **A card dome reads as a lollipop until it reaches the ground.** The first crown (centre 0.74 m,
   outer cards above -0.55 of the ellipsoid) sat on bare stems. Extending outer cards to -0.9 and
   lowering the centre to 0.64 m gave a shrub whose dome comes down to about 0.14 m. Judged from the
   textured EEVEE low side view; the 6-view render would not show it (untextured cards).
3. **Collider size is a gameplay choice for a prop.** fern_clump's 0.3 m core collider would let the
   walker into a 2 m dense dome; the shrub uses a 0.7 m prism so the walker stops at the leaves and
   scatter keeps other footprints out. Re-provenance after the change. No skill change.
4. **Kit layout cell count is pinned** (`hex-render` `tile_counts`, 27 then 28). Adding a kit-scenario
   cell needs that test and its comment updated; `make checkall` caught it. Proposed skill text
   (project.md, Fast loops and runtime couplings): "A new kit-scenario cell changes
   `hex_render::tests::tile_counts`."
