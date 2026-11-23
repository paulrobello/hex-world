//! The ground a first-person walker stands on (PRD P8, D14): per placed tile, the topmost LOD0
//! surface of its ground pieces (the same surface scatter stands on), whether it holds water, and
//! the obstacles its kit colliders put in the way. `hex-app`'s walk controller moves through it with
//! [`WalkGround::step`].
//!
//! - **Ground.** A tile's ground is [`crate::play::scatter::Surface`] built from its base, centre and lane edge
//!   pieces (`scatter::ground_pieces`), sampled in the tile frame: meadow cap, road, beach slope,
//!   river banks and bed, the bridge deck, and hero props that are ground (`scatter::ground_props`:
//!   the station platform's paving and ramps). A tile whose ground pieces are outside the kit (a
//!   placeholder) stands at y = 0, the placeholder cap. Tiles hidden until their pack arrives
//!   ([`crate::play::PlayWaiting`]) are not ground at all.
//! - **Water.** On a tile with water (`water::tiles::water_kind`) the surface is [`crate::water::tiles::WATER_Y`]: the
//!   walker wades where the bed is at most [`MAX_WADE`] under it and stops at that contour. Rivers
//!   and spring streams ([`WaterKind::River`](crate::water::tiles::WaterKind::River), [`WaterKind::Spring`](crate::water::tiles::WaterKind::Spring)) are fords instead (owner,
//!   2026-10-20: "allow walk mode to cross rivers"): over deeper water the feet hold at
//!   [`MAX_WADE`] under the surface, so the walker crosses with its head above the water. The sea
//!   (ocean, coast and river mouth tiles) still stops it.
//! - **Obstacles.** Houses, the windmill, the barn, the station shelter, the hero oak, spruces,
//!   rocks and the farm's hay
//!   bales and (static) animals block the walker by their collider
//!   (`<piece>_collider` in the kit
//!   GLB) as a convex footprint on the ground, placed with the hero
//!   or scatter instance's transform (a hero's `y` lifts its volume, the shelter onto the paving)
//!   under the tile's rest transform. The paddock's wandering animals (`farm`) block as `animal`
//!   where they are drawn: `farm::pose` moves their footprints every frame
//!   ([`WalkGround::pose_herd`]). A collider of disjoint parts (the paddock fence's six run
//!   boxes, [`PieceShape::parts`]) blocks and is clicked part by part, so the walker enters the
//!   paddock through its gate and stands inside it. A part that stands outside its piece's
//!   collider blocks by its own footprint ([`piece_posts`]: house_cottage_b's porch posts).
//!   `barn_red_b`'s parts are declared (`data/pieces.ron` `declared_parts`): its closed walls and shed posts, so
//!   the walker enters its open lean-to (the shed's roof is not clicked; a ray must enter a
//!   part's prism). Wildflowers do not block, and the wheat
//!   field's wheat is neither ground nor obstacle (`data/pieces.ron` `not_ground`): the walker walks
//!   through it.
//! - **Props to look at.** Photo mode's click to focus ([`WalkGround::pick_prop`]) hits the same
//!   drawn obstacle instances as volumes: the collider footprint extruded from the collider's foot
//!   to the top of the piece's non-foliage LOD0 mesh, and for trees a crown of stacked cylinders
//!   fitted to the LOD0 foliage ([`PieceShape`]). Wildflowers are not pickable.
//! - **Edges.** A cell with no drawn tile is the map edge. The walker's body (a [`BODY_RADIUS`]
//!   circle) never overhangs an edge or deep water, and never climbs a step higher than
//!   [`MAX_STEP`] (a bridge railing, a house pad's lip if it were one).

mod ground;
mod shape;
mod sync;
mod tables;

#[cfg(test)]
pub(crate) mod tests;

pub use ground::{
    BODY_RADIUS, Blocked, MAX_STEP, MAX_WADE, PARKING_CORRIDOR, RIDE_FLAT, ROAD_CORRIDOR,
    STAGE_BOLLARDS, STAGE_DECK, Step, TileSurface, WalkGround, ride_max,
};
pub use shape::{
    CROWN_BANDS, CROWN_SECTORS, ColliderPart, Crown, Obstacle, PickProp, PieceShape, fallback_hull,
    hull, ray_prism, ray_triangle,
};
pub use sync::{WalkCache, sync};
pub use tables::{
    PropLabel, VIADUCT_PIER, centre_obstacles, footprint_obstacles, is_car_ground, is_obstacle,
    obstacles, piece_posts, prop_kind, walk_through,
};
