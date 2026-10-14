"""Tiny 3-LOD hex piece (cap + 1-row skirt), collider and socket, used by the lib tests and the gate fixture."""
from lib import collider, export, geom, hexgeo, materials, scene, uv


def build(piece_id="fixture", spacings=(4.0, 8.0, 50.0)):
    """spacings: interior lattice spacing per LOD (m). The gate fixture passes a denser LOD0 so its
    LOD1/LOD2 ratios meet the kit budgets (the 48-tri skirt is fixed per LOD)."""
    scene.reset_scene(piece_id)
    root = scene.empty(piece_id)
    ring = hexgeo.border_ring(4)
    mat = materials.pbr_material(f"{piece_id}_cap", materials.srgb("#5a8f3a"), 0.8)
    lods = []
    for i, spacing in enumerate(spacings):
        cap = geom.cdt_cap(f"{piece_id}_lod{i}_cap", ring, hexgeo.lattice_points(spacing, 1.0), lambda x, y: 0.0)
        geom.set_border_normals_up(cap)
        uv.planar_cap_uv(cap)
        cap.data.materials.append(mat)
        sk = geom.skirt(f"{piece_id}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, 1)
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.005, 0.005)  # parked outside the hex: the cap owns the unique atlas
        sk.data.materials.append(mat)
        body = geom.join([cap, sk], f"{piece_id}_lod{i}")
        body.parent = root
        lods.append(body)
    col = collider.collider_from(lods[2], "cap", f"{piece_id}_collider")
    col.parent = root
    lane = hexgeo.lane_frame(0, 0.0, 6.0)
    export.add_socket("socket-lane-0", (*lane["center"], 0.0), root, yaw_deg=0.0,
                      kind="road", edge=0, offset=0.0, width=6.0)
    return root, lods
