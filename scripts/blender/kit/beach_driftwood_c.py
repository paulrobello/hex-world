"""beach_driftwood_c: a short, thick stub of sea-bleached wood on the beach's dry sand, about 1.2 m
long and 0.4 m thick: a root flare with three snapped roots at the butt and a deep, splintered break
at the other end (refs: assets-source/refs/driftwood ref-1's root flare, colour and grain; approved
2026-10-20). Scripted route, prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=beach_driftwood_c.

`_driftwood` gives `_log`'s pipeline the bleached materials. The scatter lays it along the coast
(`scatter::beach_band`)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _driftwood  # noqa: E402

D = _driftwood.D
SPEC = _driftwood.Spec(
    half_l=0.5, r0=0.17, bury=0.08, taper=0.01, flare=(0.09, 0.32), lump=0.08, bend=(0.02, 1.0),
    seed_off=(6.6, 2.9, -9.4),
    # Three snapped roots spread round the flare.
    stubs=[(-0.4, D(25), 0.55, 0.28, 0.07, 0.03), (-0.42, D(115), 0.5, 0.2, 0.065, 0.03),
           (-0.38, D(160), 0.6, 0.26, 0.06, 0.028)],
    lod1_stubs=[0, 1, 2],
    rim=[(0.07, 2, 0.3), (0.04, 5, 1.7)], hollow=None, peak=(0.1, 0.03, 0.05),
    splinters=[(0.05, 3, 0.9), (0.03, 7, 2.4)],
    rings=(8, 4, 2, 60))

_driftwood.build("beach_driftwood_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, SPEC,
                 length=(1.0, 1.5), top=(0.25, 0.55))
