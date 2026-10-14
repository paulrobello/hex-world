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
}

/// A prop at a fixed tile-local position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeroProp {
    pub piece: KitRef,
    pub at: [f32; 2],
    #[serde(default)]
    pub yaw_deg: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PropScatter {
    #[serde(default)]
    pub scatter: Vec<ScatterRule>,
    #[serde(default)]
    pub hero: Vec<HeroProp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NetworkKind {
    Road,
    River,
}

impl NetworkKind {
    /// The socket kind whose lanes this network crosses.
    pub fn socket(self) -> SocketKind {
        match self {
            NetworkKind::Road => SocketKind::Road,
            NetworkKind::River => SocketKind::River,
        }
    }
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
    House,
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

/// A positional ambience source; the cue id names an audio asset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioEmitter {
    pub cue: String,
    pub at: [f32; 2],
    pub radius: f32,
}

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
    #[serde(default)]
    pub emitters: Vec<AudioEmitter>,
    #[serde(default)]
    pub agents: Vec<AgentSpawnRule>,
    #[serde(default)]
    pub rise: RiseTimeline,
    #[serde(default)]
    pub unlock: Unlock,
}

impl TileDef {
    /// The socket facing world edge `world` when the tile is placed with `rotation`.
    pub fn edge(&self, world: u8, rotation: u8) -> &EdgeSocket {
        &self.edges[local_edge(world, rotation) as usize]
    }

    /// Edge pieces by local edge, resolving [`EdgePieces::Derived`] through the socket table.
    pub fn edge_pieces<'a>(&'a self, sockets: &'a SocketTable) -> [Option<&'a KitRef>; 6] {
        match &self.edge_pieces {
            EdgePieces::Derived => std::array::from_fn(|e| sockets.edge_piece(self.edges[e].kind)),
            EdgePieces::Explicit(pieces) => std::array::from_fn(|e| pieces[e].as_ref()),
        }
    }

    /// Every kit piece the tile references: base, center, edge pieces and props.
    pub fn kit_refs<'a>(&'a self, sockets: &'a SocketTable) -> BTreeSet<&'a KitRef> {
        let mut refs: BTreeSet<&KitRef> = BTreeSet::new();
        refs.insert(&self.base);
        refs.extend(self.center.iter());
        refs.extend(self.edge_pieces(sockets).into_iter().flatten());
        refs.extend(self.props.scatter.iter().map(|s| &s.piece));
        refs.extend(self.props.hero.iter().map(|h| &h.piece));
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
            emitters: vec![],
            agents: vec![],
            rise: RiseTimeline::default(),
            unlock: Unlock::Default,
        }
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
        let refs: Vec<&str> = t.kit_refs(&s).into_iter().map(KitRef::as_str).collect();
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
}
