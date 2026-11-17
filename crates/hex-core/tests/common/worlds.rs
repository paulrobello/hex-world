//! Random worlds played through `hex_core`'s public API, shared by the save decoder's property
//! tests (`tests/save_props.rs`). The builder consults the world it is playing into, so every
//! accepted placement is one the game itself could reach; what the world refuses is skipped.

use std::collections::{BTreeMap, VecDeque};

use proptest::prelude::*;

use hex_core::{Axial, PlacedTile, TileCatalog, World};

/// The tile ids the generator places: the ground, tree, road and beach set the history-replay
/// oracle in `save.rs` plays with, plus a windmill and the granite massif (the mega tile).
pub const IDS: [&str; 7] = [
    "meadow",
    "forest",
    "oak",
    "road_straight",
    "beach",
    "windmill",
    "massif_granite",
];

/// One played action: the op kind, the pick of a frontier or occupied cell, the pick of a tile id
/// and orientation, and a spare. The ints are proptest-generated, so a failing case shrinks to a
/// short sequence of small numbers.
pub type Op = [u32; 4];

/// Up to 200 played actions per world.
pub fn arb_ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(any::<[u32; 4]>(), 0..200)
}

/// Plays `ops` into a fresh world, choosing among the cells and orientations the world reports as
/// legal (`frontier`, `legal_orientations`, `replace_orientations`, `can_bulldoze`); levels come
/// out of the legal orientation set, which enumerates `0..=MAX_LEVEL` for raisable tiles.
pub fn played_world(catalog: &TileCatalog, seed: u64, ops: &[Op]) -> World {
    let mut w = World::new(seed);
    for &[kind, pick, arg, spare] in ops {
        match kind % 6 {
            0 | 1 => {
                let slots = w.frontier();
                let cell = slots[(pick as usize) % slots.len()];
                let id = IDS[(arg as usize) % IDS.len()];
                let Some(def) = catalog.get(id) else { continue };
                let legal = w.legal_orientations(catalog, cell, def);
                let Some(o) = legal.get((spare as usize) % legal.len().max(1)).copied() else {
                    continue;
                };
                let _ = w.place(catalog, cell, id, o);
            }
            2 => {
                let keys: Vec<Axial> = w.tiles().keys().copied().collect();
                let here = keys[(pick as usize) % keys.len()];
                let id = IDS[(arg as usize) % IDS.len()];
                let Some(def) = catalog.get(id) else { continue };
                let os = w.replace_orientations(catalog, here, def);
                if let Some(o) = os.first().copied() {
                    let _ = w.replace(catalog, here, id, o);
                }
            }
            3 => {
                let keys: Vec<Axial> = w.tiles().keys().copied().collect();
                let here = keys[(pick as usize) % keys.len()];
                if w.can_bulldoze(catalog, here).is_ok() {
                    let _ = w.bulldoze(catalog, here);
                }
            }
            4 => {
                w.undo(catalog);
            }
            _ => {
                w.redo(catalog);
            }
        }
    }
    w
}

/// A valid save envelope around `world`: default camera and settings, no unlocks.
pub fn save_of(world: World) -> hex_core::SaveData {
    hex_core::SaveData {
        world,
        unlocks: Default::default(),
        camera: hex_core::CameraState {
            focus: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            distance: 100.0,
        },
        settings: hex_core::GameSettings {
            quality: hex_core::QualitySetting::Auto,
            tilt_shift: false,
            tilt_shift_chosen: false,
            tilt_shift_strength: 0.5,
            volumes: hex_core::AudioVolumes {
                master: 0.8,
                music: 0.5,
                sfx: 0.75,
                ambience: 0.7,
                ui: 0.5,
            },
        },
    }
}

/// `world` with every tile (and every mega tile's ring cells) shifted by `(dq, dr)`, and its
/// history dropped. Translation preserves sockets, connectivity and footprints, so the result is
/// legal wherever the original was; `World::from_parts` is the constructor decode itself uses.
pub fn translated(world: &World, dq: i32, dr: i32) -> World {
    let tiles: BTreeMap<Axial, PlacedTile> = world
        .tiles()
        .iter()
        .map(|(&c, t)| (Axial::new(c.q + dq, c.r + dr), t.clone()))
        .collect();
    World::from_parts(world.seed(), tiles, VecDeque::new(), Vec::new())
}

/// The smallest and largest `q` over every covered cell (tile anchors and mega rings).
pub fn q_extent(world: &hex_core::World, catalog: &TileCatalog) -> (i32, i32) {
    let covered = covered_cells(world, catalog);
    let min = covered.iter().map(|c| c.q).min().unwrap_or(0);
    let max = covered.iter().map(|c| c.q).max().unwrap_or(0);
    (min, max)
}

/// Every cell a world covers: tile anchors, and the ring cells of mega tiles.
pub fn covered_cells(world: &hex_core::World, catalog: &TileCatalog) -> Vec<hex_core::Axial> {
    let mut out: Vec<hex_core::Axial> = Vec::new();
    for (&cell, t) in world.tiles() {
        out.push(cell);
        if catalog.get(&t.def).is_some_and(|def| def.is_mega()) {
            out.extend(hex_core::footprint::members(cell));
        }
    }
    out
}
