"""log_fallen_b: the butt section of a fallen spruce, about 2.2 m long and 0.52 m thick, a broad root
flare with three snapped roots at one end and a solid, splintered break at the other, one broken
branch stub, the trees' bark and moss in long streaks down one flank with small cushions on top
(refs: assets-source/refs/forest, "fallen log"). Same bark set and tint, moss and end-grain colours,
sink, atlas density and LOD scheme as log_fallen; the pipeline is `_log.build`. Scripted route,
prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=log_fallen_b.

The sink is log_fallen's rule at this length: the forest floor relief drops at most about 0.08 m
from a 1.1 m half-length log's centre to its ends (log_fallen: 0.127 m at 1.8 m), so 0.12 m keeps
both ends in the litter."""
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _log  # noqa: E402

D = math.radians
SPEC = _log.Spec(
    half_l=1.05, r0=0.26, bury=0.12, flare=(0.1, 0.55), lump=0.08, bend=(0.03, 1.1),
    seed_off=(-5.3, 9.1, 2.6),
    # Three snapped roots on the flare, one broken branch stub on the upper flank.
    stubs=[(-0.88, D(20), 0.5, 0.46, 0.09, 0.045), (-0.92, D(95), 0.4, 0.4, 0.085, 0.04),
           (-0.86, D(150), 0.55, 0.34, 0.08, 0.04), (0.3, D(60), -0.55, 0.26, 0.05, 0.03)],
    lod1_stubs=[0, 1, 2],
    # A deep, splintered break: two long tongues of wood and a spike off-centre toward the top.
    rim=[(0.11, 2, 0.4), (0.05, 5, 1.9)], hollow=None, peak=(0.2, 0.06, 0.07),
    splinters=[(0.09, 3, 0.9), (0.05, 7, 2.6)],
    rings=(12, 6, 4, 90),
    # Moss in long streaks along the grain down the +y flank (log_fallen: blotches over the top),
    # and small cushions on the top.
    moss=_log.Moss(toward=(0.0, 0.8, 0.6), lo_hi=(0.25, 0.7), scale=3.4, stretch=(0.25, 1.0, 1.0),
                   cut=(0.6, 0.7), top=(0.6, 0.9), cushion_scale=8.0, cushion_cut=(0.68, 0.74),
                   offset=(3.1, 1.7, -2.2)))

_log.build("log_fallen_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, SPEC,
           length=(2.2, 2.8), top=(0.55, 1.0))
