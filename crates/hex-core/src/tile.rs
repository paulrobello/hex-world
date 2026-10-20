//! Tile definitions (PRD §5.3) and tile rotation (PRD §5.1).
//!
//! Positions inside a tile are tile-local meters `(x, z)` before rotation, centered on the cell,
//! with +x toward edge 0 and +z toward edge 1 (`axial.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::socket::{EdgeSocket, SocketKind, SocketTable};

/// Id of a kit piece in `assets/kit/manifest.json` (or `data/pending-pieces.ron` until it is built).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KitRef(pub String);

impl KitRef {
    /// The kit piece id without allocating.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for KitRef {
    fn from(s: &str) -> Self {
        KitRef(s.to_owned())
    }
}

/// Local edge `local` of a tile placed with `rotation` lands on world edge `(local + rotation) mod 6`.
pub fn world_edge(local: u8, rotation: u8) -> u8 {
    (local % 6 + rotation % 6) % 6
}

/// Inverse of [`world_edge`]: the local edge that faces world edge `world` under `rotation`.
pub fn local_edge(world: u8, rotation: u8) -> u8 {
    (world % 6 + 6 - rotation % 6) % 6
}

/// The world edge of the neighbor across world edge `edge`.
pub fn opposite_edge(edge: u8) -> u8 {
    (edge % 6 + 3) % 6
}

/// Terrain classification authored on a tile definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Grass,
    Forest,
    Beach,
    Ocean,
}

/// Edge pieces of a tile: derived from its sockets (`data/sockets.ron` `edge_pieces`) unless listed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum EdgePieces {
    #[default]
    Derived,
    Explicit([Option<KitRef>; 6]),
}

/// Seeded scatter of one prop over the tile, using the placed tile's seed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScatterRule {
    pub piece: KitRef,
    /// Mean instances per 100 m² of cap area.
    pub density: f32,
    /// Keep-out distance from network centerlines, in meters.
    #[serde(default)]
    pub lane_clearance: f32,
    /// Grow in clustered drifts from the tile's seeded noise instead of spreading evenly.
    #[serde(default)]
    pub drift: bool,
    /// Spread evenly but thinned by the tile's seeded noise, in [0, 1]: a placement is kept with
    /// probability 1 - vary * (1 - noise), so the density varies gently over the cap while the
    /// count stays `density`. 0 (the default) is an even spread.
    #[serde(default)]
    pub vary: f32,
}

/// A prop at a fixed tile-local position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeroProp {
    pub piece: KitRef,
    pub at: [f32; 2],
    #[serde(default)]
    pub yaw_deg: f32,
    /// Height of the piece's origin over the tile frame (m): 0 for a piece on the ground; a
    /// station shelter stands on its platform's paving.
    #[serde(default)]
    pub y: f32,
}

/// Seeded scatter rules and fixed hero props authored for a tile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PropScatter {
    #[serde(default)]
    pub scatter: Vec<ScatterRule>,
    #[serde(default)]
    pub hero: Vec<HeroProp>,
}

/// Road, river or rail connectivity carried by a network segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NetworkKind {
    Road,
    River,
    Rail,
}

impl NetworkKind {
    pub const ALL: [NetworkKind; 3] = [NetworkKind::Road, NetworkKind::River, NetworkKind::Rail];

    /// The socket kind whose lanes this network crosses.
    pub fn socket(self) -> SocketKind {
        match self {
            NetworkKind::Road => SocketKind::Road,
            NetworkKind::River => SocketKind::River,
            NetworkKind::Rail => SocketKind::Rail,
        }
    }
}

/// A rail turnout (PRD §7.1, P3): three rail segments meet at its toe node. A train running in on
/// the `toe` segment leaves on `straight` or `branch` as the switch is set; from either leg it
/// trails through onto the toe segment. No route joins the two legs, so a train never turns back
/// through the vee.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwitchDef {
    /// The node in [`TileDef::nodes`] where the three segments meet (the blade tips).
    pub node: String,
    /// Indices into [`TileDef::networks`]: the segment from the toe edge to `node`, and the two
    /// legs from `node`.
    pub toe: u16,
    pub straight: u16,
    pub branch: u16,
    /// Distance in meters from the toe edge's border, along either leg, past which a train clears
    /// a train on the other leg (`rail_junction_center` friction notes: the centrelines are 5.0 m
    /// apart there). A waiting train stands beyond it.
    pub fouling: f32,
}

/// A station stop (P3): the platform's level section beside rail segment `segment`, as distances
/// in meters along it from its `from` end. A train stops there with every axle of its consist on
/// the level section, waits, and runs on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopDef {
    pub segment: u16,
    pub level: [f32; 2],
}

/// End of a network segment: the lane on a local edge, or a node declared in [`TileDef::nodes`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Endpoint {
    Edge(u8),
    Node(String),
}

/// Destination tag that agents route to (PRD §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Destination {
    /// Houses own the cars, which park there between trips.
    House,
    /// A shop (the bakery) owns no cars: house cars visit, park for a short stay and leave.
    Shop,
}

/// A named point inside the tile: a junction, or a tagged destination.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkNode {
    pub id: String,
    pub at: [f32; 2],
    #[serde(default)]
    pub destination: Option<Destination>,
}

/// One road or river graph edge inside the tile (PRD §7.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkSegment {
    pub kind: NetworkKind,
    pub from: Endpoint,
    pub to: Endpoint,
    /// Interior control points, ordered from `from` to `to`, in tile-local meters.
    #[serde(default)]
    pub points: Vec<[f32; 2]>,
}

/// A tile-local pose on a parking path: position in meters and the heading the car faces, in
/// degrees from +x toward +z (the hero prop `yaw_deg` convention).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkPose {
    pub at: [f32; 2],
    pub yaw_deg: f32,
}

/// A parking spot (PRD §7.1) at a house or a shop, and the short paths between it and the tile's
/// road node.
///
/// A car parks at `at` facing `yaw_deg`. It arrives forward from the node's inbound lane through
/// `enter` into the spot. It leaves by reversing through `back` (the last pose is where it stops
/// to change direction; empty means it pulls out forward), then drives forward through `leave` onto
/// the node's outbound lane. A spot on a through road can also give `back_alt`, a reverse that
/// swings the other way, for leaving in the other direction; the sim takes whichever reverse turns
/// least. The sim joins and leaves the lane at the node when the turn there is gentle, else a
/// little past the node (or before it, arriving), so the car does not turn on the spot. Consecutive
/// poses are joined by cubic Hermite curves whose end tangents follow the poses' headings, so every
/// pose sets the direction of travel there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkingSpot {
    /// `park-<n>`, n = the spot's index in [`TileDef::parking`]; the tile's centre piece carries a
    /// matching `anchor-park-<n>` socket.
    pub id: String,
    /// Index into [`PropScatter::hero`] of the building the spot serves: at a house destination,
    /// the house that owns the spot's car.
    pub house: u16,
    /// The destination node in [`TileDef::nodes`] the paths join.
    pub node: String,
    pub at: [f32; 2],
    pub yaw_deg: f32,
    #[serde(default)]
    pub enter: Vec<ParkPose>,
    #[serde(default)]
    pub back: Vec<ParkPose>,
    #[serde(default)]
    pub back_alt: Vec<ParkPose>,
    #[serde(default)]
    pub leave: Vec<ParkPose>,
}

impl ParkingSpot {
    /// The spot's own pose.
    pub fn pose(&self) -> ParkPose {
        ParkPose {
            at: self.at,
            yaw_deg: self.yaw_deg,
        }
    }
}

/// A positional ambience source; the cue id names an audio asset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioEmitter {
    pub cue: String,
    pub at: [f32; 2],
    pub radius: f32,
}

/// Agent family a tile contributes through its spawn rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentKind {
    Car,
    Bird,
}

/// Agents this tile contributes once its rise finishes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentSpawnRule {
    pub agent: AgentKind,
    pub max: u16,
}

/// The staged rise phases of PRD §4.4, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiseStage {
    Column,
    Cap,
    Edges,
    Props,
    Agents,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiseStep {
    pub stage: RiseStage,
    /// Seconds from the start of the rise.
    pub start: f32,
    pub duration: f32,
}

/// Allowed total rise length in seconds (PRD §4.4).
pub const RISE_SECONDS: RangeInclusive<f32> = 1.5..=2.5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiseTimeline {
    pub steps: Vec<RiseStep>,
}

impl Default for RiseTimeline {
    /// The standard 2.0 s rise shared by tiles that do not author their own.
    fn default() -> Self {
        let step = |stage, start, duration| RiseStep {
            stage,
            start,
            duration,
        };
        Self {
            steps: vec![
                step(RiseStage::Column, 0.0, 0.6),
                step(RiseStage::Cap, 0.45, 0.4),
                step(RiseStage::Edges, 0.8, 0.4),
                step(RiseStage::Props, 1.1, 0.6),
                step(RiseStage::Agents, 1.6, 0.4),
            ],
        }
    }
}

impl RiseTimeline {
    /// End of the last step, in seconds.
    pub fn total(&self) -> f32 {
        self.steps
            .iter()
            .map(|s| s.start + s.duration)
            .fold(0.0, f32::max)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unlock {
    #[default]
    Default,
    Recipe(String),
}

/// One tile type, loaded from `data/tiles/<id>.ron` (PRD §5.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileDef {
    pub id: String,
    pub biome: Biome,
    pub base: KitRef,
    /// Indexed by local edge (0 = east, clockwise viewed from +Y).
    pub edges: [EdgeSocket; 6],
    #[serde(default)]
    pub center: Option<KitRef>,
    #[serde(default)]
    pub edge_pieces: EdgePieces,
    #[serde(default)]
    pub props: PropScatter,
    #[serde(default)]
    pub nodes: Vec<NetworkNode>,
    #[serde(default)]
    pub networks: Vec<NetworkSegment>,
    /// Rail turnouts at nodes where three rail segments meet.
    #[serde(default)]
    pub switches: Vec<SwitchDef>,
    /// Station stops on rail segments.
    #[serde(default)]
    pub stops: Vec<StopDef>,
    /// One spot per house that owns a car, or a shop's visitor spots (PRD §7.1).
    #[serde(default)]
    pub parking: Vec<ParkingSpot>,
    #[serde(default)]
    pub emitters: Vec<AudioEmitter>,
    #[serde(default)]
    pub agents: Vec<AgentSpawnRule>,
    #[serde(default)]
    pub rise: RiseTimeline,
    #[serde(default)]
    pub unlock: Unlock,
    /// The centre's look repeats every this many rotations, though its sockets may repeat sooner
    /// (the wheat field's parallel rows, symmetric under a half turn: 3). `None` leaves the
    /// sockets alone to decide ([`TileDef::orientation_period`]). Divides 6.
    #[serde(default)]
    pub look_period: Option<u8>,
}

impl TileDef {
    /// The socket facing world edge `world` when the tile is placed with `rotation`.
    pub fn edge(&self, world: u8, rotation: u8) -> &EdgeSocket {
        &self.edges[local_edge(world, rotation) as usize]
    }

    /// Whether rotations `a` and `b` give every world edge an equal socket (kind, lane, land side,
    /// height, profile): one orientation, as a symmetric tile turned by its symmetry (PRD §4.7 Q6).
    /// Nor do two rotations that are not a whole number of [`TileDef::orientation_period`]s apart:
    /// a tile with hero props never is (unless `a` and `b` are one rotation), as its heroes turn
    /// with it (owner, 2026-10-20: the windmill's sails, the farm's barn), and the wheat field is
    /// only every third rotation, as its rows lie three ways.
    pub fn same_orientation(&self, a: u8, b: u8) -> bool {
        (i16::from(a % 6) - i16::from(b % 6)) % i16::from(self.orientation_period()) == 0
            && (0..6).all(|w| self.edge(w, a) == self.edge(w, b))
    }

    /// Rotations after which the tile's look repeats, whatever its sockets do: 6 with hero props
    /// (they turn with the tile), the definition's `look_period` where it sets one (owner D5,
    /// 2026-10-20: the wheat field's rows run three ways), else 1.
    pub fn orientation_period(&self) -> u8 {
        if !self.props.hero.is_empty() {
            return 6;
        }
        self.look_period
            .filter(|p| matches!(p, 1 | 2 | 3 | 6))
            .unwrap_or(1)
    }

    /// Edge pieces by local edge, resolving [`EdgePieces::Derived`] through the socket table.
    pub fn edge_pieces<'a>(&'a self, sockets: &'a SocketTable) -> [Option<&'a KitRef>; 6] {
        match &self.edge_pieces {
            EdgePieces::Derived => std::array::from_fn(|e| sockets.edge_piece(self.edges[e].kind)),
            EdgePieces::Explicit(pieces) => std::array::from_fn(|e| pieces[e].as_ref()),
        }
    }

    /// The car cap of the tile's [`AgentKind::Car`] spawn rules, 0 without one.
    pub fn car_cap(&self) -> usize {
        self.agents
            .iter()
            .filter(|a| a.agent == AgentKind::Car)
            .map(|a| usize::from(a.max))
            .sum()
    }

    /// Every kit piece the tile references: base, center, edge pieces and props, with every
    /// variant a hero slot may draw ([`TileDef::hero_options`]).
    pub fn kit_refs<'a>(&'a self, sockets: &'a SocketTable) -> BTreeSet<&'a str> {
        let mut refs: BTreeSet<&str> = BTreeSet::new();
        refs.insert(self.base.as_str());
        refs.extend(self.center.iter().map(KitRef::as_str));
        refs.extend(
            self.edge_pieces(sockets)
                .into_iter()
                .flatten()
                .map(KitRef::as_str),
        );
        refs.extend(self.props.scatter.iter().map(|s| s.piece.as_str()));
        refs.extend((0..self.props.hero.len()).flat_map(|i| self.hero_options(i)));
        refs
    }

    /// Rule problems in this definition, each prefixed with the tile id. Empty means valid.
    ///
    /// Checks the lane contract (a lane exactly on lane-kind edges, equal to the table's lane), the
    /// land side (exactly on directional edges),
    /// network endpoints (each lane edge is the endpoint of exactly one segment of the matching
    /// kind, and no endpoint sits on a non-lane edge or names an undeclared node) and the rise
    /// length.
    pub fn validate(&self, sockets: &SocketTable) -> Vec<String> {
        let mut issues = vec![];
        let mut issue = |msg: String| issues.push(format!("{}: {msg}", self.id));

        for (e, edge) in self.edges.iter().enumerate() {
            match (sockets.lane(edge.kind), edge.lane) {
                (Some(want), Some(got)) if want != got => issue(format!(
                    "edge {e} {:?} lane {got:?} differs from sockets.ron {want:?}",
                    edge.kind
                )),
                (Some(_), None) => issue(format!("edge {e} {:?} needs a lane", edge.kind)),
                (None, Some(_)) => issue(format!("edge {e} {:?} cannot carry a lane", edge.kind)),
                _ => {}
            }
            match (sockets.directional(edge.kind), edge.land) {
                (true, None) => issue(format!("edge {e} {:?} needs a land side", edge.kind)),
                (false, Some(_)) => {
                    issue(format!("edge {e} {:?} cannot carry a land side", edge.kind))
                }
                _ => {}
            }
        }

        let mut node_ids: BTreeSet<&str> = BTreeSet::new();
        for node in &self.nodes {
            if !node_ids.insert(node.id.as_str()) {
                issue(format!("node {:?} is declared twice", node.id));
            }
        }

        let mut endpoints: BTreeMap<u8, usize> = BTreeMap::new();
        for (i, seg) in self.networks.iter().enumerate() {
            if seg.from == seg.to {
                issue(format!("segment {i} starts and ends at {:?}", seg.from));
            }
            for end in [&seg.from, &seg.to] {
                match end {
                    Endpoint::Edge(e) if *e >= 6 => {
                        issue(format!("segment {i} edge {e} is not 0..6"))
                    }
                    Endpoint::Edge(e) => {
                        let edge = &self.edges[*e as usize];
                        if edge.lane.is_none() || sockets.lane(edge.kind).is_none() {
                            issue(format!(
                                "segment {i} ends on edge {e} {:?}, which has no lane",
                                edge.kind
                            ));
                        } else if edge.kind != seg.kind.socket() {
                            issue(format!(
                                "segment {i} is {:?} but edge {e} is {:?}",
                                seg.kind, edge.kind
                            ));
                        }
                        *endpoints.entry(*e).or_default() += 1;
                    }
                    Endpoint::Node(id) if !node_ids.contains(id.as_str()) => {
                        issue(format!("segment {i} names undeclared node {id:?}"))
                    }
                    Endpoint::Node(_) => {}
                }
            }
        }
        for (e, edge) in self.edges.iter().enumerate() {
            let count = endpoints.get(&(e as u8)).copied().unwrap_or(0);
            if sockets.lane(edge.kind).is_some() && count != 1 {
                issue(format!(
                    "lane edge {e} has {count} network endpoints, needs exactly 1"
                ));
            }
        }

        self.validate_destinations(&mut issue);
        self.validate_parking(&mut issue);
        self.validate_rail(&mut issue);

        if let Some(p) = self.look_period
            && !matches!(p, 1 | 2 | 3 | 6)
        {
            issue(format!("look_period {p} does not divide 6"));
        }

        let total = self.rise.total();
        if !RISE_SECONDS.contains(&total) {
            issue(format!("rise lasts {total} s, outside {RISE_SECONDS:?}"));
        }
        if self
            .rise
            .steps
            .iter()
            .any(|s| s.start < 0.0 || s.duration < 0.0)
        {
            issue("rise steps need start >= 0 and duration >= 0".into());
        }
        if !self.rise.steps.is_sorted_by_key(|s| s.stage) {
            issue("rise steps must follow the PRD §4.4 stage order".into());
        }

        issues
    }

    /// Rail rules (P3): a rail node joins two rail segments (a plain joint) or three at a declared
    /// switch, whose toe segment runs from a rail lane edge to the node and whose legs leave it;
    /// a switch names three distinct rail segments and a positive fouling distance.
    fn validate_rail(&self, issue: &mut impl FnMut(String)) {
        let rail = |i: u16| {
            self.networks
                .get(usize::from(i))
                .filter(|s| s.kind == NetworkKind::Rail)
        };
        let mut switch_nodes: BTreeSet<&str> = BTreeSet::new();
        for sw in &self.switches {
            if !switch_nodes.insert(sw.node.as_str()) {
                issue(format!("switch at {:?} is declared twice", sw.node));
            }
            let node = Endpoint::Node(sw.node.clone());
            if sw.toe == sw.straight || sw.toe == sw.branch || sw.straight == sw.branch {
                issue(format!(
                    "switch at {:?} needs three distinct segments",
                    sw.node
                ));
            }
            for (role, i) in [
                ("toe", sw.toe),
                ("straight", sw.straight),
                ("branch", sw.branch),
            ] {
                match rail(i) {
                    Some(s) if s.from == node || s.to == node => {}
                    Some(_) => issue(format!(
                        "switch at {:?}: {role} segment {i} does not end at its node",
                        sw.node
                    )),
                    None => issue(format!(
                        "switch at {:?}: {role} segment {i} is not a rail segment",
                        sw.node
                    )),
                }
            }
            if let Some(s) = rail(sw.toe) {
                let other = if s.from == node { &s.to } else { &s.from };
                if !matches!(other, Endpoint::Edge(_)) {
                    issue(format!(
                        "switch at {:?}: the toe segment must run from a lane edge",
                        sw.node
                    ));
                }
            }
            if !(sw.fouling.is_finite() && sw.fouling > 0.0) {
                issue(format!(
                    "switch at {:?} needs a positive fouling distance",
                    sw.node
                ));
            }
        }
        for (k, stop) in self.stops.iter().enumerate() {
            if rail(stop.segment).is_none() {
                issue(format!(
                    "stop {k}: segment {} is not a rail segment",
                    stop.segment
                ));
            }
            let [a, b] = stop.level;
            if !(a.is_finite() && b.is_finite() && 0.0 <= a && a < b) {
                issue(format!(
                    "stop {k} needs a level section 0 <= from < to, not {:?}",
                    stop.level
                ));
            }
        }
        for node in &self.nodes {
            let end = Endpoint::Node(node.id.clone());
            let count = self
                .networks
                .iter()
                .filter(|s| s.kind == NetworkKind::Rail && (s.from == end || s.to == end))
                .count();
            let switch = switch_nodes.contains(node.id.as_str());
            match (count, switch) {
                (0, false) | (2, false) | (3, true) => {}
                (3, false) => issue(format!(
                    "rail node {:?} joins 3 segments but is not a declared switch",
                    node.id
                )),
                (n, _) => issue(format!(
                    "rail node {:?} joins {n} segments; a joint needs 2, a switch 3",
                    node.id
                )),
            }
        }
    }

    /// Destination rules (PRD §7.1): cars arrive at and leave from parking spots, so every
    /// destination node has at least one spot joining it, and at least one road edge of the tile
    /// reaches it through the tile's own road segments (a destination with no road from an edge
    /// can never be connected to another).
    fn validate_destinations(&self, issue: &mut impl FnMut(String)) {
        for node in self.nodes.iter().filter(|n| n.destination.is_some()) {
            let kind = node.destination.expect("filtered");
            if !self.parking.iter().any(|s| s.node == node.id) {
                issue(format!(
                    "{kind:?} destination {:?} has no parking spot: a car has nowhere to stop",
                    node.id
                ));
            }
            if !self.road_edge_reaches(&node.id) {
                issue(format!(
                    "{kind:?} destination {:?} is not reached by a road from any edge",
                    node.id
                ));
            }
        }
    }

    /// Whether some road lane edge connects to node `id` through the tile's road segments.
    fn road_edge_reaches(&self, id: &str) -> bool {
        let roads: Vec<&NetworkSegment> = self
            .networks
            .iter()
            .filter(|s| s.kind == NetworkKind::Road)
            .collect();
        let mut seen: std::collections::HashSet<&Endpoint> = std::collections::HashSet::new();
        let start = Endpoint::Node(id.to_owned());
        let mut stack: Vec<&Endpoint> = vec![];
        for seg in &roads {
            for (a, b) in [(&seg.from, &seg.to), (&seg.to, &seg.from)] {
                if *a == start && seen.insert(b) {
                    stack.push(b);
                }
            }
        }
        while let Some(end) = stack.pop() {
            if matches!(end, Endpoint::Edge(_)) {
                return true;
            }
            for seg in &roads {
                for (a, b) in [(&seg.from, &seg.to), (&seg.to, &seg.from)] {
                    if a == end && seen.insert(b) {
                        stack.push(b);
                    }
                }
            }
        }
        false
    }

    /// The destination kind the tile parks at: the kind of the node its first spot joins.
    pub fn parking_destination(&self) -> Option<Destination> {
        let spot = self.parking.first()?;
        self.nodes.iter().find(|n| n.id == spot.node)?.destination
    }

    /// Parking rules (PRD §7.1): ids are `park-<index>` and unique, each names a hero prop (a house
    /// at a house destination), every spot's paths join a destination node of one kind at the end
    /// of a road segment (so the spot reaches the road graph), every pose is finite, and the car
    /// cap fits: 1 up to the spots at houses, 0 at a shop, which owns no cars.
    fn validate_parking(&self, issue: &mut impl FnMut(String)) {
        if self.parking.is_empty() {
            return;
        }
        let kind = self.parking_destination();
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for (i, spot) in self.parking.iter().enumerate() {
            let id = spot.id.as_str();
            if !ids.insert(id) {
                issue(format!("parking spot {id:?} is declared twice"));
            }
            if id != format!("park-{i}") {
                issue(format!(
                    "parking spot {id:?} must be named park-{i} after its index"
                ));
            }
            match self.props.hero.get(usize::from(spot.house)) {
                Some(h)
                    if kind == Some(Destination::House)
                        && !h.piece.as_str().starts_with("house_") =>
                {
                    issue(format!(
                        "parking spot {id:?} house {} is {:?}, not a house",
                        spot.house, h.piece.0
                    ))
                }
                Some(_) => {}
                None => issue(format!(
                    "parking spot {id:?} house {} is not a hero prop",
                    spot.house
                )),
            }
            match self
                .nodes
                .iter()
                .find(|n| n.id == spot.node)
                .and_then(|n| n.destination)
            {
                Some(d) if Some(d) != kind => issue(format!(
                    "parking spot {id:?} joins a {d:?} node, but the tile parks at {kind:?}"
                )),
                Some(_) => {}
                None => issue(format!(
                    "parking spot {id:?} joins {:?}, which is not a declared destination node",
                    spot.node
                )),
            }
            let end = Endpoint::Node(spot.node.clone());
            let on_road = self
                .networks
                .iter()
                .any(|seg| seg.kind == NetworkKind::Road && (seg.from == end || seg.to == end));
            if !on_road {
                issue(format!(
                    "parking spot {id:?} cannot reach the road: no road segment ends at {:?}",
                    spot.node
                ));
            }
            let finite = std::iter::once(spot.pose())
                .chain(spot.enter.iter().copied())
                .chain(spot.back.iter().copied())
                .chain(spot.leave.iter().copied())
                .all(|p| p.at[0].is_finite() && p.at[1].is_finite() && p.yaw_deg.is_finite());
            if !finite {
                issue(format!("parking spot {id:?} has a non-finite pose"));
            }
        }
        let cap = self.car_cap();
        match kind {
            Some(Destination::Shop) if cap != 0 => {
                issue(format!("car cap {cap} must be 0: a shop owns no cars"))
            }
            Some(Destination::Shop) => {}
            _ if cap == 0 || cap > self.parking.len() => issue(format!(
                "car cap {cap} must be between 1 and the {} parking spots",
                self.parking.len()
            )),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::{LaneSpec, Side, SocketKind::*};

    fn sockets() -> SocketTable {
        SocketTable::builtin().unwrap()
    }

    fn road_lane() -> Option<LaneSpec> {
        Some(LaneSpec {
            offset: 0.0,
            width: 6.0,
        })
    }

    /// Road on edges 0 and 1 (`road_bend_60`).
    fn bend60() -> TileDef {
        let mut edges: [EdgeSocket; 6] = std::array::from_fn(|_| EdgeSocket::new(Grass));
        for e in [0, 1] {
            edges[e] = EdgeSocket {
                lane: road_lane(),
                ..EdgeSocket::new(Road)
            };
        }
        TileDef {
            id: "bend".into(),
            biome: Biome::Grass,
            base: "meadow_base".into(),
            edges,
            center: Some("road_bend_60_center".into()),
            edge_pieces: EdgePieces::Derived,
            props: PropScatter::default(),
            nodes: vec![],
            networks: vec![NetworkSegment {
                kind: NetworkKind::Road,
                from: Endpoint::Edge(0),
                to: Endpoint::Edge(1),
                points: vec![[6.93, 4.0]],
            }],
            switches: vec![],
            stops: vec![],
            parking: vec![],
            emitters: vec![],
            agents: vec![],
            rise: RiseTimeline::default(),
            unlock: Unlock::Default,
            look_period: None,
        }
    }

    /// A one-house cluster: road on edge 0 into node "home", one spot and a car cap of 1.
    fn cluster() -> TileDef {
        let mut t = bend60();
        t.edges[1] = EdgeSocket::new(Grass);
        t.props.hero = vec![HeroProp {
            piece: "house_cottage".into(),
            at: [-2.0, 5.0],
            yaw_deg: 90.0,
            y: 0.0,
        }];
        t.nodes = vec![NetworkNode {
            id: "home".into(),
            at: [8.0, 0.0],
            destination: Some(Destination::House),
        }];
        t.networks = vec![NetworkSegment {
            kind: NetworkKind::Road,
            from: Endpoint::Edge(0),
            to: Endpoint::Node("home".into()),
            points: vec![],
        }];
        t.parking = vec![ParkingSpot {
            id: "park-0".into(),
            house: 0,
            node: "home".into(),
            at: [0.0, 2.0],
            yaw_deg: 180.0,
            enter: vec![],
            back: vec![ParkPose {
                at: [4.0, 0.0],
                yaw_deg: 90.0,
            }],
            back_alt: vec![],
            leave: vec![],
        }];
        t.agents = vec![AgentSpawnRule {
            agent: AgentKind::Car,
            max: 1,
        }];
        t
    }

    #[test]
    fn valid_parking_has_no_issues() {
        assert_eq!(cluster().validate(&sockets()), Vec::<String>::new());
    }

    #[test]
    fn parking_spot_must_reach_a_road_destination_node() {
        let mut t = cluster();
        t.parking[0].node = "nowhere".into();
        let issues = t.validate(&sockets());
        assert!(
            issues
                .iter()
                .any(|i| i.contains("not a declared destination node")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("cannot reach the road")),
            "{issues:?}"
        );

        let mut t = cluster();
        t.networks.clear();
        t.edges[0] = EdgeSocket::new(Grass);
        let issues = t.validate(&sockets());
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(
            issues.iter().any(|i| i.contains("cannot reach the road")),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|i| i.contains("not reached by a road from any edge")),
            "{issues:?}"
        );
    }

    /// A one-shop tile: [`cluster`] with the bakery for the house, a Shop node and no cars.
    fn shop() -> TileDef {
        let mut t = cluster();
        t.props.hero[0].piece = "bakery_shop".into();
        t.nodes[0].destination = Some(Destination::Shop);
        t.agents.clear();
        t
    }

    #[test]
    fn parking_spot_ids_follow_their_index() {
        let mut t = cluster();
        t.parking[0].id = "spot".into();
        assert!(t.validate(&sockets())[0].contains("park-0"));
        let mut t = cluster();
        t.parking[0].id = "park-3".into();
        assert!(t.validate(&sockets())[0].contains("park-0 after its index"));
        let mut t = cluster();
        t.parking[0].house = 3;
        assert!(t.validate(&sockets())[0].contains("not a hero prop"));
        let mut t = cluster();
        t.props.hero[0].piece = "rock_small".into();
        assert!(t.validate(&sockets())[0].contains("not a house"));
        let mut t = cluster();
        t.parking.push(t.parking[0].clone());
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("declared twice"))
        );
    }

    #[test]
    fn car_cap_must_fit_the_spots() {
        let mut t = cluster();
        t.agents[0].max = 2;
        assert!(t.validate(&sockets())[0].contains("car cap 2"));
        t.agents.clear();
        assert!(t.validate(&sockets())[0].contains("car cap 0"));
    }

    #[test]
    fn a_shop_parks_visitors_and_owns_no_cars() {
        let t = shop();
        assert_eq!(t.parking_destination(), Some(Destination::Shop));
        assert_eq!(t.validate(&sockets()), Vec::<String>::new());
        let mut t = shop();
        t.agents = cluster().agents;
        assert!(t.validate(&sockets())[0].contains("a shop owns no cars"));
        let mut t = shop();
        t.parking[0].house = 1;
        assert!(t.validate(&sockets())[0].contains("not a hero prop"));
    }

    /// PRD §7.1: a destination tile needs a road from an edge to its node and a spot to stop in.
    #[test]
    fn a_destination_needs_parking_and_a_road_from_an_edge() {
        for t in [cluster(), shop()] {
            let kind = t.nodes[0].destination.unwrap();
            let mut no_spot = t.clone();
            no_spot.parking.clear();
            no_spot.agents.clear();
            let issues = no_spot.validate(&sockets());
            assert!(
                issues.iter().any(|i| i.contains("has no parking spot")),
                "{kind:?}: {issues:?}"
            );

            // The node hangs off a second node with no road to any edge.
            let mut island = t.clone();
            island.nodes.push(NetworkNode {
                id: "yard".into(),
                at: [0.0, 6.0],
                destination: None,
            });
            island.networks[0].from = Endpoint::Node("yard".into());
            island.edges[0] = EdgeSocket::new(Grass);
            let issues = island.validate(&sockets());
            assert!(
                issues
                    .iter()
                    .any(|i| i.contains("not reached by a road from any edge")),
                "{kind:?}: {issues:?}"
            );
        }
    }

    /// A destination reached through a junction node (a road through the tile with a spur into a
    /// forecourt, as the bakery tile lays out) is reached from an edge.
    #[test]
    fn a_destination_on_a_spur_is_reached_through_its_junction() {
        let mut t = shop();
        t.edges[3] = EdgeSocket {
            lane: road_lane(),
            ..EdgeSocket::new(Road)
        };
        t.nodes.push(NetworkNode {
            id: "j".into(),
            at: [0.0, 0.0],
            destination: None,
        });
        t.nodes[0].at = [0.0, -8.0];
        t.networks = vec![
            NetworkSegment {
                kind: NetworkKind::Road,
                from: Endpoint::Edge(0),
                to: Endpoint::Node("j".into()),
                points: vec![],
            },
            NetworkSegment {
                kind: NetworkKind::Road,
                from: Endpoint::Node("j".into()),
                to: Endpoint::Edge(3),
                points: vec![],
            },
            NetworkSegment {
                kind: NetworkKind::Road,
                from: Endpoint::Node("j".into()),
                to: Endpoint::Node("home".into()),
                points: vec![],
            },
        ];
        assert_eq!(t.validate(&sockets()), Vec::<String>::new());
    }

    #[test]
    fn a_tile_parks_at_one_destination_kind() {
        let mut t = cluster();
        t.nodes.push(NetworkNode {
            id: "shop".into(),
            at: [8.0, 4.0],
            destination: Some(Destination::Shop),
        });
        t.networks.push(NetworkSegment {
            kind: NetworkKind::Road,
            from: Endpoint::Node("home".into()),
            to: Endpoint::Node("shop".into()),
            points: vec![],
        });
        let mut spot = t.parking[0].clone();
        spot.id = "park-1".into();
        spot.node = "shop".into();
        t.parking.push(spot);
        let issues = t.validate(&sockets());
        assert!(
            issues.iter().any(|i| i.contains("joins a Shop node")),
            "{issues:?}"
        );
    }

    #[test]
    fn non_finite_parking_pose_is_an_issue() {
        let mut t = cluster();
        t.parking[0].back[0].yaw_deg = f32::NAN;
        assert!(t.validate(&sockets())[0].contains("non-finite"));
    }

    #[test]
    fn rotation_k_maps_local_edge_e_to_e_plus_k_mod_6() {
        for k in 0..6 {
            for e in 0..6 {
                assert_eq!(world_edge(e, k), (e + k) % 6);
                assert_eq!(local_edge(world_edge(e, k), k), e);
            }
        }
    }

    #[test]
    fn rotated_tile_reports_sockets_on_world_edges() {
        let t = bend60();
        let roads = |k: u8| -> Vec<u8> { (0..6).filter(|&w| t.edge(w, k).kind == Road).collect() };
        assert_eq!(roads(0), vec![0, 1]);
        assert_eq!(roads(2), vec![2, 3]);
        assert_eq!(roads(5), vec![0, 5]);
    }

    #[test]
    fn opposite_edges_face_each_other() {
        assert_eq!(opposite_edge(0), 3);
        assert_eq!(opposite_edge(4), 1);
    }

    #[test]
    fn derived_edge_pieces_follow_socket_kinds() {
        let t = bend60();
        let s = sockets();
        let pieces = t.edge_pieces(&s);
        assert_eq!(pieces[0].map(KitRef::as_str), Some("road_straight_edge"));
        assert_eq!(pieces[1].map(KitRef::as_str), Some("road_straight_edge"));
        assert!(pieces[2..].iter().all(Option::is_none));
    }

    #[test]
    fn explicit_edge_pieces_override_derivation() {
        let mut t = bend60();
        t.edge_pieces = EdgePieces::Explicit(Default::default());
        assert!(t.edge_pieces(&sockets()).iter().all(Option::is_none));
    }

    #[test]
    fn kit_refs_collects_base_center_and_edges() {
        let t = bend60();
        let s = sockets();
        let refs: Vec<&str> = t.kit_refs(&s).into_iter().collect();
        assert_eq!(
            refs,
            ["meadow_base", "road_bend_60_center", "road_straight_edge"]
        );
    }

    #[test]
    fn valid_tile_has_no_issues() {
        assert_eq!(bend60().validate(&sockets()), Vec::<String>::new());
    }

    #[test]
    fn lane_edge_without_endpoint_is_an_issue() {
        let mut t = bend60();
        t.networks.clear();
        assert_eq!(t.validate(&sockets()).len(), 2);
    }

    #[test]
    fn endpoint_on_grass_edge_is_an_issue() {
        let mut t = bend60();
        t.networks[0].to = Endpoint::Edge(3);
        let issues = t.validate(&sockets());
        assert!(
            issues
                .iter()
                .any(|i| i.contains("edge 3 Grass, which has no lane")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("lane edge 1 has 0")),
            "{issues:?}"
        );
    }

    #[test]
    fn river_segment_on_road_edge_is_an_issue() {
        let mut t = bend60();
        t.networks[0].kind = NetworkKind::River;
        assert_eq!(t.validate(&sockets()).len(), 2);
    }

    #[test]
    fn undeclared_node_is_an_issue() {
        let mut t = bend60();
        t.networks[0].to = Endpoint::Node("hub".into());
        let issues = t.validate(&sockets());
        assert!(
            issues.iter().any(|i| i.contains("undeclared node")),
            "{issues:?}"
        );
    }

    #[test]
    fn lane_must_match_the_socket_table() {
        let mut t = bend60();
        t.edges[0].lane = Some(LaneSpec {
            offset: 0.0,
            width: 5.0,
        });
        t.edges[2].lane = road_lane();
        assert_eq!(t.validate(&sockets()).len(), 2);
    }

    #[test]
    fn land_side_belongs_only_on_directional_edges() {
        let mut t = bend60();
        t.edges[2] = EdgeSocket::new(Beach);
        t.edges[3].land = Some(Side::Left);
        let issues = t.validate(&sockets());
        assert!(
            issues
                .iter()
                .any(|i| i.contains("edge 2 Beach needs a land side")),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|i| i.contains("edge 3 Grass cannot carry a land side")),
            "{issues:?}"
        );
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn default_rise_is_within_prd_range_and_ordered() {
        let rise = RiseTimeline::default();
        assert_eq!(rise.total(), 2.0);
        assert!(bend60().validate(&sockets()).is_empty());
    }

    #[test]
    fn rise_outside_range_is_an_issue() {
        let mut t = bend60();
        t.rise.steps.last_mut().unwrap().duration = 3.0;
        assert_eq!(t.validate(&sockets()).len(), 1);
    }

    #[test]
    fn tile_round_trips_through_ron() {
        let t = bend60();
        let text = ron::ser::to_string_pretty(&t, Default::default()).unwrap();
        let back: TileDef = ron::from_str(&text).unwrap();
        assert_eq!(back, t);
    }

    fn rail_lane() -> Option<LaneSpec> {
        Some(LaneSpec {
            offset: 0.0,
            width: 3.5,
        })
    }

    /// A turnout on edges 0 (toe), 3 (straight) and 1 (branch), as `rail_junction` lays it out.
    fn turnout() -> TileDef {
        let mut t = bend60();
        t.edges = std::array::from_fn(|_| EdgeSocket::new(Grass));
        for e in [0, 1, 3] {
            t.edges[e] = EdgeSocket {
                lane: rail_lane(),
                ..EdgeSocket::new(Rail)
            };
        }
        t.nodes = vec![NetworkNode {
            id: "toe".into(),
            at: [13.6, 0.0],
            destination: None,
        }];
        let seg = |from, to| NetworkSegment {
            kind: NetworkKind::Rail,
            from,
            to,
            points: vec![],
        };
        t.networks = vec![
            seg(Endpoint::Edge(0), Endpoint::Node("toe".into())),
            seg(Endpoint::Node("toe".into()), Endpoint::Edge(3)),
            seg(Endpoint::Node("toe".into()), Endpoint::Edge(1)),
        ];
        t.switches = vec![SwitchDef {
            node: "toe".into(),
            toe: 0,
            straight: 1,
            branch: 2,
            fouling: 7.42,
        }];
        t
    }

    #[test]
    fn a_declared_turnout_is_valid() {
        assert_eq!(turnout().validate(&sockets()), Vec::<String>::new());
    }

    /// Three rail segments at a node without a switch would let a train turn through the vee.
    #[test]
    fn three_rail_segments_need_a_declared_switch() {
        let mut t = turnout();
        t.switches.clear();
        let issues = t.validate(&sockets());
        assert!(
            issues.iter().any(|i| i.contains("not a declared switch")),
            "{issues:?}"
        );
    }

    #[test]
    fn a_switch_names_three_distinct_rail_segments_at_its_node() {
        let mut t = turnout();
        t.switches[0].branch = 1;
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("three distinct"))
        );
        let mut t = turnout();
        t.switches[0].fouling = 0.0;
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("positive fouling"))
        );
        let mut t = turnout();
        t.switches[0].branch = 9;
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("not a rail segment"))
        );
    }

    /// A stop names a rail segment and a level section that runs forward from its start.
    #[test]
    fn a_stop_names_a_rail_segment_and_a_forward_level_section() {
        let mut t = turnout();
        t.stops = vec![StopDef {
            segment: 1,
            level: [2.0, 12.0],
        }];
        assert_eq!(t.validate(&sockets()), Vec::<String>::new());
        t.stops[0].segment = 9;
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("stop 0: segment 9 is not a rail segment"))
        );
        t.stops[0] = StopDef {
            segment: 1,
            level: [12.0, 2.0],
        };
        assert!(
            t.validate(&sockets())
                .iter()
                .any(|i| i.contains("needs a level section"))
        );
    }

    #[test]
    fn a_rail_segment_needs_a_rail_lane_edge() {
        let mut t = turnout();
        t.edges[1] = EdgeSocket {
            lane: road_lane(),
            ..EdgeSocket::new(Road)
        };
        let issues = t.validate(&sockets());
        assert!(
            issues
                .iter()
                .any(|i| i.contains("is Rail but edge 1 is Road")),
            "{issues:?}"
        );
    }
}
