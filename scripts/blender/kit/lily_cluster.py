"""lily_cluster: three river_lily pads about 1.8 m across, a water-surface scatter prop (B3, owner A26, 2026-10-27). Refs:
assets-source/refs/water_props. Scripted route, prop kind: make kit-build PIECE=lily_cluster. Pipeline: `_water_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _water_props  # noqa: E402

_water_props.build_lily("lily_cluster", __file__, (0.46, 0.4, 0.32), 0.9)
