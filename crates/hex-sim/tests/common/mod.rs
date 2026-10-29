//! Shared layouts for the hex-sim integration tests. Every layout is a list of edits that is legal
//! for `hex_core::World`, so the tests exercise the same tile placements the game allows.

#![allow(dead_code)]

use std::collections::{BTreeMap, BinaryHeap};

use hex_core::{Axial, HEX_RADIUS, TileCatalog, World};
use hex_sim::{Network, NodeId, SimConfig, Simulation};

pub const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

pub fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Place(Axial, &'static str, u8),
    /// A placement at a level: cell, tile, rotation, level.
    PlaceAt(Axial, &'static str, u8, u8),
    Bulldoze(Axial),
}

/// A `World` and a `Simulation` kept in step: each edit must succeed on the world first, and the
/// sim's tunnels are recomputed after it, as the app does.
pub struct Scene {
    pub catalog: TileCatalog,
    pub world: World,
    pub sim: Simulation,
}

impl Scene {
    pub fn new(seed: u64, config: SimConfig) -> Self {
        let catalog = catalog();
        let world = World::new(seed);
        let sim = Simulation::from_world(&world, &catalog, config);
        Self {
            catalog,
            world,
            sim,
        }
    }

    pub fn with(seed: u64, config: SimConfig, edits: &[Edit]) -> Self {
        let mut scene = Self::new(seed, config);
        scene.apply_all(edits);
        scene
    }

    pub fn apply(&mut self, edit: &Edit) {
        match *edit {
            Edit::Place(cell, id, rot) => self.apply(&Edit::PlaceAt(cell, id, rot, 0)),
            Edit::PlaceAt(cell, id, rot, level) => {
                self.world
                    .place(
                        &self.catalog,
                        cell,
                        id,
                        hex_core::Orientation::new(rot, level),
                    )
                    .unwrap_or_else(|e| {
                        panic!("place {id} at {cell:?} rot {rot} level {level}: {e}")
                    });
                let def = self.catalog.get(id).expect("placed above");
                self.sim.apply_place(cell, def, rot);
                self.sim.sync_tunnels(&self.world, &self.catalog);
            }
            Edit::Bulldoze(cell) => {
                self.world
                    .bulldoze(&self.catalog, cell)
                    .unwrap_or_else(|e| panic!("bulldoze {cell:?}: {e}"));
                self.sim.apply_bulldoze(cell);
                self.sim.sync_tunnels(&self.world, &self.catalog);
            }
        }
    }

    /// Undoes the world's latest edit and mirrors the change into the sim.
    pub fn undo(&mut self) {
        let before = self.world.tiles().clone();
        assert!(self.world.undo(), "nothing to undo");
        self.mirror(&before);
    }

    /// Redoes the world's latest undone edit and mirrors the change into the sim.
    pub fn redo(&mut self) {
        let before = self.world.tiles().clone();
        assert!(self.world.redo(), "nothing to redo");
        self.mirror(&before);
    }

    /// Brings the sim from the tiles in `before` to the world's: a tile gone is a bulldoze, a new
    /// or changed one a place, then the tunnels are recomputed.
    fn mirror(&mut self, before: &BTreeMap<Axial, hex_core::PlacedTile>) {
        for cell in before.keys() {
            if self.world.tile(*cell).is_none() {
                self.sim.apply_bulldoze(*cell);
            }
        }
        for (cell, tile) in self.world.tiles() {
            if before.get(cell) != Some(tile) {
                let def = self.catalog.get(&tile.def).expect("builtin tile");
                self.sim.apply_place(*cell, def, tile.rotation);
            }
        }
        self.sim.sync_tunnels(&self.world, &self.catalog);
    }

    pub fn apply_all(&mut self, edits: &[Edit]) {
        for e in edits {
            self.apply(e);
        }
    }

    pub fn roads(&self) -> &Network {
        self.sim.network(hex_core::NetworkKind::Road)
    }
}

/// Places one tile straight into a network, bypassing world rules.
pub fn put(net: &mut Network, cat: &TileCatalog, cell: Axial, id: &str, rotation: u8) {
    net.insert_tile(cell, cat.get(id).expect("builtin tile"), rotation);
}

/// Houses at (1,0) and (3,0) joined by a road_straight at (2,0): 30.43 m between the house nodes.
pub fn house_chain() -> Vec<Edit> {
    vec![
        Edit::Place(Axial::new(1, 0), "house_cluster", 0),
        Edit::Place(Axial::new(2, 0), "road_straight", 0),
        Edit::Place(Axial::new(3, 0), "house_cluster", 3),
    ]
}

/// [`house_chain`] with meadows at (1,1) and (2,1), so the road tile can be bulldozed without
/// splitting the world.
pub fn house_chain_with_meadows() -> Vec<Edit> {
    let mut edits = house_chain();
    edits.push(Edit::Place(Axial::new(1, 1), "meadow", 0));
    edits.push(Edit::Place(Axial::new(2, 1), "meadow", 0));
    edits
}

/// The ring cell next to the origin across world edge `d`.
pub fn ring_cell(d: u8) -> Axial {
    Axial::ORIGIN.neighbor(d)
}

/// The house outside junction ring cell `d`.
pub fn ring_house(d: u8) -> Axial {
    ring_cell(d).neighbor(d)
}

/// A road loop through the six cells around the start meadow: road_bend_120 everywhere except a
/// road_junction at each cell in `junctions`, whose outward branch ends at a house.
pub fn ring(junctions: &[u8]) -> Vec<Edit> {
    let mut edits = vec![];
    for d in 0..6u8 {
        if junctions.contains(&d) {
            edits.push(Edit::Place(ring_cell(d), "road_junction", d));
        } else {
            edits.push(Edit::Place(ring_cell(d), "road_bend_120", (d + 2) % 6));
        }
    }
    for &d in junctions {
        edits.push(Edit::Place(ring_house(d), "house_cluster", (d + 3) % 6));
    }
    edits
}

/// A ring with houses at junctions 0, 2 and 4, plus a separate pair of houses at (4,0) and (5,0)
/// behind a meadow at (3,0): five destinations in two components.
pub fn ring_and_pair() -> Vec<Edit> {
    let mut edits = ring(&[0, 2, 4]);
    edits.push(Edit::Place(Axial::new(3, 0), "meadow", 0));
    edits.push(Edit::Place(Axial::new(4, 0), "house_cluster", 0));
    edits.push(Edit::Place(Axial::new(5, 0), "house_cluster", 3));
    edits
}

/// The house at the west end of [`tunnel_layout`].
pub const TUNNEL_HOUSE_A: Axial = Axial::new(1, 0);
/// The house at the east end of [`tunnel_layout`].
pub const TUNNEL_HOUSE_B: Axial = Axial::new(8, 0);
/// The west portal of [`tunnel_layout`], its mouth facing east.
pub const PORTAL_A: Axial = Axial::new(3, 0);
/// The east portal of [`tunnel_layout`], its mouth facing west.
pub const PORTAL_B: Axial = Axial::new(6, 0);

/// Two houses joined only through a tunnel along r = 0: house (1,0), road (2,0), portal (3,0),
/// level-1 meadow cover at (4,0) and (5,0), portal (6,0), road (7,0), house (8,0). The east half
/// is reached first by a road-free meadow detour along r = 2, so the cover at (5,0) is the last
/// edit and completes the tunnel.
pub fn tunnel_layout() -> Vec<Edit> {
    let mut edits = vec![
        Edit::Place(TUNNEL_HOUSE_A, "house_cluster", 0),
        Edit::Place(Axial::new(2, 0), "road_straight", 0),
        Edit::Place(PORTAL_A, "road_tunnel_portal", 0),
        Edit::Place(Axial::new(0, 1), "meadow", 0),
    ];
    for q in 0..=9 {
        edits.push(Edit::Place(Axial::new(q, 2), "meadow", 0));
    }
    edits.extend([
        Edit::Place(Axial::new(9, 1), "meadow", 0),
        Edit::Place(Axial::new(9, 0), "meadow", 0),
        Edit::Place(TUNNEL_HOUSE_B, "house_cluster", 3),
        Edit::Place(Axial::new(7, 0), "road_straight", 0),
        Edit::Place(PORTAL_B, "road_tunnel_portal", 3),
        Edit::PlaceAt(Axial::new(4, 0), "meadow", 0, 1),
        Edit::PlaceAt(Axial::new(5, 0), "meadow", 0, 1),
    ]);
    edits
}

/// SplitMix64 for test layouts, separate from the simulation's own generator.
pub struct TestRng(pub u64);

impl TestRng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// The next legal edit of a seeded random world walk: mostly placements that favor road tiles,
/// sometimes a bulldoze the world allows. `None` when the drawn cell has no legal tile.
pub fn random_edit(world: &World, cat: &TileCatalog, rng: &mut TestRng) -> Option<Edit> {
    if world.tiles().len() > 8 && rng.below(10) == 0 {
        let cells: Vec<Axial> = world.tiles().keys().copied().collect();
        let cell = cells[rng.below(cells.len())];
        return world
            .can_bulldoze(cat, cell)
            .ok()
            .map(|_| Edit::Bulldoze(cell));
    }
    let frontier = world.frontier();
    let cell = frontier[rng.below(frontier.len())];
    // These level-0 walks never place the hillside family, the road ramp or the portal; leaving
    // them out keeps every seeded walk as it was.
    let fits: Vec<&hex_core::TileDef> = world
        .inventory(cat, cell)
        .into_iter()
        .filter(|d| {
            !(d.id.starts_with("hillside_") || d.id == "road_ramp" || d.id == "road_tunnel_portal")
        })
        .collect();
    let roads: Vec<_> = fits
        .iter()
        .filter(|d| d.id.starts_with("road") || d.id == "house_cluster")
        .collect();
    let def = if !roads.is_empty() && rng.below(10) < 8 {
        roads[rng.below(roads.len())]
    } else if !fits.is_empty() {
        &fits[rng.below(fits.len())]
    } else {
        return None;
    };
    let rots: Vec<u8> = world
        .legal_orientations(cat, cell, def)
        .into_iter()
        .filter(|o| o.level == 0)
        .map(|o| o.rotation)
        .collect();
    let rot = rots[rng.below(rots.len())];
    let (id, _) = hex_core::BUILTIN_TILES
        .iter()
        .find(|(id, _)| *id == def.id)
        .expect("builtin tile");
    Some(Edit::Place(cell, id, rot))
}

/// Plain Dijkstra over the whole graph, as an oracle for A*: distance from `from` to every
/// reachable node.
pub fn dijkstra(net: &Network, from: NodeId) -> BTreeMap<NodeId, f32> {
    #[derive(PartialEq)]
    struct Item(f32, NodeId);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Item {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
        }
    }
    let mut dist: BTreeMap<NodeId, f32> = BTreeMap::new();
    let mut heap = BinaryHeap::from([Item(0.0, from)]);
    while let Some(Item(d, n)) = heap.pop() {
        if dist.contains_key(&n) {
            continue;
        }
        dist.insert(n, d);
        for link in net.node(n).expect("live node").links() {
            let l = net.link(*link).expect("live link");
            let next = l.other(n);
            if !dist.contains_key(&next) {
                heap.push(Item(d + l.length(), next));
            }
        }
    }
    dist
}
