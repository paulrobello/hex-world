"""lily_cluster_c: five river_lily pads about 2.4 m across with a pink bud, a water-surface scatter prop (B3, owner A26, 2026-10-27). Refs:
assets-source/refs/water_props. Scripted route, prop kind: make kit-build PIECE=lily_cluster_c. Pipeline: `_water_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _water_props  # noqa: E402

_water_props.build_lily("lily_cluster_c", __file__, (0.5, 0.45, 0.4, 0.34, 0.3), 1.2, extra="bud")
