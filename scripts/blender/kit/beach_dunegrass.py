"""beach_dunegrass: a full marram clump on its own low sand mound, about 0.9 m tall and 1.3 m across,
narrow blue-green blades fading to straw tips, a few dead blades and two seed spikes (refs:
assets-source/refs/dune_grass ref-1, approved 2026-10-20; the mound, owner BD-Q3). Scripted route,
prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=beach_dunegrass. Pipeline: `_dunegrass`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _dunegrass  # noqa: E402

CLUMP = _dunegrass.Clump(cards=12, height=(0.8, 0.95), width=0.95, blades=150, dead=0.12, spikes=True,
                         mound_r=0.55, mound_h=0.15)
_dunegrass.build_piece("beach_dunegrass", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, CLUMP,
                       height_range=(0.8, 1.05), reach_range=(0.5, 0.9))
