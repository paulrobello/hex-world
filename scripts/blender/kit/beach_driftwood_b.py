"""beach_driftwood_b: a forked branch of sea-bleached wood on the beach's dry sand, a thin stem about
1.9 m long with two long side branches reaching forward from it and a short snapped twig (refs:
assets-source/refs/driftwood ref-2's forked branch, ref-1's colour and grain; approved 2026-10-20).
Scripted route, prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=beach_driftwood_b.

`_driftwood` gives `_log`'s pipeline the bleached materials. The branches lean 55-60 degrees toward
the stem's +x end, so they stay within about 0.4 m of the stem's line, and the scatter lays the stem
along the coast (`scatter::beach_band`), so the slope across it lifts a branch tip by at most about
0.07 m at s = -4."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _driftwood  # noqa: E402

D = _driftwood.D
SPEC = _driftwood.Spec(
    half_l=0.95, r0=0.075, bury=0.035, taper=0.004, flare=(0.03, 0.25), lump=0.08, bend=(0.07, 1.6),
    seed_off=(-8.1, 4.4, 12.3),
    # The fork: two long branches leaving the stem's flanks toward +x, and a snapped twig on top.
    stubs=[(0.35, D(8), -1.0, 0.85, 0.048, 0.02), (0.05, D(172), -0.95, 0.68, 0.042, 0.018),
           (-0.45, D(80), 0.35, 0.14, 0.03, 0.018)],
    lod1_stubs=[0, 1],
    rim=[(0.03, 2, 0.4), (0.015, 5, 1.9)], hollow=None, peak=(0.06, 0.0, 0.01),
    splinters=[(0.02, 3, 0.5)],
    rings=(10, 4, 2, 70))

_driftwood.build("beach_driftwood_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, SPEC,
                 length=(1.9, 2.5), top=(0.1, 0.35))
