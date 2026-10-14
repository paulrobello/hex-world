//! Edge sockets and the socket compatibility table (PRD §5.2, plan F2/F3, owner O1).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tile::KitRef;

/// Terrain or feature type at a tile edge (PRD §5.2, MVP set).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SocketKind {
    Grass,
    Forest,
    Road,
    River,
    Beach,
    Ocean,
}

impl SocketKind {
    pub const ALL: [SocketKind; 6] = [
        SocketKind::Grass,
        SocketKind::Forest,
        SocketKind::Road,
        SocketKind::River,
        SocketKind::Beach,
        SocketKind::Ocean,
    ];
}

/// A network crossing at an edge (road or river), in meters.
///
/// `offset` is measured from the edge midpoint along the edge tangent `(-n_z, n_x)`, where `n` is
/// the tile's outward edge normal (`hexgeo.lane_frame`). The two tiles at a seam have opposite
/// normals, so their tangents point in opposite directions: a lane at offset `o` on one side meets
/// offset `-o` on the other. Rotation is a proper rotation, so it never flips the sign.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaneSpec {
    pub offset: f32,
    pub width: f32,
}

impl LaneSpec {
    /// Whether this lane lines up with `other` on the far side of a seam: equal width and mirrored
    /// offset. Values are data-authored constants, so exact equality is intended (`0.0 == -0.0`).
    pub fn meets(&self, other: &LaneSpec) -> bool {
        self.width == other.width && self.offset == -other.offset
    }
}

/// Which side of a directional edge the land is on, looking out across the edge from its own tile
/// (PRD §5.2, owner O1). `Right` puts the land at the corner the edge shares with the next edge
/// clockwise (`e + 1`), `Left` at the corner it shares with `e - 1`. The two tiles at a seam look in
/// opposite directions, so a coast continues the same way only where `Left` meets `Right`. A proper
/// rotation never changes the side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// Edge cross-section shape. Only `Flat` is used in the MVP (D5); `Cliff` and `Ramp` arrive in P4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeProfile {
    #[default]
    Flat,
    Cliff,
    Ramp,
}

/// The socket descriptor carried by one tile edge (PRD §5.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeSocket {
    pub kind: SocketKind,
    #[serde(default)]
    pub lane: Option<LaneSpec>,
    /// The land side of a directional edge (`data/sockets.ron` `directional`), `None` elsewhere.
    #[serde(default)]
    pub land: Option<Side>,
    /// Edge height level; 0 for every MVP edge.
    #[serde(default)]
    pub height: u8,
    #[serde(default)]
    pub profile: EdgeProfile,
}

impl EdgeSocket {
    pub fn new(kind: SocketKind) -> Self {
        Self {
            kind,
            lane: None,
            land: None,
            height: 0,
            profile: EdgeProfile::Flat,
        }
    }
}

/// `data/sockets.ron` as authored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SocketTableFile {
    /// For each kind, the kinds it may meet. Must list every kind and be symmetric.
    pub compatible: BTreeMap<SocketKind, Vec<SocketKind>>,
    /// The lane every edge of a lane kind carries (F3). Kinds absent here never carry a lane.
    pub lanes: BTreeMap<SocketKind, LaneSpec>,
    /// Edge piece derived for an edge of this kind when a tile does not list its own (PRD §6).
    #[serde(default)]
    pub edge_pieces: BTreeMap<SocketKind, KitRef>,
    /// Kinds whose edges carry a land [`Side`] and meet only the opposite side (owner O1).
    #[serde(default)]
    pub directional: Vec<SocketKind>,
}

/// A validated, symmetric socket compatibility table.
#[derive(Clone, Debug, PartialEq)]
pub struct SocketTable {
    file: SocketTableFile,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SocketTableError {
    Parse(String),
    MissingKind(SocketKind),
    Asymmetric(SocketKind, SocketKind),
    BadLane(SocketKind),
}

impl fmt::Display for SocketTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketTableError::Parse(e) => write!(f, "sockets.ron: {e}"),
            SocketTableError::MissingKind(k) => write!(f, "sockets.ron: {k:?} has no entry"),
            SocketTableError::Asymmetric(a, b) => {
                write!(
                    f,
                    "sockets.ron: {a:?} meets {b:?} but {b:?} does not meet {a:?}"
                )
            }
            SocketTableError::BadLane(k) => write!(f, "sockets.ron: {k:?} lane width must be > 0"),
        }
    }
}

impl std::error::Error for SocketTableError {}

/// The socket table shipped in `data/sockets.ron`.
pub const SOCKETS_RON: &str = include_str!("../../../data/sockets.ron");

impl SocketTable {
    pub fn from_ron(text: &str) -> Result<Self, SocketTableError> {
        let file: SocketTableFile =
            ron::from_str(text).map_err(|e| SocketTableError::Parse(e.to_string()))?;
        Self::new(file)
    }

    pub fn new(file: SocketTableFile) -> Result<Self, SocketTableError> {
        for kind in SocketKind::ALL {
            if !file.compatible.contains_key(&kind) {
                return Err(SocketTableError::MissingKind(kind));
            }
        }
        for (&a, partners) in &file.compatible {
            for &b in partners {
                if !file
                    .compatible
                    .get(&b)
                    .is_some_and(|back| back.contains(&a))
                {
                    return Err(SocketTableError::Asymmetric(a, b));
                }
            }
        }
        for (&kind, lane) in &file.lanes {
            if lane.width <= 0.0 || !lane.width.is_finite() || !lane.offset.is_finite() {
                return Err(SocketTableError::BadLane(kind));
            }
        }
        Ok(Self { file })
    }

    pub fn builtin() -> Result<Self, SocketTableError> {
        Self::from_ron(SOCKETS_RON)
    }

    pub fn kinds_compatible(&self, a: SocketKind, b: SocketKind) -> bool {
        self.file
            .compatible
            .get(&a)
            .is_some_and(|partners| partners.contains(&b))
    }

    /// Whether two edges may meet across a seam: compatible kinds, matching lanes (both absent, or
    /// both present with equal width and mirrored offset, see [`LaneSpec`]), opposite land sides on
    /// directional edges (both absent, or `Left` against `Right`, see [`Side`]) and a legal height
    /// step.
    pub fn edges_compatible(&self, a: &EdgeSocket, b: &EdgeSocket) -> bool {
        let lanes = match (&a.lane, &b.lane) {
            (None, None) => true,
            (Some(x), Some(y)) => x.meets(y),
            _ => false,
        };
        let sides = match (a.land, b.land) {
            (None, None) => true,
            (Some(x), Some(y)) => x == y.opposite(),
            _ => false,
        };
        self.kinds_compatible(a.kind, b.kind) && lanes && sides && heights_compatible(a, b)
    }

    /// Whether edges of `kind` are directional: each carries a land [`Side`].
    pub fn directional(&self, kind: SocketKind) -> bool {
        self.file.directional.contains(&kind)
    }

    /// The canonical lane for a lane kind, or `None` for kinds that never carry one.
    pub fn lane(&self, kind: SocketKind) -> Option<LaneSpec> {
        self.file.lanes.get(&kind).copied()
    }

    /// The edge piece derived for an edge of `kind`.
    pub fn edge_piece(&self, kind: SocketKind) -> Option<&KitRef> {
        self.file.edge_pieces.get(&kind)
    }

    pub fn file(&self) -> &SocketTableFile {
        &self.file
    }
}

/// PRD §5.2 height rule: equal heights, except `Cliff` (any difference) and `Ramp` (exactly one
/// level). Every MVP edge is `Flat` at height 0; the non-flat arms are the P4 rule as written.
fn heights_compatible(a: &EdgeSocket, b: &EdgeSocket) -> bool {
    let diff = a.height.abs_diff(b.height);
    if a.profile == EdgeProfile::Cliff || b.profile == EdgeProfile::Cliff {
        true
    } else if a.profile == EdgeProfile::Ramp || b.profile == EdgeProfile::Ramp {
        diff == 1
    } else {
        diff == 0
    }
}

#[cfg(test)]
mod tests {
    use super::SocketKind::*;
    use super::*;

    /// Plan F2 with owner O1 (beach never meets grass or ocean), as unordered pairs.
    const F2_LEGAL: [(SocketKind, SocketKind); 7] = [
        (Grass, Grass),
        (Grass, Forest),
        (Forest, Forest),
        (Road, Road),
        (River, River),
        (Beach, Beach),
        (Ocean, Ocean),
    ];

    fn table() -> SocketTable {
        SocketTable::builtin().expect("data/sockets.ron loads")
    }

    fn beach(land: Side) -> EdgeSocket {
        EdgeSocket {
            land: Some(land),
            ..EdgeSocket::new(Beach)
        }
    }

    fn lane(kind: SocketKind, offset: f32, width: f32) -> EdgeSocket {
        EdgeSocket {
            lane: Some(LaneSpec { offset, width }),
            ..EdgeSocket::new(kind)
        }
    }

    #[test]
    fn builtin_table_matches_f2_for_all_36_ordered_pairs() {
        let t = table();
        for a in SocketKind::ALL {
            for b in SocketKind::ALL {
                let want = F2_LEGAL.contains(&(a, b)) || F2_LEGAL.contains(&(b, a));
                assert_eq!(t.kinds_compatible(a, b), want, "{a:?}-{b:?}");
            }
        }
    }

    #[test]
    fn builtin_table_is_symmetric() {
        let t = table();
        for a in SocketKind::ALL {
            for b in SocketKind::ALL {
                assert_eq!(t.kinds_compatible(a, b), t.kinds_compatible(b, a));
            }
        }
    }

    #[test]
    fn builtin_lanes_follow_f3() {
        let t = table();
        assert_eq!(
            t.lane(Road),
            Some(LaneSpec {
                offset: 0.0,
                width: 6.0
            })
        );
        assert_eq!(
            t.lane(River),
            Some(LaneSpec {
                offset: 0.0,
                width: 8.0
            })
        );
        for k in [Grass, Forest, Beach, Ocean] {
            assert_eq!(t.lane(k), None);
        }
    }

    #[test]
    fn asymmetric_table_is_rejected() {
        let mut file = table().file().clone();
        file.compatible.get_mut(&Road).unwrap().push(Grass);
        assert_eq!(
            SocketTable::new(file),
            Err(SocketTableError::Asymmetric(Road, Grass))
        );
    }

    #[test]
    fn table_missing_a_kind_is_rejected() {
        let mut file = table().file().clone();
        file.compatible.remove(&Ocean);
        assert_eq!(
            SocketTable::new(file),
            Err(SocketTableError::MissingKind(Ocean))
        );
    }

    #[test]
    fn only_beach_is_directional() {
        let t = table();
        for k in SocketKind::ALL {
            assert_eq!(t.directional(k), k == Beach, "{k:?}");
        }
    }

    /// A coast continues the same way: the land sides are named looking out from each tile, so
    /// the two tiles at a seam see the same land corner on opposite hands.
    #[test]
    fn beach_meets_only_the_opposite_land_side() {
        let t = table();
        assert!(t.edges_compatible(&beach(Side::Left), &beach(Side::Right)));
        assert!(t.edges_compatible(&beach(Side::Right), &beach(Side::Left)));
        assert!(!t.edges_compatible(&beach(Side::Left), &beach(Side::Left)));
        assert!(!t.edges_compatible(&beach(Side::Right), &beach(Side::Right)));
        assert!(!t.edges_compatible(&beach(Side::Left), &EdgeSocket::new(Beach)));
    }

    #[test]
    fn beach_never_meets_grass_or_ocean_on_either_side() {
        let t = table();
        for side in [Side::Left, Side::Right] {
            for k in [Grass, Forest, Road, River, Ocean] {
                let other = EdgeSocket {
                    land: Some(side.opposite()),
                    ..EdgeSocket::new(k)
                };
                assert!(
                    !t.edges_compatible(&beach(side), &EdgeSocket::new(k)),
                    "{k:?}"
                );
                assert!(
                    !t.edges_compatible(&beach(side), &other),
                    "{k:?} with a side"
                );
            }
        }
    }

    #[test]
    fn zero_width_lane_is_rejected() {
        let mut file = table().file().clone();
        file.lanes.insert(
            Road,
            LaneSpec {
                offset: 0.0,
                width: 0.0,
            },
        );
        assert_eq!(SocketTable::new(file), Err(SocketTableError::BadLane(Road)));
    }

    #[test]
    fn table_round_trips_through_ron() {
        let file = table().file().clone();
        let text = ron::ser::to_string_pretty(&file, Default::default()).unwrap();
        assert_eq!(SocketTable::from_ron(&text).unwrap().file(), &file);
    }

    #[test]
    fn matching_lanes_meet() {
        let t = table();
        assert!(t.edges_compatible(&lane(Road, 0.0, 6.0), &lane(Road, 0.0, 6.0)));
    }

    #[test]
    fn lane_offset_is_mirrored_across_the_seam() {
        let t = table();
        assert!(t.edges_compatible(&lane(Road, 1.5, 6.0), &lane(Road, -1.5, 6.0)));
        assert!(!t.edges_compatible(&lane(Road, 1.5, 6.0), &lane(Road, 1.5, 6.0)));
    }

    #[test]
    fn lane_width_must_match() {
        let t = table();
        assert!(!t.edges_compatible(&lane(River, 0.0, 8.0), &lane(River, 0.0, 6.0)));
    }

    #[test]
    fn lane_against_no_lane_is_incompatible() {
        let t = table();
        assert!(!t.edges_compatible(&lane(Road, 0.0, 6.0), &EdgeSocket::new(Road)));
    }

    #[test]
    fn compatible_kinds_without_lanes_meet() {
        let t = table();
        assert!(t.edges_compatible(&EdgeSocket::new(Grass), &EdgeSocket::new(Forest)));
        assert!(!t.edges_compatible(&EdgeSocket::new(Grass), &EdgeSocket::new(Ocean)));
    }

    #[test]
    fn flat_edges_need_equal_height() {
        let t = table();
        let high = EdgeSocket {
            height: 1,
            ..EdgeSocket::new(Grass)
        };
        assert!(!t.edges_compatible(&EdgeSocket::new(Grass), &high));
        assert!(t.edges_compatible(&high, &high.clone()));
    }

    #[test]
    fn beach_socket_reads_its_land_side_from_ron() {
        let e: EdgeSocket = ron::from_str("(kind: Beach, land: Some(Right))").unwrap();
        assert_eq!(e, beach(Side::Right));
        assert_eq!(Side::Right.opposite(), Side::Left);
    }

    #[test]
    fn edge_socket_defaults_to_flat_height_zero_without_lane() {
        let e: EdgeSocket = ron::from_str("(kind: Grass)").unwrap();
        assert_eq!(e, EdgeSocket::new(Grass));
    }
}
