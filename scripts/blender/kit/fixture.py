"""Gate fixture piece: tiny base-like hex with 3 LODs, a collider and one lane socket.
The one generator allowed to import the tests package (preflight M5)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export  # noqa: E402
from tests import fixture_piece  # noqa: E402

out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
root, lods = fixture_piece.build("fixture", spacings=(1.5, 8.0, 50.0))
bake.make_masks(64, 1.0, 0.5, 0.0, None, str(out / "fixture.masks.png"))
export.export_glb(root, str(out / "fixture.glb"))
export.write_export_report(str(out / "export-report.json"), root, "fixture", __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature(lods),
                       device="CPU", note="fixture: no bake")
