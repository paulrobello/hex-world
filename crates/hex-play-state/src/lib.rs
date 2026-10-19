//! Shared presentation data. No game rules, plugins, or renderer dependency.
use bevy::prelude::*;
use hex_core::{Axial, EdgeStatus, PairPieces, TileCatalog};
use std::collections::{HashMap, HashSet};
/// The play systems (kit loading, the view sync, the rise and the affordances), in order. `hex-app`
/// orders its view writer before this set and the car mirror after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaySystems;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayTile {
    pub cell: Axial,
    pub tile: String,
    pub rotation: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayGhost {
    pub cell: Axial,
    pub tile: String,
    pub rotation: u8,
    /// Status per world edge 0..6.
    pub edges: [EdgeStatus; 6],
    /// False when the tile does not fit here at this rotation.
    pub valid: bool,
    /// The ghost sits on a pinned slot (U4) rather than under the pointer.
    pub pinned: bool,
}

/// What the play scenario draws, written by `hex-app` when the game changes.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PlayView {
    pub tiles: Vec<PlayTile>,
    pub frontier: Vec<Axial>,
    pub ghost: Option<PlayGhost>,
    /// A free slot under the pointer while another slot is pinned: a click there pins it.
    pub pointer_slot: Option<Axial>,
    /// A placed tile under the pointer with the place tool, outside photo mode: a faint rim marks it.
    pub pointer_tile: Option<Axial>,
    /// Bulldoze tool: the hovered tile and whether removing it is allowed.
    pub bulldoze: Option<(Axial, bool)>,
    /// The bulldoze tool is active (the slot ring hides).
    pub bulldoze_mode: bool,
    /// The cell the latest world change bulldozed: when its tile leaves the view it sinks inside a
    /// dust cloud (the bulldoze sink) rather than with the undo puff.
    pub bulldozed: Option<Axial>,
    /// Refused placements and bulldozes so far; each new one pulses the refused highlight.
    pub refusals: u32,
    /// Bumped when the world is replaced wholesale (a loaded save, a new world): the next sync
    /// redraws every tile settled, water filled and agents ready, with no rise, sink or cue.
    /// Placements in the new world rise as usual.
    pub epoch: u64,
    /// The world seed: each tile scatters from it and its cell (scatter presentation).
    pub world_seed: u64,
}

/// Tile definitions the view's ids resolve against (the embedded catalog `hex-app` also uses).
#[derive(Resource)]
pub struct PlayCatalog(pub TileCatalog);

impl Default for PlayCatalog {
    fn default() -> Self {
        Self(TileCatalog::builtin().expect("the embedded catalog is validated by hex-core"))
    }
}

/// Root of one placed tile's entities.
#[derive(Component)]
pub struct PlayTileRoot;

/// Entities spawned for the current view. `setup` replaces it on every switch into the scenario,
/// since the switch despawned the previous scene.
#[derive(Resource, Default)]
pub struct PlaySpawned {
    pub tiles: HashMap<Axial, (PlayTile, Entity)>,
    pub rebuild: bool,
    /// Pair edge pieces (`data/sockets.ron` `pair_edge_pieces`, decision D3) resolved per cell,
    /// and the entities drawing them under each tile root. They depend on the neighbours, so a
    /// change at a cell re-evaluates it and its six neighbours ([`PairPieces::update`]).
    pub pairs: PairPieces,
    pub pair_entities: HashMap<Axial, Vec<Entity>>,
    /// The view epoch the spawned tiles belong to.
    pub epoch: u64,
    /// Pieces built since the last sync: tiles drawing them swap to the kit look in place.
    pub refresh: HashSet<String>,
    /// Placed tiles hidden until every pack they draw from arrives (owner decision A10), with
    /// whether they rise on arrival (a placement) or appear at rest (a rebuild or loaded world).
    pub hidden: HashMap<Axial, bool>,
}

/// Placed tiles hidden until their pack arrives, sorted by cell: the empty slot shows a soft
/// pulsing outline instead (play affordances).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PlayWaiting(pub Vec<Axial>);

#[derive(Clone)]
pub struct KitPart<Mat: bevy::pbr::Material> {
    pub lod: usize,
    pub mesh: Handle<Mesh>,
    pub material: Handle<Mat>,
    /// Its manifest material slot (`beach_sand`, `river_bed`, ...).
    pub slot: String,
    /// A rotor part (PRD §7.2): the `<id>_rotor_lod<n>` node's rest transform in the piece frame. It
    /// draws there under its LOD's body and turns about its local X (the renderer rotor component).
    pub rotor: Option<Transform>,
}

/// The built kit pieces. `Loading` holds pieces of packs that arrived before core; `Ready` is
/// core and every pack since (pack streaming). A piece missing from the manifest (pending, plan F4),
/// failing to load, or in a pack still streaming has no entry and draws as a placeholder.
#[derive(Resource)]
pub enum PlayKit<Mat: bevy::pbr::Material> {
    Loading(HashMap<String, Vec<KitPart<Mat>>>),
    Ready(HashMap<String, Vec<KitPart<Mat>>>),
}

impl PlaySpawned {
    /// Whether this sync redraws every tile at rest: a scenario switch or core arriving set
    /// `rebuild`, and a new view epoch (a replaced world) sets it here.
    pub fn rebuild_for(&mut self, epoch: u64) -> bool {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.rebuild = true;
        }
        self.rebuild
    }

    /// A redraw is queued that the next sync has not spawned yet.
    pub fn redraw_pending(&self) -> bool {
        self.rebuild
    }
}

/// The grass density (Settings → Graphics → Grass density, the `grass` key, `set_grass`) in whole
/// percent of the normal blade count: 0 is off, 100 Normal, 150 the most ([`LUSH`]). Every tile seeds the Lush
/// set once; a density draws the blades whose `keep` is under its share of Lush, so every lower value
/// draws a subset of every higher one and a change only changes each group's drawn instance count
/// (never the scene, never an upload).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Density(u8);

impl Default for Density {
    fn default() -> Self {
        Density::NORMAL
    }
}

impl Density {
    pub const OFF: Density = Density(0);
    pub const NORMAL: Density = Density(100);
    pub const MAX: Density = Density(150);
    /// The stepped levels the slider replaced, kept as `grass` aliases: name and percent.
    pub const ALIASES: [(&'static str, u8); 4] =
        [("off", 0), ("sparse", 50), ("normal", 100), ("lush", 150)];

    /// A share of the normal blade count, clamped to 0..=[`LUSH`] and rounded to whole percent; `None` when it
    /// is not finite.
    pub fn from_share(share: f32) -> Option<Density> {
        share
            .is_finite()
            .then(|| Density((share.clamp(0.0, LUSH) * 100.0).round() as u8))
    }

    /// A share (`1.2`), a percentage (`120%`), or an alias (`off|sparse|normal|lush`, `on` is
    /// Normal). Out-of-range numbers clamp.
    pub fn parse(s: &str) -> Option<Density> {
        let s = s.trim();
        if s == "on" {
            return Some(Density::NORMAL);
        }
        if let Some((_, p)) = Self::ALIASES.iter().find(|(n, _)| *n == s) {
            return Some(Density(*p));
        }
        let (num, scale) = match s.strip_suffix('%') {
            Some(p) => (p.trim(), 0.01),
            None => (s, 1.0),
        };
        Density::from_share(num.parse::<f32>().ok()? * scale)
    }

    pub fn percent(self) -> u8 {
        self.0
    }

    /// Share of the normal blade count.
    pub fn share(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    pub fn is_off(self) -> bool {
        self.0 == 0
    }

    /// Whether a blade with this `keep` draws at this density.
    pub fn draws(self, keep: f32) -> bool {
        keep * LUSH * 100.0 < f32::from(self.0)
    }
}

/// The Lush share of the normal blade count: the seeded set every density draws from, and the slider's top.
pub const LUSH: f32 = 1.5;

/// The grass draw distance (Settings → Graphics → Grass distance, the `grass_distance` key,
/// `set_grass_distance`, owner request 2026-10-19) in hundredths of a multiplier on every grass
/// distance (thinning, fade, build and drop radii): 100 is Near, the tuning grass shipped with, and
/// the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GrassDistance(u16);

impl Default for GrassDistance {
    fn default() -> Self {
        GrassDistance::NEAR
    }
}

impl GrassDistance {
    pub const NEAR: GrassDistance = GrassDistance(100);
    pub const MAX: GrassDistance = GrassDistance(300);
    /// The named stops the Settings slider offers, nearest first: name and hundredths.
    pub const ALIASES: [(&'static str, u16); 4] = [
        ("near", 100),
        ("medium", 150),
        ("far", 200),
        ("very-far", 300),
    ];

    /// A multiplier clamped to Near..=[`Self::MAX`] and rounded to hundredths; `None` when it is
    /// not finite.
    pub fn from_scale(scale: f32) -> Option<GrassDistance> {
        let (lo, hi) = (Self::NEAR.scale(), Self::MAX.scale());
        scale
            .is_finite()
            .then(|| GrassDistance((scale.clamp(lo, hi) * 100.0).round() as u16))
    }

    /// A stop name (`near|medium|far|very-far`) or a multiplier (`2`, `1.5`). Out-of-range numbers
    /// clamp.
    pub fn parse(s: &str) -> Option<GrassDistance> {
        let s = s.trim();
        if let Some((_, h)) = Self::ALIASES.iter().find(|(n, _)| *n == s) {
            return Some(GrassDistance(*h));
        }
        GrassDistance::from_scale(s.parse::<f32>().ok()?)
    }

    pub fn hundredths(self) -> u16 {
        self.0
    }

    /// The multiplier on every grass distance.
    pub fn scale(self) -> f32 {
        f32::from(self.0) / 100.0
    }
}

/// How a part of a placed tile moves during its rise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RiseRole {
    /// The tile root: the column and cap, and every child that has no role of its own.
    Root,
    /// A centre or edge piece (roads, lanes, pads): snaps in at the edges stage.
    Edge,
    /// A hero prop: grows at the props stage, `order` of `count`.
    Prop {
        order: u8,
        count: u8,
        kind: PropKind,
    },
    /// A water tile's placeholder bed: moves with the column.
    Bed,
    /// A water surface or cut face: moves with the column and fills at the edges stage.
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropKind {
    Tree,
    Building { floors: u8 },
}

/// A tile part the rise animates, with its rest transform.
#[derive(Component, Clone, Copy, Debug)]
pub struct RisePart {
    pub cell: Axial,
    pub role: RiseRole,
    pub rest: Transform,
}

impl RisePart {
    pub fn new(cell: Axial, role: RiseRole, rest: Transform) -> Self {
        Self { cell, role, rest }
    }
}
