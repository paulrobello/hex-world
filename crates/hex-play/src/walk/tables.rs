use hex_core::piece::{CentreObstacle, FootprintObstacle, PieceTable, WalkClass};

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

/// The parts of centre pieces that block the walker (`data/pieces.ron` `centre_obstacles`: the
/// junction's point lever, a rail viaduct's abutments and piers).
pub fn centre_obstacles() -> &'static [CentreObstacle] {
    &PieceTable::builtin().centre_obstacles
}

/// The [`centre_obstacles`] part of a rail viaduct's abutments and piers: grass keeps off them too
/// (`grass::exclusions`).
pub const VIADUCT_PIER: &str = "rail_viaduct_pier";

/// Landform tiles' standing rock that blocks the walker (`data/pieces.ron`
/// `footprint_obstacles`: the massif's, the hill's and the peak's rock feet as convex parts).
pub fn footprint_obstacles() -> &'static [FootprintObstacle] {
    &PieceTable::builtin().footprint_obstacles
}

/// The squares of `piece` that stand outside its collider and block the walker as the piece
/// (`data/pieces.ron` `posts`: house_cottage_b's porch posts).
pub fn piece_posts(piece: &str) -> &'static [[[f32; 2]; 4]] {
    PieceTable::builtin().get(piece).map_or(&[], |p| &p.posts)
}

/// The walker collider parts declared for `piece` (`data/pieces.ron` `declared_parts`), if any.
pub(super) fn declared_parts(piece: &str) -> Option<&'static [[f32; 5]]> {
    PieceTable::builtin().get(piece)?.declared_parts.as_deref()
}

/// Whether ground slot `slot` carries a car's wheels (`data/pieces.ron` `car_ground`: the road,
/// and a level crossing's deck and rails).
pub fn is_car_ground(slot: &str) -> bool {
    PieceTable::builtin().is_car_ground(slot)
}

/// What a prop is to the player (`data/pieces.ron` `label`).
pub use hex_core::piece::PropLabel;

/// What `piece` is to the player: the `data/pieces.ron` label of the piece or collider part. Every
/// piece that can block the walker or be clicked has one; an unlabelled one panics, and
/// `every_collider_piece_has_a_label` fails first for any obstacle, the [`centre_obstacles`] and
/// [`footprint_obstacles`] parts, and the boats.
pub fn prop_kind(piece: &str) -> PropLabel {
    PieceTable::builtin()
        .label(piece)
        .unwrap_or_else(|| panic!("{piece} has no label: give it one in data/pieces.ron"))
}
