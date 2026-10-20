"""beach_dunegrass_c: a wind-bent marram clump on a broad sand hummock, about 0.8 m tall, its blades
swept to one side, more dead straw and two seed spikes (refs: assets-source/refs/dune_grass ref-2's
hummocks, ref-1's plant, approved 2026-10-20; the mound, owner BD-Q3). Scripted route, prop kind
(LOD0 <= 1,500 tris): make kit-build PIECE=beach_dunegrass_c. Pipeline: `_dunegrass`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _dunegrass  # noqa: E402

CLUMP = _dunegrass.Clump(cards=10, height=(0.7, 0.85), width=0.9, blades=135, dead=0.22, spikes=True,
                         mound_r=0.7, mound_h=0.2, lean=0.2, drift=(0.22, 0.08), straw_from=(0.45, 0.7))
_dunegrass.build_piece("beach_dunegrass_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, CLUMP,
                       height_range=(0.75, 1.05), reach_range=(0.5, 1.0))
