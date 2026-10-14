"""Mock-only build of option O-b (corner-aware taper) for the corner-options stills. Not a kit piece:
nothing here ships, and it writes only under the stage dir given after `--`.

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P assets-source/kit/forest_forest_edge/ob_mock_build.py -- \
      target/kit-stage/forest_forest_edge_e<k>_ob <k>
Both sides tapered: in the mock neither corner's next edge is forest-forest."""
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))

import _forest_edge  # noqa: E402

k = int(sys.argv[sys.argv.index("--") + 2])
pathlib.Path(sys.argv[sys.argv.index("--") + 1]).mkdir(parents=True, exist_ok=True)
_forest_edge.build(f"forest_forest_edge_e{k}_ob", k, __file__, taper=(True, True))
