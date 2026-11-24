//! Seeded prop scatter in play (PRD §5.3, §6): the pine forest's spruces and rocks, the oak tile's
//! rocks and flowers on the litter, and the meadow's wildflowers and rocks, from each tile's
//! `props.scatter` rules.
//!
//! - **Seed.** A tile scatters from `hex_core::cell_seed(world seed, cell)`, which is exactly its
//!   `PlacedTile::seed`. [`fn@layout`] is a pure function of the definition, that seed and the tile's
//!   ground, so a world always looks the same, and undo, redo, a reload or a late pack redraw a tile
//!   as it was.
//! - **Density.** `ScatterRule::density` is the mean count per 100 m² of the hex cap ([`CAP_AREA`],
//!   665 m²), so the forest's 3.0 is 20 trees a tile. The count is rounded stochastically from the
//!   seed, which keeps its mean over cells exact. Exclusions move instances onto the ground that is
//!   left and do not thin the count; where blue-noise spacing cannot fit it, the report's `placed`
//!   falls short of `planned`.
//! - **Drifts.** A rule with `drift` (the meadow's wildflowers, A18) grows only on the [`DRIFT_SHARE`]
//!   of its ground where the tile's seeded value noise ([`drift_noise`]) is highest, so its
//!   instances gather in a few clustered drifts. The noise is tile-local and seeded from the tile
//!   alone, so a tile needs no neighbour to scatter and redraws identically.
//! - **Variants.** A rule names a piece; each instance draws one piece of its group (its `data/pieces.ron`
//!   `variants`: the piece and its `_b`, `_c`, … variants, PRD §6 Variety), picked by [`pick_variant`] from a
//!   hash of the tile seed, the rule and the instance's place in it, so the dart stream is
//!   untouched and the same seed draws the same mix. A group draws its pieces equally unless
//!   its `variant_weights` weigh them (the verge bushes favour `bush_verge`), or on a road or rail
//!   tile its `verge_variant_weights` (more poppies on the verges, [`variant_weights`]). The rule's ground
//!   mask and spacing keep the group's largest footprint clear ([`group_footprint`]); each
//!   instance stands and clears others by its own. A rule draws once every piece of its group is in the kit.
//! - **Vary.** A rule with `vary` (the meadow's single wildflowers) spreads evenly but keeps a dart
//!   with probability 1 - vary × (1 - [`vary_noise`]), so its density varies gently over the cap
//!   while its count stays exact.
//! - **Beaches.** A tile that scatters only beach dressing ([`beach_family`]: beach, beach_inner,
//!   beach_outer and both river mouths) keeps a seeded 0 to [`BEACH_MAX`] of the pieces its rules
//!   place ([`beach_count`], owner 2026-10-21), picked jointly across its rules from a stream of
//!   its own, so a kept piece stands where it would among all of them and the dart stream is
//!   untouched. Its `planned` counts what it keeps.
//! - **Riverbanks.** Bank stone clusters and reed clumps ([`bank_band`]) stand in a band at a set
//!   distance from the river centreline, on the river bases' bed and bank grass ([`BANK_FLOORS`]).
//!   A tile that scatters only bank props ([`river_family`]: the river channel tiles) keeps a
//!   seeded 0 to [`RIVER_MAX`] of them ([`river_count`], owner 2026-10-26) the way a beach keeps
//!   its count; the river mouths keep theirs under the beach cap.
//! - **Seabed.** The ocean's rocks and pebble clusters ([`seabed`], owner 2026-10-26) stand on its
//!   floor ([`SEABED_FLOOR`]) at the bed's contract level ([`SEABED_Y`], under the floor's relief)
//!   and keep [`SEABED_BORDER`] inside the border, so the floor stays within its
//!   [`SEABED_ALLOWANCE`] (water contract §4).
//! - **Ground.** An instance stands only on its piece's floor slots ([`floor_slots`]) of the topmost
//!   LOD0 surface of the tile's base, centre and lane edge pieces, sampled the way the kit gate's
//!   `surface_at` does: trees on `forest_base_floor` (the litter), rocks and wildflowers on either
//!   that and `meadow_base_cap`. Its whole ground footprint must lie on that floor, so nothing
//!   stands on a road, a river bed or bank, a house pad or driveway, a beach, a rim, a skirt or water
//!   ([`excluded_slot`]), except the beach dressing on the dry sand ([`BEACH_FLOOR`],
//!   [`beach_band`]), the bank props on a river's bank ([`BANK_FLOORS`], [`bank_band`]) and the
//!   seabed dressing on the ocean floor ([`FLOOR_EXEMPT`]). It keeps clear of hero props (a tree
//!   clears the oak's canopy; a fence, [`fence`], by its line, dropped after the darts so nothing
//!   else moves), of road and
//!   river centrelines (`lane_clearance`, at least the lane's half width plus [`LANE_GAP`]) and of
//!   the hex border (a crown overhangs it by at most [`CROWN_OVERHANG`]), and stands upright at the
//!   surface height.
//! - **Spacing.** Dart throwing over the valid ground (Poisson-disc, never a grid): instances of one
//!   piece keep [`BLUE_NOISE`] of their mean spacing apart (trees at least [`CROWN_SPACING`] of a
//!   crown), and different pieces never share a footprint. Each gets a random yaw and a small scale
//!   jitter, both carried by its transform.
//! - **Drawing.** Instances are children of the tile root, which carries the tile's rotation, built
//!   from the kit's own parts (`spawn_kit_parts`): shared meshes and kit materials, so Bevy draws each
//!   (piece, LOD, slot) as one instanced batch per pass within the kit's LOD ranges, and no sampled
//!   texture is added. They grow in the rise's props stage in a ripple from the centre outward, and
//!   sink with the tile.
//! - **Streaming.** A tile scatters once its ground pieces are in the kit (the forest pack brings
//!   `forest_base`), and each rule draws once its piece is (the props pack brings rocks and
//!   wildflowers, the forest pack spruces). Until then the tile shows its placeholder or bare cap.

mod bands;
mod falls;
mod layout;
mod surface;
mod sync;
mod tables;

#[cfg(test)]
mod tests;

pub use bands::{
    BANK_FLOORS, BANK_REACH, BEACH_BORDER, BEACH_FLOOR, BEACH_MAX, BeachBand, DAMP_TOP_S,
    DRIFT_YAW_JITTER, FLOOR_EXEMPT, LILY_Y, MASSIF_CAP, PEAK_CAP, PEAK_PINE_BAND, PINE_BAND,
    REED_BED_BAND, RIVER_MAX, SEABED_ALLOWANCE, SEABED_BORDER, SEABED_FLOOR, SEABED_Y,
    SPRING_REACH, VERGE_BAND, WATER_BORDER, WATER_FLOOR, WATER_MAX, WATER_REST, bank_band,
    beach_band, beach_count, beach_family, coast_band, dressing_cap, excluded_slot, floor_ok,
    floor_slots, ring_band, river_count, river_family, seabed, spring_nodes, verge_band,
    water_count, water_prop,
};
pub use falls::{
    FALL_CLEAR, FALL_ROCK_RING, FALL_ROCKS_MAX, FallSeam, fall_rocks, fall_seams, seams_sig,
    with_falls,
};
pub use layout::{
    BLUE_NOISE, BORDER_GAP, CAP_AREA, CROWN_OVERHANG, CROWN_SPACING, DRIFT_CELL, DRIFT_PACK,
    DRIFT_SHARE, HERO_CROWN, HERO_GAP, Instance, LANE_GAP, Layout, Mask, Prepared, RING_GROUND,
    border_reach, drift_noise, fence_clearance, hero_clearance, hero_footprint, hex_dir, hex_reach,
    lane_clearance, lanes, layout, prepare, ring_seed, turn, vary_noise,
};
pub(crate) use layout::{Rng, polyline_distance};
pub use surface::{Surface, Tri, ground_pieces, member_tris};
pub(crate) use surface::{ground_tris, kit_tris};
pub use sync::{PlayScatter, ScatterGroup, ScatterLayouts, ScatterReport, TileScatter, sync};
pub use tables::{
    FENCE_HALF, Fence, Footprint, GroundBox, LANDFORMS, family, fence, fence_lines, footprint,
    ground_box, ground_props, group, group_footprint, is_ground_prop, is_not_ground, pick_variant,
    variant_piece, variant_weights, verge_tile,
};
