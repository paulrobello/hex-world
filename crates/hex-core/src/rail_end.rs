//! Buffer stops at open rail ends (owner D2, 2026-10-20). The rule is
//! `docs/checkpoints/rail_buffer_stop/runtime-notes.md`: an open end is a rail lane edge whose
//! neighbour has no rail lane back; its stop's pivot is the track point [`STOP_U`] in from the edge
//! along the centreline trains run, at the cap height, and its local +X points along the chord to
//! the track point [`STOP_CHORD`] nearer the edge (the buffers look along -X, at the train). At a
//! turnout's toe the walk in from the edge crosses the short toe segment onto the main (straight)
//! leg.
//!
//! Everything here is in the tile frame at rotation 0. A placed tile's root carries its cell and
//! rotation, so a stop is a child of the root at its local pose; `hex-sim` turns the same poses
//! into world space for the trains.

use crate::{Axial, Endpoint, HEX_RADIUS, NetworkKind, SocketKind, TileDef, path::segment_shape};

type Vec2 = [f32; 2];

/// The buffer stop kit piece.
pub const BUFFER_STOP: &str = "rail_buffer_stop";
/// The pivot's distance in from the edge midpoint along the track (m): where a train's buffers
/// meet the stop's (its buffer contact plane is the pivot plane).
pub const STOP_U: f32 = 3.5;
/// The heading chord (m): from the pivot toward the edge, to the track point this much nearer it.
pub const STOP_CHORD: f32 = 2.24;
/// The longest buffer-to-outer-axle overhang of the rolling stock (m): the engine's 4.47 m half
/// length less its 2.0 m half wheelbase (`hex_sim::rail::CONSIST`, whose tests check the two agree).
/// At a station's platform end the stop stands this much short of the level section, so the
/// leading axle of a train at the stop is still on it.
pub const STATION_LEAD: f32 = 2.47;

/// A buffer stop in the tile frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BufferStop {
    /// The local edge whose open end it closes.
    pub edge: u8,
    /// Pivot, tile-local XZ meters.
    pub pos: Vec2,
    /// The piece's local +X (toward the edge), a unit vector.
    pub dir: Vec2,
}

impl BufferStop {
    /// Heading in radians from +x toward +z (the yaw a transform turns +x to).
    pub fn yaw(&self) -> f32 {
        self.dir[1].atan2(self.dir[0])
    }
}

/// Whether local edge `edge` of `def` carries a rail lane.
pub fn rail_edge(def: &TileDef, edge: u8) -> bool {
    def.edges
        .get(edge as usize)
        .is_some_and(|s| s.kind == SocketKind::Rail && s.lane.is_some())
}

/// Whether world edge `edge` of a tile is open: the tile across it (with its rotation) has no rail
/// lane back, or there is none.
pub fn open_across(neighbour: Option<(&TileDef, u8)>, edge: u8) -> bool {
    neighbour.is_none_or(|(n, rot)| {
        let back = (edge + 3) % 6;
        let local = (back + 6 - rot % 6) % 6;
        !rail_edge(n, local)
    })
}

/// The stops at the open ends of the tile `def` placed at `cell` with `rotation`, in its local
/// frame. `at` gives the tile at a cell.
pub fn open_stops<'a>(
    def: &TileDef,
    cell: Axial,
    rotation: u8,
    at: impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> Vec<BufferStop> {
    buffer_stops(def)
        .into_iter()
        .filter(|s| {
            let world = (s.edge + rotation) % 6;
            open_across(at(cell.neighbor(world)), world)
        })
        .collect()
}

/// A stop for every rail lane edge of `def`, as if every end were open, in the tile frame.
pub fn buffer_stops(def: &TileDef) -> Vec<BufferStop> {
    #[derive(Clone, PartialEq)]
    enum Key<'a> {
        Edge(u8),
        Node(&'a str),
    }
    fn key(e: &Endpoint) -> Key<'_> {
        match e {
            Endpoint::Edge(k) => Key::Edge(*k),
            Endpoint::Node(id) => Key::Node(id.as_str()),
        }
    }
    let pos = |e: &Endpoint| -> Option<Vec2> {
        match e {
            Endpoint::Edge(k) => {
                let offset = def.edges.get(*k as usize)?.lane.map_or(0.0, |l| l.offset);
                Some(lane_point(*k, offset))
            }
            Endpoint::Node(id) => def.nodes.iter().find(|n| &n.id == id).map(|n| n.at),
        }
    };
    let segs: Vec<(usize, Key, Key, Vec<Vec2>)> = def
        .networks
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == NetworkKind::Rail)
        .filter_map(|(i, s)| {
            let pts = segment_shape(pos(&s.from)?, &s.points, pos(&s.to)?);
            Some((i, key(&s.from), key(&s.to), pts))
        })
        .collect();
    let mut out = vec![];
    for edge in (0..6u8).filter(|&e| rail_edge(def, e)) {
        let mut at = Key::Edge(edge);
        let mut from: Option<usize> = None;
        let mut path: Vec<Vec2> = vec![];
        // The first segment walked in from the edge and whether the walk runs from its `from` end.
        let mut first: Option<(usize, bool)> = None;
        loop {
            let toe_straight = match &at {
                Key::Node(id) => def
                    .switches
                    .iter()
                    .find(|sw| sw.node == *id && from == Some(sw.toe as usize))
                    .map(|sw| sw.straight as usize),
                Key::Edge(_) => None,
            };
            let next = segs
                .iter()
                .filter(|(i, a, b, _)| Some(*i) != from && (*a == at || *b == at))
                .find(|(i, ..)| toe_straight.is_none_or(|s| s == *i));
            let Some((i, a, b, pts)) = next else {
                break;
            };
            let mut pts = pts.clone();
            first.get_or_insert((*i, *a == at));
            let far = if *a == at {
                b.clone()
            } else {
                pts.reverse();
                a.clone()
            };
            if !path.is_empty() {
                pts.remove(0);
            }
            path.extend(pts);
            from = Some(*i);
            at = far;
            if length(&path) >= STOP_U || matches!(at, Key::Edge(_)) {
                break;
            }
        }
        let u = first.map_or(STOP_U, |(i, forward)| stop_u(def, i, forward, &segs[..]));
        if length(&path) < u {
            continue;
        }
        let p = along(&path, u);
        let q = along(&path, (u - STOP_CHORD).max(0.0));
        let d = [q[0] - p[0], q[1] - p[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
        out.push(BufferStop {
            edge,
            pos: p,
            dir: [d[0] / l, d[1] / l],
        });
    }
    out
}

/// How far in from its edge a stop stands on segment `seg` of `def`, walked in from its `from` end
/// when `forward`: [`STOP_U`], or at a station's platform end ([`crate::StopDef`]) close enough
/// to the edge that a train centred on the level section clears it, so every axle stays on the level
/// section (owner D2 follow-up): the level section's near end less [`STATION_LEAD`].
fn stop_u<K>(def: &TileDef, seg: usize, forward: bool, segs: &[(usize, K, K, Vec<Vec2>)]) -> f32 {
    let Some(stop) = def.stops.iter().find(|s| s.segment as usize == seg) else {
        return STOP_U;
    };
    let len = segs
        .iter()
        .find(|(i, ..)| *i == seg)
        .map_or(0.0, |(.., pts)| length(pts));
    let near = if forward {
        stop.level[0]
    } else {
        len - stop.level[1]
    };
    STOP_U.min(near - STATION_LEAD).max(0.0)
}

/// The lane crossing local edge `edge`, `offset` meters along the edge tangent from its midpoint.
fn lane_point(edge: u8, offset: f32) -> Vec2 {
    let a = (60.0 * f32::from(edge % 6)).to_radians();
    let n = [a.cos(), a.sin()];
    let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
    [
        n[0] * apothem - n[1] * offset,
        n[1] * apothem + n[0] * offset,
    ]
}

fn length(points: &[Vec2]) -> f32 {
    points
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .sum()
}

/// The point `s` meters along a polyline.
fn along(points: &[Vec2], s: f32) -> Vec2 {
    let mut left = s.max(0.0);
    for w in points.windows(2) {
        let l = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        if l > 0.0 && left <= l {
            let t = left / l;
            return [
                w[0][0] + (w[1][0] - w[0][0]) * t,
                w[0][1] + (w[1][1] - w[0][1]) * t,
            ];
        }
        left -= l;
    }
    points.last().copied().unwrap_or([0.0, 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileCatalog;

    fn stop(tile: &str, edge: u8) -> BufferStop {
        let c = TileCatalog::builtin().unwrap();
        *buffer_stops(c.get(tile).unwrap())
            .iter()
            .find(|s| s.edge == edge)
            .unwrap_or_else(|| panic!("{tile} edge {edge}"))
    }

    /// The pivots and yaws of the runtime notes' table (measured by the asset pass against each
    /// track piece's LOD0; `docs/checkpoints/rail_buffer_stop/poses.py`).
    #[test]
    fn stops_stand_where_the_runtime_notes_measured_them() {
        let table = [
            ("rail_straight", 0, [10.356, 0.0], 0.0),
            ("rail_straight", 3, [-10.356, 0.0], 180.0),
            ("rail_bend_60", 0, [10.467, 0.753], -17.05),
            ("rail_bend_60", 1, [5.886, 8.688], 77.05),
            ("rail_junction", 3, [-10.356, 0.0], 180.0),
            ("rail_junction", 1, [5.886, 8.688], 77.05),
            ("rail_junction", 0, [10.356, 0.0], 0.0),
            ("rail_road_crossing", 0, [10.356, 0.0], 0.0),
            ("river_rail_bridge", 1, [5.178, 8.968], 60.0),
            // The platform end: the level section starts 3.856 m in, so the stop stands at 1.386 m.
            ("rail_station", 0, [12.47, 0.0], 0.0),
            ("rail_station", 3, [-12.47, 0.0], 180.0),
        ];
        for (tile, edge, at, yaw) in table {
            let s = stop(tile, edge);
            let dyaw = (s.yaw().to_degrees() - yaw + 540.0).rem_euclid(360.0) - 180.0;
            assert!(
                (s.pos[0] - at[0]).abs() < 0.02 && (s.pos[1] - at[1]).abs() < 0.02,
                "{tile} edge {edge}: pivot {:?}, want {at:?}",
                s.pos
            );
            assert!(
                dyaw.abs() < 0.2,
                "{tile} edge {edge}: yaw {}",
                s.yaw().to_degrees()
            );
        }
    }

    #[test]
    fn every_rail_lane_edge_gets_a_stop_and_nothing_else_does() {
        let c = TileCatalog::builtin().unwrap();
        for def in c.tiles() {
            let edges: Vec<u8> = buffer_stops(def).iter().map(|s| s.edge).collect();
            let want: Vec<u8> = (0..6).filter(|&e| rail_edge(def, e)).collect();
            assert_eq!(edges, want, "{}", def.id);
        }
    }

    /// A rail neighbour with its lane facing back closes the end; a rotated one facing away, a
    /// grass tile or an empty cell leave it open.
    #[test]
    fn only_a_rail_lane_back_closes_an_end() {
        let c = TileCatalog::builtin().unwrap();
        let straight = c.get("rail_straight").unwrap();
        let meadow = c.tiles().iter().find(|t| t.networks.is_empty()).unwrap();
        assert!(!open_across(Some((straight, 0)), 0));
        assert!(!open_across(Some((straight, 3)), 0));
        assert!(open_across(Some((straight, 1)), 0));
        assert!(open_across(Some((meadow, 0)), 0));
        assert!(open_across(None, 0));
        let two = |cell: Axial| (cell == Axial::new(1, 0)).then_some((straight, 0u8));
        let open = open_stops(straight, Axial::ORIGIN, 0, two);
        assert_eq!(open.iter().map(|s| s.edge).collect::<Vec<_>>(), vec![3]);
    }
}
