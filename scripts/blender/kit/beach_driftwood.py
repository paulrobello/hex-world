"""beach_driftwood: a long sea-bleached log on the beach's dry sand, about 2.7 m long and 0.3 m
thick, barkless, with a small root flare and two snapped roots at the butt, a snapped, splintered
end and one broken branch stub (refs: assets-source/refs/driftwood ref-1, approved 2026-10-20).
Scripted route, prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=beach_driftwood.

`_driftwood` gives `_log`'s pipeline the bleached materials. The scatter lays driftwood along the
coast (`scatter::beach_band`), where the sand is level along the log, so the sink only has to
cover the scatter's yaw jitter on the slope across it (about 0.05 m at 12 degrees, s = -4)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _driftwood  # noqa: E402

D = _driftwood.D
SPEC = _driftwood.Spec(
    half_l=1.3, r0=0.14, bury=0.07, taper=0.012, flare=(0.05, 0.35), lump=0.06, bend=(0.07, 0.9),
    seed_off=(3.7, -11.2, 5.9),
    # Two snapped roots on the flare and a broken branch stub on the upper flank (ref-1).
    stubs=[(-1.14, D(35), 0.55, 0.2, 0.05, 0.025), (-1.16, D(145), 0.5, 0.16, 0.045, 0.022),
           (0.42, D(62), -0.5, 0.15, 0.045, 0.026)],
    lod1_stubs=[2],
    rim=[(0.06, 3, 0.4), (0.03, 5, 1.2)], hollow=None, peak=(0.12, 0.02, 0.03),
    splinters=[(0.05, 3, 0.7), (0.02, 6, 2.1)],
    rings=(12, 6, 2, 90))

_driftwood.build("beach_driftwood", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, SPEC,
                 length=(2.4, 3.1), top=(0.2, 0.45))
