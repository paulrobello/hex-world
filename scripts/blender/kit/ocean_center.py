"""ocean_center: seabed detail for the ocean tile, pebble clusters and a few low rocks (refs/ocean
ref-1). Scripted route; positions, sizes and the bed level come from _coast.py, which ocean_base
also bakes its contact shadows from.

A separate piece because the play water draws its placeholder bed until the tile's centre piece is
in the kit, and because the detail can thin per LOD while the base keeps its fixed border ring.
Every stone stays 1.5 m inside the border and within the floor's 0.10 m allowance above the bed
(_coast.BED_TOL_M: the water reads the prepass depth as its bed), so it touches no seam. Kind `prop` (1,500-tri LOD0 budget): it carries no lanes, and the gate's
`center` kind is the road hub that must meet the edge pieces at x = +-6. Colour-only slots plus a
uniform §9.4 masks map (MASKS): every stone lies on the ocean floor, under at least 2.5 m of water at
every sea level, so it takes neither snow (R 0) nor rain wetness (G 0, as the contract's seabeds), no
night emission (B 0), and A 1 keeps the AO the slots had before the map (none), so the MVP look is
unchanged."""
import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, collider, export, geom, lod, materials, scene, uv  # noqa: E402
import _coast  # noqa: E402

PIECE = "ocean_center"
MASK_SIZE = 32  # uniform, so the size only has to survive the mip chain
MASKS = (0.0, 0.0, 0.0, 1.0)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(PIECE)

items = _coast.scatter()
rocks = [it for it in items if it[0] == "rock"]
pebbles = sorted((it for it in items if it[0] == "pebble"), key=lambda it: -it[3])
# (rock subdiv, pebble subdiv, pebbles kept: largest first). LOD2 keeps the rocks and a few pebbles.
LODS = ((2, 1, len(pebbles)), (1, 1, 12), (1, 0, 0))

mats = {"rock": materials.pbr_material("coast_rock", (*_coast.ROCK_COLOR, 1.0), 0.6),
        "pebble": materials.pbr_material("coast_pebble", (*_coast.PEBBLE_COLOR, 1.0), 0.45)}
root = scene.empty(PIECE)
lods = []
for i, (rock_sub, peb_sub, keep) in enumerate(LODS):
    parts = [_coast.stone(f"{PIECE}_l{i}_r{n}", it, rock_sub) for n, it in enumerate(rocks)]
    parts += [_coast.stone(f"{PIECE}_l{i}_p{n}", it, peb_sub) for n, it in enumerate(pebbles[:keep])]
    for p, it in zip(parts, rocks + pebbles[:keep]):
        p.data.materials.append(mats[it[0]])
    body = geom.join(parts, f"{PIECE}_lod{i}")
    lod.triangulate(body)
    uv.atlas_unwrap([body])  # UV0 for tangents and the uniform masks map
    body.parent = root
    lods.append(body)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
print(f"{PIECE}: {len(rocks)} rocks, {len(pebbles)} pebbles, tris per LOD {tris}")

col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_png(MASK_SIZE, np.tile(np.asarray(MASKS, np.float32), (MASK_SIZE * MASK_SIZE, 1)),
               str(out / f"{PIECE}.masks.png"))
# Nothing to bake (colour-only slots, uniform masks); the report records that and the receiver.
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, skipped=True,
                       reason="colour-only slots; uniform masks, every stone always under the sea",
                       masks={"snow": MASKS[0], "porosity": MASKS[1], "night": MASKS[2], "ao": MASKS[3]})
