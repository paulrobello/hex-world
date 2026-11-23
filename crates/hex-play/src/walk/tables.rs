use hex_core::piece::{PieceTable, WalkClass};

/// Whether `piece`'s collider blocks the walker (`data/pieces.ron` `walk: Some(Obstacle)`).
pub fn is_obstacle(piece: &str) -> bool {
    PieceTable::builtin().is_walk(piece, WalkClass::Obstacle)
}

/// The pieces whose collider blocks the walker, in `data/pieces.ron` order.
pub fn obstacles() -> impl Iterator<Item = &'static str> {
    PieceTable::builtin().with_walk(WalkClass::Obstacle)
}

/// The props the walker deliberately passes through (`walk: Some(WalkThrough)`). With
/// [`obstacles`] and `scatter::ground_props` it accounts for every prop piece the catalog places
/// (`every_prop_piece_is_classified`).
pub fn walk_through() -> impl Iterator<Item = &'static str> {
    PieceTable::builtin().with_walk(WalkClass::WalkThrough)
}

/// Parts of centre pieces that block the walker, as (centre piece, part, footprint corners in the
/// tile frame). A centre's own collider is its whole bed, so a part that stands up from it gets a
/// footprint here: the junction's point lever (`_rail_junction.py` `lever_parts`: the base timber
/// 0.7 m in from edge 0, 2.6 m off the main line away from corner 0, and the lever leaning 0.45 m
/// toward edge 3). The rails and sleepers are walkable ground. A rail viaduct's two abutments and
/// two piers ([`VIADUCT_PIER`]) are the stone's widest extent below head height (1.8 m), its faces
/// at z ±3.0: the battered foot on decks 2 and 3 (the bake report's `pierFootprintsRuntimeXZ`),
/// the arch ring at 1.8 m on deck 1, whose arches spring at 0.5 m. Its deck is not ground
/// (`data/pieces.ron` `not_ground`), so the walker walks the meadow under the arches.
pub const CENTRE_OBSTACLES: &[(&str, &str, [[f32; 2]; 4])] = &[
    (
        "rail_junction_center",
        "rail_lever",
        [
            [12.69, -2.85],
            [13.46, -2.85],
            [13.46, -2.35],
            [12.69, -2.35],
        ],
    ),
    viaduct_pier("rail_viaduct_l1", -13.854, -8.897),
    viaduct_pier("rail_viaduct_l1", -5.103, -1.897),
    viaduct_pier("rail_viaduct_l1", 1.897, 5.103),
    viaduct_pier("rail_viaduct_l1", 8.897, 13.854),
    viaduct_pier("rail_viaduct_l2", -13.854, -9.1875),
    viaduct_pier("rail_viaduct_l2", -4.8125, -2.1875),
    viaduct_pier("rail_viaduct_l2", 2.1875, 4.8125),
    viaduct_pier("rail_viaduct_l2", 9.1875, 13.854),
    viaduct_pier("rail_viaduct_l3", -13.854, -9.0875),
    viaduct_pier("rail_viaduct_l3", -4.9125, -2.0875),
    viaduct_pier("rail_viaduct_l3", 2.0875, 4.9125),
    viaduct_pier("rail_viaduct_l3", 9.0875, 13.854),
];

/// The [`CENTRE_OBSTACLES`] part of a rail viaduct's abutments and piers: grass keeps off them too
/// (`grass::exclusions`).
pub const VIADUCT_PIER: &str = "rail_viaduct_pier";

/// A viaduct's abutment or pier from x `x0` to `x1` across its whole width (z ±3.0).
const fn viaduct_pier(
    centre: &'static str,
    x0: f32,
    x1: f32,
) -> (&'static str, &'static str, [[f32; 2]; 4]) {
    (
        centre,
        VIADUCT_PIER,
        [[x0, -3.0], [x1, -3.0], [x1, 3.0], [x0, 3.0]],
    )
}

/// Landform tiles' standing rock that blocks the walker, as (tile, part, convex parts of its
/// footprint in the tile frame, x and z). The massif's is its rock foot as measured top-down on its
/// LOD0: the outermost rock along each 5-degree bearing from the anchor (the spines reach 14-20 m,
/// just into every ring cell), split into convex fans from the anchor
/// (`docs/checkpoints/massif_granite/walk_parts.py` over `form/form-measure.json`). The single-tile
/// landforms' (spec 2026-11-13 D13) are measured the same way on the rock visible from above on
/// their LOD0: peak_granite's as fans from its summit, hill_meadow's as one hull per slab
/// (`docs/checkpoints/hill_meadow/walk_parts.py`). The grass slopes and scree below the rock are
/// walkable ground.
pub const FOOTPRINT_OBSTACLES: &[(&str, &str, &[&[[f32; 2]]])] = &[
    ("massif_granite", "massif_granite", MASSIF_ROCK_FOOT),
    ("hill_meadow", "hill_meadow", HILL_ROCK_FOOT),
    ("peak_granite", "peak_granite", PEAK_ROCK_FOOT),
];

/// hill_meadow's four granite slabs: 4 convex hulls, 62 m² (their top-down cover measures 57 m²).
const HILL_ROCK_FOOT: &[&[[f32; 2]]] = &[
    &[
        [-1.15, 0.75],
        [-0.85, 0.05],
        [-0.05, -1.15],
        [0.85, -1.85],
        [2.05, -2.35],
        [2.85, -2.45],
        [6.35, -1.85],
        [6.85, -1.35],
        [7.05, -0.65],
        [6.75, -0.05],
        [6.05, 0.45],
        [3.35, 2.25],
        [1.75, 2.55],
        [1.45, 2.55],
        [0.35, 2.35],
        [-0.35, 2.15],
        [-0.85, 1.65],
        [-1.15, 1.05],
    ],
    &[
        [-6.35, -6.55],
        [-6.15, -6.95],
        [-5.75, -7.25],
        [-4.95, -7.25],
        [-2.85, -6.25],
        [-2.05, -5.55],
        [-1.45, -4.75],
        [-1.35, -4.45],
        [-1.35, -3.85],
        [-1.95, -3.15],
        [-2.45, -2.85],
        [-3.15, -2.75],
        [-4.65, -3.05],
        [-5.15, -3.35],
        [-5.55, -3.75],
        [-6.25, -5.95],
    ],
    &[
        [-0.25, -7.65],
        [1.35, -9.15],
        [2.15, -9.75],
        [3.25, -9.65],
        [3.55, -9.25],
        [3.85, -8.35],
        [3.95, -7.85],
        [3.75, -6.95],
        [3.45, -6.45],
        [3.15, -6.15],
        [2.15, -5.65],
        [1.45, -5.55],
        [0.75, -5.75],
        [-0.05, -6.35],
        [-0.15, -6.65],
    ],
    &[
        [-3.45, 3.05],
        [-3.15, 2.25],
        [-2.35, 1.85],
        [-1.65, 1.85],
        [-1.25, 1.95],
        [-0.75, 2.45],
        [-0.75, 3.45],
        [-0.85, 3.75],
        [-1.55, 4.15],
        [-2.45, 4.25],
        [-3.25, 3.85],
    ],
];

/// peak_granite's rock foot from its summit (-0.6, -0.2): 18 convex fans, 206 m² (the rock's
/// top-down cover measures 195 m²).
const PEAK_ROCK_FOOT: &[&[[f32; 2]]] = &[
    &[[-0.60, -0.20], [1.95, -3.24], [2.17, -4.15]],
    &[
        [-0.60, -0.20],
        [2.17, -4.15],
        [2.60, -5.75],
        [2.22, -6.26],
        [1.69, -6.49],
        [1.13, -6.64],
        [0.53, -6.58],
    ],
    &[[-0.60, -0.20], [0.53, -6.58], [-0.03, -6.73]],
    &[
        [-0.60, -0.20],
        [-0.03, -6.73],
        [-0.60, -7.36],
        [-1.25, -7.68],
        [-1.94, -7.80],
        [-2.55, -7.49],
    ],
    &[
        [-0.60, -0.20],
        [-2.55, -7.49],
        [-3.46, -8.05],
        [-4.33, -8.20],
        [-5.00, -7.82],
        [-5.61, -7.35],
        [-6.12, -6.78],
    ],
    &[
        [-0.60, -0.20],
        [-6.12, -6.78],
        [-6.64, -6.24],
        [-7.04, -5.60],
        [-7.19, -4.82],
        [-7.30, -4.07],
        [-6.97, -3.17],
        [-6.18, -2.23],
    ],
    &[
        [-0.60, -0.20],
        [-6.18, -2.23],
        [-6.79, -1.86],
        [-7.24, -1.37],
    ],
    &[
        [-0.60, -0.20],
        [-7.24, -1.37],
        [-9.33, -0.96],
        [-9.75, -0.20],
        [-9.84, 0.61],
    ],
    &[
        [-0.60, -0.20],
        [-9.84, 0.61],
        [-10.02, 1.46],
        [-9.88, 2.29],
        [-9.65, 3.10],
        [-9.33, 3.87],
        [-8.86, 4.57],
        [-8.05, 5.02],
        [-7.30, 5.43],
    ],
    &[
        [-0.60, -0.20],
        [-7.30, 5.43],
        [-6.64, 5.84],
        [-5.82, 6.02],
        [-4.88, 5.92],
        [-4.00, 5.68],
        [-2.45, 3.78],
    ],
    &[[-0.60, -0.20], [-2.45, 3.78], [-2.06, 3.81]],
    &[
        [-0.60, -0.20],
        [-2.06, 3.81],
        [-2.02, 5.10],
        [-1.64, 5.71],
        [-1.16, 6.23],
        [-0.60, 6.45],
        [-0.02, 6.44],
        [0.46, 5.81],
        [0.71, 4.71],
    ],
    &[[-0.60, -0.20], [0.71, 4.71], [1.18, 4.70], [1.65, 4.63]],
    &[
        [-0.60, -0.20],
        [1.65, 4.63],
        [2.44, 5.06],
        [3.00, 4.94],
        [3.44, 4.61],
    ],
    &[
        [-0.60, -0.20],
        [3.44, 4.61],
        [6.64, 7.04],
        [7.62, 6.70],
        [8.41, 6.11],
        [8.94, 5.31],
        [9.34, 4.43],
        [9.57, 3.50],
        [9.35, 2.47],
        [8.89, 1.47],
    ],
    &[
        [-0.60, -0.20],
        [8.89, 1.47],
        [9.23, 0.66],
        [9.46, -0.20],
        [9.44, -1.08],
        [9.32, -1.95],
    ],
    &[
        [-0.60, -0.20],
        [9.32, -1.95],
        [9.27, -2.84],
        [8.93, -3.67],
        [8.18, -4.29],
        [7.27, -4.74],
        [6.14, -4.92],
        [4.68, -4.63],
    ],
    &[[-0.60, -0.20], [4.68, -4.63], [3.69, -4.49], [1.95, -3.24]],
];

/// The massif's rock foot: 20 convex parts, 543 m² (the rock's top-down cover measures 544 m²;
/// form pass 3, the rock half buried in the raised grass).
pub(super) const MASSIF_ROCK_FOOT: &[&[[f32; 2]]] = &[
    &[
        [0.00, 0.00],
        [3.35, -5.80],
        [4.10, -8.79],
        [4.10, -11.28],
        [3.70, -13.81],
        [2.43, -13.79],
        [1.04, -11.85],
    ],
    &[
        [0.00, 0.00],
        [1.04, -11.85],
        [0.00, -11.30],
        [-0.92, -10.56],
        [-1.68, -9.55],
        [-1.84, -6.86],
    ],
    &[[0.00, 0.00], [-1.84, -6.86], [-2.50, -6.86]],
    &[[0.00, 0.00], [-2.50, -6.86], [-3.55, -7.61], [-4.95, -8.57]],
    &[[0.00, 0.00], [-4.95, -8.57], [-7.05, -10.08]],
    &[
        [0.00, 0.00],
        [-7.05, -10.08],
        [-12.02, -14.33],
        [-14.35, -14.35],
        [-13.87, -11.63],
        [-11.96, -8.37],
    ],
    &[
        [0.00, 0.00],
        [-11.96, -8.37],
        [-12.82, -7.40],
        [-13.59, -6.34],
        [-13.34, -4.86],
        [-13.04, -3.49],
        [-12.31, -2.17],
        [-10.86, -0.95],
    ],
    &[
        [0.00, 0.00],
        [-10.86, -0.95],
        [-13.30, -0.00],
        [-15.34, 1.34],
        [-16.05, 2.83],
        [-16.03, 4.30],
        [-15.32, 5.57],
        [-14.59, 6.80],
    ],
    &[
        [0.00, 0.00],
        [-14.59, 6.80],
        [-16.80, 9.70],
        [-16.30, 11.41],
        [-12.95, 10.86],
        [-10.11, 10.11],
        [-7.84, 9.35],
        [-5.85, 8.36],
    ],
    &[
        [0.00, 0.00],
        [-5.85, 8.36],
        [-5.60, 9.70],
        [-4.94, 10.60],
        [-4.21, 11.56],
        [-3.36, 12.56],
    ],
    &[
        [0.00, 0.00],
        [-3.36, 12.56],
        [-2.83, 16.05],
        [-1.58, 18.03],
        [-0.00, 16.10],
        [1.26, 14.35],
        [2.08, 11.82],
    ],
    &[
        [0.00, 0.00],
        [2.08, 11.82],
        [3.11, 11.59],
        [3.86, 10.62],
        [3.59, 7.70],
    ],
    &[[0.00, 0.00], [3.59, 7.70], [4.10, 7.10], [4.36, 6.23]],
    &[[0.00, 0.00], [4.36, 6.23], [6.17, 7.35], [7.57, 7.57]],
    &[
        [0.00, 0.00],
        [7.57, 7.57],
        [9.96, 8.36],
        [11.55, 8.09],
        [12.56, 7.25],
        [13.59, 6.34],
        [14.10, 5.13],
        [14.30, 3.83],
        [14.28, 2.52],
        [13.05, 1.14],
        [10.30, -0.00],
        [6.87, -0.60],
    ],
    &[[0.00, 0.00], [6.87, -0.60], [7.58, -1.34]],
    &[[0.00, 0.00], [7.58, -1.34], [8.60, -2.30]],
    &[
        [0.00, 0.00],
        [8.60, -2.30],
        [12.12, -4.41],
        [14.14, -6.59],
        [15.33, -8.85],
        [11.96, -8.37],
        [7.89, -6.62],
    ],
    &[[0.00, 0.00], [7.89, -6.62], [6.36, -6.36], [4.89, -5.82]],
    &[[0.00, 0.00], [4.89, -5.82], [4.07, -5.82], [3.35, -5.80]],
];

/// Parts of obstacle pieces that stand outside the piece's collider, as (piece, footprint corners
/// in the piece frame); they block the walker as the piece. house_cottage_b's box collider is its
/// walls' (LOD2), so its porch posts get their own squares (`_house_slate.py` `porch`: 0.12 m oak
/// posts whose outer face is 0.75 m out from the front wall at x 2.2, 0.54 m either side of the
/// door), owner Q65. The walker still steps between them onto the doorstep.
pub const PIECE_POSTS: &[(&str, [[f32; 2]; 4])] = &[
    (
        "house_cottage_b",
        [[2.83, 0.48], [2.95, 0.48], [2.95, 0.60], [2.83, 0.60]],
    ),
    (
        "house_cottage_b",
        [[2.83, -0.60], [2.95, -0.60], [2.95, -0.48], [2.83, -0.48]],
    ),
];

/// Pieces whose walker collider is declared here as convex parts in the piece frame, in place of
/// the GLB collider's one hull: (piece, parts as `[x min, x max, z min, z max, top]`), each part
/// standing from the GLB collider's foot to its top. `barn_red_b`'s GLB collider is its LOD2 roof
/// box, which walled off its open lean-to shed; its parts follow the closed walls instead (owner
/// A1-Q1, 2026-10-20), so the walker enters the shed between the posts and stops at its back
/// wall. Measured on its LOD0 (`_barn_b.py`, Blender (x, y) is (x, -z) here): the closed barn
/// (gables at x +-5.0, plinth to +-5.05 and the door leaves to 5.08 on +x, the long wall's plinth
/// at z -3.55, the shed's back wall at z 1.0; up to the cupola's finial), the shed's end walls
/// (inner faces at x +-4.9 to the plinth, z 1.0..3.35, under the catslide at 3.6 m) and its four
/// 0.2 m posts on the post line (x +-1.633 and +-4.9, z 3.35..3.55, to the plate at 2.65 m).
///
/// The wells' GLB collider is the LOD2 hull, which reaches the flagstone apron's rim (1.385 m, so
/// grass and scatter keep off the apron: `scatter::FOOTPRINTS`); their parts keep the walker on the
/// apron instead (`_house_well.py`, measured on LOD0). `well_stone` is its roof's footprint (the
/// eaves and bargeboards to x +-0.9, the gable bargeboards and the crank to z +-1.09), so the eye at
/// 1.7 m never enters the eave at 1.52 m. `well_stone_b` has no roof: a stepped disc over its wall
/// and capstones (radius 0.79), its posts (0.17 m, centres on z +-0.8, to 2.16 m) and the crank on
/// the +z post (x -0.03..0.09, out to z 1.09, to 1.62 m).
pub const DECLARED_PARTS: &[(&str, &[[f32; 5]])] = &[
    (
        "barn_red_b",
        &[
            [-5.05, 5.08, -3.55, 1.0, 7.85],
            [4.9, 5.05, 1.0, 3.35, 3.6],
            [-5.05, -4.9, 1.0, 3.35, 3.6],
            [4.8, 5.0, 3.35, 3.55, 2.65],
            [1.533, 1.733, 3.35, 3.55, 2.65],
            [-1.733, -1.533, 3.35, 3.55, 2.65],
            [-5.0, -4.8, 3.35, 3.55, 2.65],
        ],
    ),
    ("well_stone", &[[-0.9, 0.9, -1.09, 1.09, 2.46]]),
    (
        "well_stone_b",
        &[
            [-0.79, 0.79, -0.3, 0.3, 0.9],
            [-0.74, 0.74, -0.5, 0.5, 0.9],
            [-0.61, 0.61, -0.61, 0.61, 0.9],
            [-0.5, 0.5, -0.74, 0.74, 0.9],
            [-0.3, 0.3, -0.79, 0.79, 0.9],
            [-0.085, 0.085, -0.885, 0.885, 2.16],
            [-0.03, 0.09, 0.885, 1.09, 1.62],
        ],
    ),
];

/// [`DECLARED_PARTS`] of `piece`, if it has any.
pub(super) fn declared_parts(piece: &str) -> Option<&'static [[f32; 5]]> {
    DECLARED_PARTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map(|(_, p)| *p)
}

/// Ground slots a car's wheels rest on: the road, and a level crossing's deck and rails. Kerbs,
/// verges, driveway gravel, hedges and gate bars never lift a car.
pub const CAR_GROUND: &[&str] = &[
    "road_asphalt",
    "road_paint",
    "rail_crossing_deck",
    "rail_head",
    "rail_steel",
];

/// What a prop is to the player (`data/pieces.ron` `label`).
pub use hex_core::piece::PropLabel;

/// What `piece` is to the player: the `data/pieces.ron` label of the piece or collider part. Every
/// piece that can block the walker or be clicked has one; an unlabelled one panics, and
/// `every_collider_piece_has_a_label` fails first for any obstacle, the [`CENTRE_OBSTACLES`] and
/// [`FOOTPRINT_OBSTACLES`] parts, and the boats.
pub fn prop_kind(piece: &str) -> PropLabel {
    PieceTable::builtin()
        .label(piece)
        .unwrap_or_else(|| panic!("{piece} has no label: give it one in data/pieces.ron"))
}
