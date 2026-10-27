# Friction: B3 water props (lily_cluster, lily_cluster_b, lily_cluster_c, reed_bed)

One log for the four pieces, which share `scripts/blender/kit/_water_props.py`.

1. **The gate cannot take an empty LOD2.** The brief asked the pads to leave LOD2 past 400 m, but every
   lily piece's only LOD2 content is pads, and `kit_gate` (`lod_budget_errors`, the reimport-name
   test, about 30 per-LOD loops) and scatter's `footprints_match_the_kit` all expect a `<id>_lod2`
   mesh. Making LOD2 optional is a new gate and runtime branch (the runtime already skips a missing
   LOD mesh in `play.rs` `build_piece`). Fix taken: each LOD2 keeps the biggest pads as 2-tri quads
   (as many as the 15% ceiling allows: 1 of 3, 7 of 7, 3 of 5), and the flower, heart and bud slots
   leave LOD2 with measured reasons. About 10 minutes. Skill text proposed in the report.
2. **A greedy centre-first packer cannot fill a cluster disc.** Placing the biggest pad nearest the
   centre left no room for the second inside a 0.9 m reach. Fix: whole-layout random restarts, keeping
   the tightest layout (smallest sum of nearest-neighbour gaps). Two builds, about 3 minutes.
3. **Flat colliders.** `collider.collider_from(..., "box")` on a flat pad cluster has zero height and
   degenerate faces, and `"hull"` on coplanar points is degenerate too. Fix: a thin octagonal prism of
   the piece's reach (`_water_props._collider_disc`), which is exactly what `footprints_match_the_kit`
   measures. No time lost.
4. **Reed plumes drawn across a horizontal perpendicular came out edge-on** (thin spikes). Fix: rotate
   the three fins about the plume's own axis. One rebuild.
5. **This agent's own fork dispatch blocked it for about an hour (dispatched about 04:03Z, returned 05:13Z)**: a background `Agent` call
   with `subagent_type: "fork"` returned only when the fork finished. Dispatch a fork only when the
   parent has nothing to do until it returns, or use a fresh background agent.
6. **Steep colour-only blades light near-black in engine.** The reed leaves read as black dashes in play
   (the side facing the camera faced away from the high kit sun), though Blender showed them green.
   Fix: up-biased custom normals on every leaf, plume fin and LOD2 card (`card_normals`, CARD_UP 0.65),
   as the foliage-card rule does; stems keep face normals. One rebuild plus shots, about 6 minutes.
7. **The fork's mixed-world tile at (1, 5) was refused in play** ("cell is not next to the world"):
   the Rust layout test cannot see adjacency. Fix: a scan of the world's rim cells for a bend seed
   that draws lily_cluster_b and reed_bed, then (0, 3) below the spring. About 8 minutes.

## Proposed skill text (for the orchestrator to fold in)

- gates.md, LOD slots: "A piece cannot ship an empty LOD2: `kit_gate` (`lod_budget_errors`, the
  reimport names, the per-LOD loops) and scatter's `footprints_match_the_kit` all require
  `<id>_lod2`. A prop whose every slot would leave LOD2 keeps a coarse stand-in of its main slot
  (as many 2-tri quads as the ceiling allows: the B3 lily clusters) until an optional-LOD2 gate
  branch lands."
- pitfalls.md, Leaves: "Steep colour-only blades and cards (reed leaves, plume fins) light near
  black in engine on the side turned from the sun even when Blender shows them lit: give them
  custom normals blended toward +Z (`_water_props.card_normals`, 0.65) and keep stems on face
  normals."
- pitfalls.md, Export: "A flat prop has no box or hull collider (zero height, degenerate faces):
  build a thin prism at the footprint reach (`_water_props._collider_disc`)."
- dispatch.md, part 7: "A tile added to a shot or checkpoint world must be next to the world; grep
  the shots log for `placementRefused` after the first run (a Rust layout test cannot catch it)."
