# Owner queue, 2026-11-22: rivers and waterfalls at levels

The questions from the draft spec `docs/superpowers/specs/2026-11-22-rivers-at-levels-design.md`. Each one has a recommended option. "Go with the recommendations" answers all of them.


- **RQ1. Default level when a waterfall is possible.** O1: a waterfall counts as a cliff, never picked by default, R/scroll reaches it, as cliffs today. O2: placement prefers continuing the river at its own level but picks a waterfall over a grass cliff. **Recommended: O1.**
- **RQ2. Fall face look.** O1: grey granite ledges in the boulder and massif palette, moss and dark wet streaks, a straight lip. O2: stepped ledges that break the sheet into two or three tiers. O3: a soil and turf bank with exposed roots. **Recommended: O1 for the 4 m drop and O2 for 8 and 12 m, one generator, through reference approval.**
- **RQ3. Rapids.** O1: rivers change level only at waterfalls in this round. O2: also a river ramp tile on the straight-bank hillside, with white water down a 14% channel. **Recommended: O1**, since a ramp is a second river family with its own bed contract.
- **RQ4. Waterfalls into the sea.** O1: rivers reach the sea only through a level-0 mouth, and a waterfall may land on the mouth's river edge. O2: a new sea-cliff tile (raised land meeting ocean at a rock face) with a waterfall straight into the surf. **Recommended: O1 now, O2 as a later card.**
- **RQ5. Sheet look.** O1: a thick, mostly white sheet with streaks, readable from the play camera. O2: a clear, refracting sheet at the lip turning white below. **Recommended: O2 at High and Ultra, O1 at Low and Medium**, chosen from in-engine options as the water foam was (A3).
- **RQ6. Fish at waterfalls.** O1: fish stay in the region on their side. O2: trout sometimes leap a waterfall upstream on a scripted arc between the two regions. **Recommended: O1 now, O2 later.**
- **RQ7. River rail bridge at levels.** O1: it stays at level 0, as the rail-at-levels spec set. O2: it becomes raisable with the road bridge, since raised rail exists. **Recommended: O2.**
- **RQ8. Waterfalls at bridges.** O1: a bridge tile may be the lip or the pool. O2: only the lip (a pool beside the arch is refused). O3: no waterfall on a bridge tile. **Recommended: O1**, checked in engine for the pool beside the arch.
- **RQ9. Plunge pool depth.** O1: no bed change, the pool reads through foam and ripples. O2: new deepened-pool base variants for every river tile a waterfall can land on. **Recommended: O1.**
- **RQ10. Waterfall sound.** O1: a quiet positional loop at each pool, gain by drop, at most two voices near the focus. O2: occasional one-shots (a splash surge), like land ambience. O3: a low loop with occasional surges. **Recommended: O1**, auditioned against the fan-noise concern, with O2 as the fallback.
- **RQ11. Showing flow to the player.** O1: no flow indicator in play (the water's motion shows it), arrows only in the debug overlay. O2: the ghost shows the flow its placement would give. **Recommended: O2**, since the new refusal depends on a direction the player cannot otherwise predict.
- **RQ12. A chain that reverses.** O1: a flat chain with no spring or mouth may reverse when a waterfall is placed at one end, and boats and foam re-derive. O2: once a chain has a direction it never reverses, and placing against it is refused. **Recommended: O1.**


## Answers (owner, 2026-11-22)

"Go with the recommendations": RQ1 O1, RQ2 O1 for the 4 m drop and O2 for 8 and 12 m, RQ3 O1, RQ4 O1 (sea-cliff tile is a later card), RQ5 O2 at High/Ultra and O1 at Low/Medium (picked from in-engine options), RQ6 O1, RQ7 O2, RQ8 O1, RQ9 O1, RQ10 O1 (auditioned, O2 fallback), RQ11 O2, RQ12 O1.
