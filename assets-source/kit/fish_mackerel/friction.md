# fish_mackerel friction log

Shared builder `scripts/blender/kit/_fauna.py` (imported, not edited); see the fish_trout and bird_starling
logs for the shared items.

1. **A concave plate of more than five points fans across its notch.** The deep-forked caudal (a 7-point
   outline with the notch as its one reflex vertex) came out of `Builder.finish` as a fan: two triangles
   spanned the notch with the wrong winding, so the fork was filled in, and the overlapping sheets baked
   AO 0 there (a black wedge in the EEVEE still, caudal AO p05 0.0). bmesh leaves a new face's stored
   normal unset, and `bmesh.ops.triangulate`'s ear clipping projects each ngon along it. The trout's
   5-point fork happens to triangulate correctly (checked on its committed GLB: every caudal triangle
   faces its sheet's way). Fix in the generator, not the approved helper: `b.bm.normal_update()` before
   `b.finish(...)`. Found by dumping the caudal island's triangles from the staged GLB. Proposed
   pitfalls.md text (Leaves, foliage and LODs, or a fauna entry): "**`_fauna.Builder.finish` triangulates
   without face normals.** bmesh leaves a new face's normal unset and `bmesh.ops.triangulate` ear-clips
   along it, so a concave plate beyond the trout's 5-point fork (a deep caudal fork) fans across its
   notch. Call `b.bm.normal_update()` before `finish`, and read a plate's triangles off the staged GLB
   when its AO shows a black wedge." Lost about 12 minutes.
2. **A paired fin laid flat on the flank bakes dark.** The pectoral at the mackerel's mid-flank root,
   with the trout's fin directions, lay nearly parallel to the body 0.8 mm off it; both sheets share one
   island, so the inner sheet's occluded AO won (pectoral island mean 0.27). Held out from the flank
   (`dir_b` 0.85 outward), it bakes 0.50 and the piece's AO is mean 0.940, p05 0.655 (trout 0.943 /
   0.686). No skill change: one piece; the trout's lower root clears its flank. Lost about 4 minutes.
3. **Species by water needs no plan change, but cannot be a static slot split.** Fish species follow
   spawn water, which `set_world` decides per world, so the goldfinch's per-flock `FormSlots` stride
   cannot apply; both fish meshes cover every fish slot and the vertex shader drops a copy whose palette
   offset (`anim.z`, already the spawn water) belongs to the other form (`boids::FLAG_RIVER_FISH`). No
   skill change (runtime, documented in docs/agent-operability.md). Lost about 0 minutes.
4. **Framing fish blind.** Nothing in the state dump says where fish are, and at 90 m a 0.8 m fish is
   6-8 px, so the first overviews showed no school and the river looked empty (the trout read as pale
   green specks under the murky surface). A fixed-clock run replays the flock closely enough to frame
   from: take play-distance overviews and long-lens (fov 14) views at fixed frames, unproject the school's
   pixels onto the fish plane, then add the framed shots right after that frame in the same script
   (`docs/checkpoints/fish_mackerel/make_script.py`). Two later runs both framed the same school groups;
   the fish within a group sat differently in the long-lens pair (GPU boids read neighbours mid-pass), so
   frame a group, not one fish. The first of those runs also caught a smeared fish in one frame that the
   re-run did not repeat. Proposed SKILL.md text (Owner checkpoints): "Boid shots: under `--clock fixed`
   take overviews first, read the school or flock off them (unproject its pixels to world), and insert the
   framed play-distance and swim-pair shots right after that frame in the same script; the replay lands the
   same groups in frame, not the same fish." Lost about 10 minutes.
