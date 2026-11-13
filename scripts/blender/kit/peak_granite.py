"""peak_granite: a single-tile landform (P4, owner A40 O1 2026-10-27), kind base, KTX2 class piece, pack hero.
Spec: docs/superpowers/specs/2026-11-13-single-tile-hills-design.md. The form is `_hills.FORMS["peak_granite"]`
and the pipeline `_massif.build` through `_hills`: make kit-build PIECE=peak_granite."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hills  # noqa: E402

_hills.build("peak_granite", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
