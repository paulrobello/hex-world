"""reed_bed: a dense common-reed stand about 4 x 2 m and 2.4 m tall with buff and purple-brown plumes, a water-surface scatter prop (B3, owner A26, 2026-10-27). Refs:
assets-source/refs/water_props. Scripted route, prop kind: make kit-build PIECE=reed_bed. Pipeline: `_water_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _water_props  # noqa: E402

_water_props.build_reed_bed("reed_bed", __file__)
