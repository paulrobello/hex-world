# river_mouth_l_base friction (2026-10-14)

Built from the same helper as `river_mouth_base` (`scripts/blender/kit/_mouth.py`); its friction log covers the shared construction.

1. **The mirror is not the reflected triangulation.** The 2D inputs are reflected before the CDT, but cocircular lattice points let Delaunay break ties differently, so the mirrored LODs differ slightly (LOD2 coast-dry error 0.0109 m against 0.0254 m on the right-handed piece, same tri counts). Refinement and the profile check rerun with hand "l", so the tolerance holds independently. No time lost.
2. **A zero tangent appeared only on the mirror.** The hull collider of the mirrored LOD2 had a zero-tangent vertex that the right-handed one did not, so `kit-compress` failed on this piece first. The fix is shared (collider cleanup and UV, see river_mouth_base friction item 5). About 4 minutes.
