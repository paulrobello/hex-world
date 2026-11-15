"""hill_meadow: a single-tile landform (P4, owner A40 O1 2026-10-27), kind base, KTX2 class piece, pack hero.
Spec: docs/superpowers/specs/2026-11-13-single-tile-hills-design.md. The form is `_hills.FORMS["hill_meadow"]`
and the pipeline `_massif.build` through `_hills`: make kit-build PIECE=hill_meadow."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hills

_hills.build("hill_meadow", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
