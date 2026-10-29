//! Edge sockets and the socket compatibility table (PRD §5.2, plan F2/F3, owner O1).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tile::KitRef;

/// Terrain or feature type at a tile edge (PRD §5.2): the MVP set plus `Rail` (P3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SocketKind {
    Grass,
    Forest,
    Road,
    River,
    Beach,
    Ocean,
    Rail,
}

impl SocketKind {
    /// Every socket kind, in declaration order.
    pub const ALL: [SocketKind; 7] = [
        SocketKind::Grass,
        SocketKind::Forest,
        SocketKind::Road,
        SocketKind::River,
        SocketKind::Beach,
        SocketKind::Ocean,
        SocketKind::Rail,
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
    /// The land side seen from the neighboring tile across a seam.
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// Edge height profile, read clockwise viewed from +Y. `Flat` holds `height` along the whole edge,
/// `Rise` climbs from `height` at the edge's start corner to `height + 1` at its end corner and
/// `Fall` is the reverse (P4). A cliff is not a profile: it is two `Flat` edges that differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeProfile {
    #[default]
    Flat,
    Rise,
    Fall,
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
    /// Edge base height level, relative to the tile's own level; 0 for every MVP edge.
    #[serde(default)]
    pub height: u8,
    #[serde(default)]
    pub profile: EdgeProfile,
}

impl EdgeSocket {
    /// A flat, height-zero socket without a lane or directional land side.
    pub fn new(kind: SocketKind) -> Self {
        Self {
            kind,
            lane: None,
            land: None,
            height: 0,
            profile: EdgeProfile::Flat,
        }
    }

    /// The relative heights at the edge's clockwise (start, end) corners.
    pub fn corners(&self) -> (u8, u8) {
        match self.profile {
            EdgeProfile::Flat => (self.height, self.height),
            EdgeProfile::Rise => (self.height, self.height + 1),
            EdgeProfile::Fall => (self.height + 1, self.height),
        }
    }
}

/// How two edges that share a seam meet in height (PRD §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightMeet {
    /// The corner heights agree on both ends: no step.
    Seamless,
    /// Both edges are flat with no lane and sit at different heights: a cliff face.
    Cliff,
    Refused,
}

/// PRD §5.2 height rule. Edge `a` of a tile at `a_level` and edge `b` of the tile across the seam
/// at `b_level` meet `Seamless` when `a`'s absolute (start, end) corner heights equal `b`'s
/// absolute (end, start), as the two edges run in opposite directions. Two `Flat` edges without
/// lanes that differ meet as a `Cliff`. Anything else is `Refused`, so a lane edge (road, river,
/// rail) always meets at equal absolute height.
pub fn heights_meet(a: &EdgeSocket, a_level: u8, b: &EdgeSocket, b_level: u8) -> HeightMeet {
    let (a_start, a_end) = a.corners();
    let (b_start, b_end) = b.corners();
    let (a_start, a_end) = (a_start + a_level, a_end + a_level);
    let (b_start, b_end) = (b_start + b_level, b_end + b_level);
    if (a_start, a_end) == (b_end, b_start) {
        HeightMeet::Seamless
    } else if a.profile == EdgeProfile::Flat
        && b.profile == EdgeProfile::Flat
        && a.lane.is_none()
        && b.lane.is_none()
    {
        HeightMeet::Cliff
    } else {
        HeightMeet::Refused
    }
}

/// How a pair edge piece ends at a tile corner whose next edge is not the same pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PairCorners {
    /// The piece ends on its corner rays whatever the next edges are (option O-a).
    #[default]
    Straight,
    /// The piece tapers into the base's own outline beside an edge that is not the same pair
    /// (option O-b): one more variant per combination of the two corner flags, named `_t<p><n>`.
    Taper,
}

/// An edge piece chosen by the (own socket, neighbour socket) pair at an edge, for seams whose
/// look depends on both sides (forest floor across a Forest/Forest seam, decision D3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairEdgePiece {
    pub own: SocketKind,
    pub neighbour: SocketKind,
    /// The piece id, or the family stem when `per_base_edge` or `Taper` add suffixes.
    pub piece: String,
    /// One variant per local edge k of the tile's base (`<piece>_e<k>`): for a base that is not
    /// 60-degree symmetric, each variant matches its own edge's slice of the base.
    #[serde(default)]
    pub per_base_edge: bool,
    #[serde(default)]
    pub corners: PairCorners,
}

impl PairEdgePiece {
    /// The piece for local edge `local_edge`, given whether the previous (`local_edge - 1`) and
    /// next (`local_edge + 1`) edges of the tile carry the same pair. `Straight` ignores the flags.
    pub fn name(&self, local_edge: u8, prev_same: bool, next_same: bool) -> KitRef {
        let mut id = self.piece.clone();
        if self.per_base_edge {
            id.push_str(&format!("_e{}", local_edge % 6));
        }
        if self.corners == PairCorners::Taper && !(prev_same && next_same) {
            id.push_str(&format!("_t{}{}", prev_same as u8, next_same as u8));
        }
        KitRef(id)
    }

    /// Every piece id this entry can name.
    pub fn names(&self) -> BTreeSet<KitRef> {
        let edges: &[u8] = if self.per_base_edge {
            &[0, 1, 2, 3, 4, 5]
        } else {
            &[0]
        };
        let flags: &[bool] = if self.corners == PairCorners::Taper {
            &[false, true]
        } else {
            &[true]
        };
        let mut out = BTreeSet::new();
        for &e in edges {
            for &p in flags {
                for &n in flags {
                    out.insert(self.name(e, p, n));
                }
            }
        }
        out
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
    /// Edge pieces chosen by the socket pair across an edge (decision D3), placed in addition to
    /// the single-kind `edge_pieces`. Each pair must be compatible and listed once.
    #[serde(default)]
    pub pair_edge_pieces: Vec<PairEdgePiece>,
}

/// A validated, symmetric socket compatibility table.
#[derive(Clone, Debug, PartialEq)]
pub struct SocketTable {
    file: SocketTableFile,
}

/// A parse or invariant failure in the authored socket table.
#[derive(Clone, Debug, PartialEq)]
pub enum SocketTableError {
    Parse(String),
    MissingKind(SocketKind),
    Asymmetric(SocketKind, SocketKind),
    BadLane(SocketKind),
    BadPair(SocketKind, SocketKind),
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
            SocketTableError::BadPair(a, b) => write!(
                f,
                "sockets.ron: pair edge piece {a:?}/{b:?} is listed twice, never meets or has no piece"
            ),
        }
    }
}

impl std::error::Error for SocketTableError {}

/// The socket table shipped in `data/sockets.ron`.
pub const SOCKETS_RON: &str = include_str!("../../../data/sockets.ron");

impl SocketTable {
    /// Parses RON and validates compatibility, lanes and pair-piece entries.
    pub fn from_ron(text: &str) -> Result<Self, SocketTableError> {
        let file: SocketTableFile =
            ron::from_str(text).map_err(|e| SocketTableError::Parse(e.to_string()))?;
        Self::new(file)
    }

    /// Validates complete symmetric kind entries, finite positive-width lanes and unique legal pairs.
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
        let mut pairs = BTreeSet::new();
        for p in &file.pair_edge_pieces {
            let meets = file
                .compatible
                .get(&p.own)
                .is_some_and(|k| k.contains(&p.neighbour));
            if !meets || p.piece.is_empty() || !pairs.insert((p.own, p.neighbour)) {
                return Err(SocketTableError::BadPair(p.own, p.neighbour));
            }
        }
        Ok(Self { file })
    }

    /// Loads and validates the embedded `data/sockets.ron`.
    pub fn builtin() -> Result<Self, SocketTableError> {
        Self::from_ron(SOCKETS_RON)
    }

    /// Whether the kind pair is allowed, without checking lane, land-side or height descriptors.
    pub fn kinds_compatible(&self, a: SocketKind, b: SocketKind) -> bool {
        self.file
            .compatible
            .get(&a)
            .is_some_and(|partners| partners.contains(&b))
    }

    /// Whether two edges may meet across a seam: compatible kinds, matching lanes (both absent, or
    /// both present with equal width and mirrored offset, see [`LaneSpec`]), opposite land sides on
    /// directional edges (both absent, or `Left` against `Right`, see [`Side`]) and a legal height
    /// meeting ([`heights_meet`], each edge's tile at its level `a_level` / `b_level`).
    pub fn edges_compatible(
        &self,
        a: &EdgeSocket,
        a_level: u8,
        b: &EdgeSocket,
        b_level: u8,
    ) -> bool {
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
        self.kinds_compatible(a.kind, b.kind)
            && lanes
            && sides
            && heights_meet(a, a_level, b, b_level) != HeightMeet::Refused
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

    /// The pair entry for an edge whose own socket is `own` and whose neighbour's is `neighbour`.
    pub fn pair_entry(&self, own: SocketKind, neighbour: SocketKind) -> Option<&PairEdgePiece> {
        self.file
            .pair_edge_pieces
            .iter()
            .find(|p| p.own == own && p.neighbour == neighbour)
    }

    /// The pair edge piece for local edge `local_edge` of a tile (decision D3): the entry for
    /// (`own`, `neighbour`) names the family, the base's local edge picks the variant, and the
    /// corner flags (`prev_same`, `next_same`: whether local edges `local_edge - 1` and
    /// `local_edge + 1` carry the same pair) pick a taper variant under [`PairCorners::Taper`].
    pub fn pair_edge_piece(
        &self,
        own: SocketKind,
        neighbour: SocketKind,
        local_edge: u8,
        prev_same: bool,
        next_same: bool,
    ) -> Option<KitRef> {
        self.pair_entry(own, neighbour)
            .map(|p| p.name(local_edge, prev_same, next_same))
    }

    /// Every piece id the pair entries can name (the data validator checks each one).
    pub fn pair_piece_names(&self) -> BTreeSet<KitRef> {
        self.file
            .pair_edge_pieces
            .iter()
            .flat_map(PairEdgePiece::names)
            .collect()
    }

    /// The authored table backing this validated instance.
    pub fn file(&self) -> &SocketTableFile {
        &self.file
    }
}

#[cfg(test)]
mod tests {
    use super::SocketKind::*;
    use super::*;

    /// Plan F2 with owner O1 (beach never meets grass or ocean) and P3 rail (rail meets only rail,
    /// as a road does: a line never dead-ends into an edge), as unordered pairs.
    const F2_LEGAL: [(SocketKind, SocketKind); 8] = [
        (Grass, Grass),
        (Grass, Forest),
        (Forest, Forest),
        (Road, Road),
        (River, River),
        (Beach, Beach),
        (Ocean, Ocean),
        (Rail, Rail),
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
        // The rail pieces' `socket-lane-*` extras: the 3.5 m ballast lane, centred.
        assert_eq!(
            t.lane(Rail),
            Some(LaneSpec {
                offset: 0.0,
                width: 3.5
            })
        );
        assert_eq!(
            t.edge_piece(Rail),
            None,
            "rail meets the border, no edge piece"
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
        assert!(t.edges_compatible(&beach(Side::Left), 0, &beach(Side::Right), 0));
        assert!(t.edges_compatible(&beach(Side::Right), 0, &beach(Side::Left), 0));
        assert!(!t.edges_compatible(&beach(Side::Left), 0, &beach(Side::Left), 0));
        assert!(!t.edges_compatible(&beach(Side::Right), 0, &beach(Side::Right), 0));
        assert!(!t.edges_compatible(&beach(Side::Left), 0, &EdgeSocket::new(Beach), 0));
    }

    #[test]
    fn beach_never_meets_grass_or_ocean_on_either_side() {
        let t = table();
        for side in [Side::Left, Side::Right] {
            for k in [Grass, Forest, Road, River, Ocean, Rail] {
                let other = EdgeSocket {
                    land: Some(side.opposite()),
                    ..EdgeSocket::new(k)
                };
                assert!(
                    !t.edges_compatible(&beach(side), 0, &EdgeSocket::new(k), 0),
                    "{k:?}"
                );
                assert!(
                    !t.edges_compatible(&beach(side), 0, &other, 0),
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
        assert!(t.edges_compatible(&lane(Road, 0.0, 6.0), 0, &lane(Road, 0.0, 6.0), 0));
    }

    #[test]
    fn lane_offset_is_mirrored_across_the_seam() {
        let t = table();
        assert!(t.edges_compatible(&lane(Road, 1.5, 6.0), 0, &lane(Road, -1.5, 6.0), 0));
        assert!(!t.edges_compatible(&lane(Road, 1.5, 6.0), 0, &lane(Road, 1.5, 6.0), 0));
    }

    #[test]
    fn lane_width_must_match() {
        let t = table();
        assert!(!t.edges_compatible(&lane(River, 0.0, 8.0), 0, &lane(River, 0.0, 6.0), 0));
    }

    #[test]
    fn lane_against_no_lane_is_incompatible() {
        let t = table();
        assert!(!t.edges_compatible(&lane(Road, 0.0, 6.0), 0, &EdgeSocket::new(Road), 0));
    }

    #[test]
    fn compatible_kinds_without_lanes_meet() {
        let t = table();
        assert!(t.edges_compatible(&EdgeSocket::new(Grass), 0, &EdgeSocket::new(Forest), 0));
        assert!(!t.edges_compatible(&EdgeSocket::new(Grass), 0, &EdgeSocket::new(Ocean), 0));
    }

    fn flat_at(kind: SocketKind, height: u8) -> EdgeSocket {
        EdgeSocket {
            height,
            ..EdgeSocket::new(kind)
        }
    }

    fn sloped(kind: SocketKind, height: u8, profile: EdgeProfile) -> EdgeSocket {
        EdgeSocket {
            height,
            profile,
            ..EdgeSocket::new(kind)
        }
    }

    #[test]
    fn flat_equal_absolute_is_seamless() {
        let (a, b) = (flat_at(Grass, 1), flat_at(Grass, 0));
        assert_eq!(heights_meet(&a, 0, &b, 1), HeightMeet::Seamless);
        assert!(table().edges_compatible(&a, 0, &b, 1));
    }

    #[test]
    fn flat_unequal_is_cliff_without_lanes() {
        let g = EdgeSocket::new(Grass);
        assert_eq!(heights_meet(&g, 1, &g, 0), HeightMeet::Cliff);
        assert!(table().edges_compatible(&g, 1, &g, 0));
    }

    #[test]
    fn lane_edges_never_cliff() {
        let t = table();
        let road = EdgeSocket {
            lane: t.lane(Road),
            ..EdgeSocket::new(Road)
        };
        assert!(road.lane.is_some());
        assert_eq!(heights_meet(&road, 1, &road, 0), HeightMeet::Refused);
        assert!(!t.edges_compatible(&road, 1, &road, 0));
        assert!(t.edges_compatible(&road, 1, &road, 1));
    }

    #[test]
    fn rise_meets_fall_at_same_base() {
        let (rise, fall) = (
            sloped(Grass, 0, EdgeProfile::Rise),
            sloped(Grass, 0, EdgeProfile::Fall),
        );
        assert_eq!(rise.corners(), (0, 1));
        assert_eq!(fall.corners(), (1, 0));
        assert_eq!(heights_meet(&rise, 0, &fall, 0), HeightMeet::Seamless);
        assert_eq!(heights_meet(&rise, 0, &fall, 1), HeightMeet::Refused);
    }

    #[test]
    fn slope_against_flat_refused() {
        let flat = EdgeSocket::new(Grass);
        for profile in [EdgeProfile::Rise, EdgeProfile::Fall] {
            let slope = sloped(Grass, 0, profile);
            for (sl, fl) in (0..=2).flat_map(|a| (0..=2).map(move |b| (a, b))) {
                assert_eq!(heights_meet(&slope, sl, &flat, fl), HeightMeet::Refused);
                assert_eq!(heights_meet(&flat, fl, &slope, sl), HeightMeet::Refused);
            }
        }
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

    #[test]
    fn pair_resolver_names_the_variant_by_base_edge() {
        let t = table();
        for k in 0..6u8 {
            assert_eq!(
                t.pair_edge_piece(Forest, Forest, k, false, true),
                Some(KitRef(format!("forest_forest_edge_e{k}")))
            );
        }
        assert_eq!(t.pair_edge_piece(Forest, Grass, 0, true, true), None);
        assert_eq!(t.pair_edge_piece(Grass, Forest, 0, true, true), None);
        assert_eq!(t.pair_piece_names().len(), 6);
    }

    #[test]
    fn taper_corners_name_a_variant_per_flag_pair() {
        let p = PairEdgePiece {
            own: Forest,
            neighbour: Forest,
            piece: "f".into(),
            per_base_edge: true,
            corners: PairCorners::Taper,
        };
        assert_eq!(p.name(2, true, true).as_str(), "f_e2");
        assert_eq!(p.name(2, false, true).as_str(), "f_e2_t01");
        assert_eq!(p.name(2, true, false).as_str(), "f_e2_t10");
        assert_eq!(p.name(2, false, false).as_str(), "f_e2_t00");
        assert_eq!(p.names().len(), 24);
        let plain = PairEdgePiece {
            per_base_edge: false,
            corners: PairCorners::Straight,
            ..p
        };
        assert_eq!(plain.name(4, false, false).as_str(), "f");
        assert_eq!(plain.names().len(), 1);
    }

    #[test]
    fn pair_entries_must_meet_and_be_unique() {
        let mut file = table().file().clone();
        let entry = file.pair_edge_pieces[0].clone();
        file.pair_edge_pieces.push(entry.clone());
        assert_eq!(
            SocketTable::new(file.clone()),
            Err(SocketTableError::BadPair(Forest, Forest))
        );
        file.pair_edge_pieces = vec![PairEdgePiece {
            own: Forest,
            neighbour: Ocean,
            ..entry
        }];
        assert_eq!(
            SocketTable::new(file),
            Err(SocketTableError::BadPair(Forest, Ocean))
        );
    }
}
