"""massif_granite: the 7-cell mega-tile ground piece with a craggy granite peak of about 36 m
(owner A53 2026-10-29, "ref-2 approved"; assets-source/refs/massif_granite). Kind hero, pack hero,
KTX2 class hero. The pipeline and its contracts are `_massif.build`:
make kit-build PIECE=massif_granite."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _massif

_massif.build(pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
