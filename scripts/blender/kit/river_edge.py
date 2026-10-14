"""river_edge: the river mouth at edge 0. The last STUB_L metres of the channel (x from JOINT_X to
APOTHEM) with the bed profile at OFFS, and the cut-column wall under the border section down to the
tile base. Placed at every River edge (data/sockets.ron edge_pieces). Contract: _river.py."""
import os
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import bake, collider, export, geom, scene, uv  # noqa: E402

import _river  # noqa: E402

PIECE = "river_edge"
# Bed rows per LOD. The border section (profile + wall) is fixed, so the ratios are
# (S*n + p) / (S*n0 + p) with S = 56 bed tris per row and p = 56 wall tris: 784 / 392 / 112.
SEGMENTS = (13, 6, 1)
MASK_SIZE = 512

out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
root = scene.empty(PIECE)
mats = _river.make_materials()
lods = []
for i, n in enumerate(SEGMENTS):
    o = _river.stub_mesh(f"{PIECE}_lod{i}", n)
    for name in _river.SLOTS:
        o.data.materials.append(mats[name])
    uv.planar_cap_uv(o)
    # The soil slot samples no UV map; park the vertical mouth wall on one texel like the meadow skirt
    # instead of the line the planar map collapses it to.
    loops = o.data.uv_layers["UVMap"].data
    for p in o.data.polygons:
        if abs(p.normal.z) < 0.5:
            for li in p.loop_indices:
                loops[li].uv = (0.005, 0.005)
    o.parent = root
    lods.append(o)
tris = [geom.tri_count(o) for o in lods]
assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: tris per LOD {tris}")
col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
col.parent = root
_river.lane_sockets(root, (0,))
texel = uv.measure_texel_density(lods[0], MASK_SIZE)
stats = None
if not skip_bake:
    stats = _river.bake_masks(lods[0], out, PIECE, size=MASK_SIZE)
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                       aoDistanceM=6.0, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, maskStats=stats, profileOffsets=_river.OFFS,
                       stubLengthM=_river.STUB_L, skipped=skip_bake)
