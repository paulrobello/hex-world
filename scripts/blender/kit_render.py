"""make kit-renders PIECE=<id>: reimport check + six-view renders of the staged export.
Committed JPEGs go to assets-source/kit/<id>/renders/, lossless PNGs to target/kit-renders/<id>/."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import gate  # noqa: E402

piece = sys.argv[sys.argv.index("--") + 1]
stage = pathlib.Path("target/kit-stage") / piece
gate.reimport_check(stage / f"{piece}.glb", stage / "export-report.json")
objs = gate.import_glb(stage / f"{piece}.glb")
print("\n".join(gate.render_six_views(objs, pathlib.Path("assets-source/kit") / piece / "renders",
                                      png_dir=pathlib.Path("target/kit-renders") / piece)))
