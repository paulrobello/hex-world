"""beach_dunegrass_b: a young marram clump on a small sand mound, about 0.65 m tall, greener than
beach_dunegrass with fewer dead blades and no seed spikes (refs: assets-source/refs/dune_grass ref-1
and ref-2's small clumps, approved 2026-10-20; the mound, owner BD-Q3). Scripted route, prop kind
(LOD0 <= 1,500 tris): make kit-build PIECE=beach_dunegrass_b. Pipeline: `_dunegrass`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _dunegrass  # noqa: E402

CLUMP = _dunegrass.Clump(cards=9, height=(0.58, 0.7), width=0.72, blades=120, dead=0.05, spikes=False,
                         mound_r=0.42, mound_h=0.1, straw_from=(0.65, 0.85))
_dunegrass.build_piece("beach_dunegrass_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, CLUMP,
                       height_range=(0.55, 0.8), reach_range=(0.35, 0.7))
