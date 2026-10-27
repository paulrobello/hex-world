"""lily_cluster_b: seven river_lily pads about 2.8 m across with a white water lily, a water-surface scatter prop (B3, owner A26, 2026-10-27). Refs:
assets-source/refs/water_props. Scripted route, prop kind: make kit-build PIECE=lily_cluster_b. Pipeline: `_water_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _water_props  # noqa: E402

_water_props.build_lily("lily_cluster_b", __file__, (0.48, 0.44, 0.4, 0.36, 0.33, 0.3, 0.26), 1.4, extra="flower")
