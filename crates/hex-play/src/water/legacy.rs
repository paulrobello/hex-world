//! The river orientation rule as it stood before it moved to `hex_core::river`, frozen as the
//! oracle for the parity tests in `tiles`. Never changed: a rule change goes to hex-core.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::prelude::*;
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, NetworkKind, TileCatalog, TileDef, opposite_edge, world_edge,
};
use hex_play_state::PlayTile;

use super::tiles::{Coastline, RiverEnd, RiverPath};

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;
const ARC_STEP: f32 = std::f32::consts::PI / 90.0;

fn source_rank(e: RiverEnd) -> u8 {
    match e {
        RiverEnd::Spring => 0,
        RiverEnd::Edge(_) | RiverEnd::Junction => 1,
        RiverEnd::Mouth => 2,
    }
}

fn lane_point(def: &TileDef, local: u8) -> Vec2 {
    let s = &def.edges[local as usize];
    let n = edge_dir(local);
    n * APOTHEM + Vec2::new(-n.y, n.x) * s.lane.map_or(0.0, |l| l.offset)
}

fn river_junction(def: &TileDef, id: &str) -> bool {
    let at = |e: &Endpoint| matches!(e, Endpoint::Node(k) if k == id);
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River && (at(&n.from) || at(&n.to)))
        .count()
        >= 2
}

fn edge_dir(e: u8) -> Vec2 {
    let a = (60.0 * (e % 6) as f32).to_radians();
    Vec2::new(a.cos(), a.sin())
}

fn to_world(cell: Axial, rotation: u8, p: Vec2) -> Vec2 {
    let r = hex_render::kit_runtime::piece_rotation(rotation) * Vec3::new(p.x, 0.0, p.y);
    Vec2::from(cell.to_world(HEX_RADIUS)) + Vec2::new(r.x, r.z)
}

/// The circle through `a`, `m` and `b`, sampled from `a` to `b` at most [`ARC_STEP`] apart (one
/// control point is the arc midpoint, as in `hex_sim::geom::segment_shape`); straight if collinear.
fn arc_through(a: Vec2, m: Vec2, b: Vec2) -> Vec<Vec2> {
    let turn = (m - a).perp_dot(b - m);
    let d = 2.0 * (m - a).perp_dot(b - a);
    if turn.abs() <= 1e-6 * a.distance_squared(b) || d == 0.0 {
        return vec![a, m, b];
    }
    let (u, v) = (m - a, b - a);
    let (uu, vv) = (u.dot(u), v.dot(v));
    let c = a + Vec2::new(v.y * uu - u.y * vv, u.x * vv - v.x * uu) / d;
    let radius = a.distance(c);
    let angle = |p: Vec2| (p.y - c.y).atan2(p.x - c.x);
    let start = angle(a);
    let tau = std::f32::consts::TAU;
    let mut sweep = (angle(b) - start).rem_euclid(tau);
    if turn < 0.0 {
        sweep -= tau;
    }
    let steps = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut out = vec![a];
    for i in 1..steps {
        let t = start + sweep * i as f32 / steps as f32;
        out.push(c + Vec2::new(t.cos(), t.sin()) * radius);
    }
    out.push(b);
    out
}

/// River segments of a placed tile as (from end, to end, centre line from -> to). A mouth's
/// centre line carries on past its node by an apothem, out to sea: the fan's bed follows it.
fn river_segments(def: &TileDef, t: &PlayTile) -> Vec<(RiverEnd, RiverEnd, Vec<Vec2>)> {
    let node_end = if Coastline::of(def, t.rotation).is_some() {
        RiverEnd::Mouth
    } else {
        RiverEnd::Spring
    };
    let end = |e: &Endpoint| match e {
        Endpoint::Edge(k) => Some((
            RiverEnd::Edge(world_edge(*k, t.rotation)),
            lane_point(def, *k),
        )),
        Endpoint::Node(id) => def.nodes.iter().find(|n| &n.id == id).map(|n| {
            let kind = if river_junction(def, id) {
                RiverEnd::Junction
            } else {
                node_end
            };
            (kind, Vec2::from(n.at))
        }),
    };
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .filter_map(|n| {
            let ((a, pa), (b, pb)) = (end(&n.from)?, end(&n.to)?);
            let pts: Vec<Vec2> = n.points.iter().map(|p| Vec2::from(*p)).collect();
            let mut local = match pts.as_slice() {
                [] => vec![pa, pb],
                [m] => arc_through(pa, *m, pb),
                _ => [vec![pa], pts.clone(), vec![pb]].concat(),
            };
            let k = local.len();
            if b == RiverEnd::Mouth {
                let dir = (local[k - 1] - local[k - 2]).normalize();
                local.push(pb + dir * APOTHEM);
            } else if a == RiverEnd::Mouth {
                let dir = (local[0] - local[1]).normalize();
                local.insert(0, pa + dir * APOTHEM);
            }
            let world = local
                .into_iter()
                .map(|p| to_world(t.cell, t.rotation, p))
                .collect();
            Some((a, b, world))
        })
        .collect()
}

/// One river segment of a placed tile: (from end, to end, world centre line from -> to).
type Segment = (RiverEnd, RiverEnd, Vec<Vec2>);
/// A chain end: (cell, the end, segment index on that cell).
type ChainEnd = (Axial, RiverEnd, usize);

/// Every placed river segment, oriented so the flow runs one way along each connected chain (water
/// contract §6): each chain starts at its spring, else at its open end with the lowest
/// (cell, edge), else at its mouth; among several of the same kind, at the lowest cell. A loop
/// starts at its lowest cell. Chains that meet at a junction (`river_fork`) are oriented together
/// by `junction_sources`. Keyed by (cell, segment index among the tile's river segments).
pub fn oriented_rivers(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
) -> BTreeMap<(Axial, usize), RiverPath> {
    let mut segs: BTreeMap<(Axial, usize), Segment> = BTreeMap::new();
    let mut rotations: HashMap<Axial, u8> = HashMap::new();
    for t in tiles {
        if let Some(def) = catalog.get(&t.tile) {
            for (i, s) in river_segments(def, t).into_iter().enumerate() {
                segs.insert((t.cell, i), s);
            }
            rotations.insert(t.cell, t.rotation);
        }
    }
    // The segment across `end` of `cell`, if it is an edge with a segment ending across it.
    let across = |cell: Axial, end: RiverEnd| -> Option<(Axial, usize)> {
        let e = end.edge()?;
        let n = cell.neighbor(e);
        let back = RiverEnd::Edge(opposite_edge(e));
        segs.range((n, 0)..=(n, usize::MAX))
            .find(|(_, (a, b, _))| *a == back || *b == back)
            .map(|(k, _)| *k)
    };
    // The connected chains, each with its open ends.
    let mut chains: Vec<(Vec<(Axial, usize)>, Vec<ChainEnd>)> = vec![];
    let mut seen: BTreeSet<(Axial, usize)> = BTreeSet::new();
    for &key in segs.keys() {
        if !seen.insert(key) {
            continue;
        }
        let mut chain = vec![key];
        let mut i = 0;
        while i < chain.len() {
            let (a, b, _) = segs[&chain[i]];
            for e in [a, b] {
                if let Some(k) = across(chain[i].0, e)
                    && seen.insert(k)
                {
                    chain.push(k);
                }
            }
            i += 1;
        }
        let ends: Vec<ChainEnd> = chain
            .iter()
            .flat_map(|k| {
                let (a, b, _) = segs[k];
                [a, b]
                    .into_iter()
                    .filter(|&e| across(k.0, e).is_none())
                    .map(move |e| (k.0, e, k.1))
            })
            .collect();
        chains.push((chain, ends));
    }
    let picked = junction_sources(&chains, &segs, &rotations);
    let mut out = BTreeMap::new();
    let mut walked: BTreeSet<(Axial, usize)> = BTreeSet::new();
    for (ci, (chain, ends)) in chains.iter().enumerate() {
        let start = picked.get(&ci).copied().or_else(|| {
            ends.iter()
                .map(|&(cell, e, i)| (source_rank(e), cell, e, i))
                .min()
                .map(|(_, cell, e, i)| (cell, e, i))
        });
        let (mut cur, mut up) = match start {
            Some((cell, e, i)) => ((cell, i), e),
            None => {
                let k = *chain.iter().min().expect("non-empty chain");
                let (a, b, _) = segs[&k];
                (k, a.min(b))
            }
        };
        loop {
            walked.insert(cur);
            let (a, b, pts) = &segs[&cur];
            let (down, points) = if *a == up {
                (*b, pts.clone())
            } else {
                (*a, pts.iter().rev().copied().collect())
            };
            out.insert(cur, RiverPath { points, up, down });
            match (across(cur.0, down), down) {
                (Some(next), RiverEnd::Edge(e)) if !walked.contains(&next) => {
                    up = RiverEnd::Edge(opposite_edge(e));
                    cur = next;
                }
                _ => break,
            }
        }
    }
    out
}

/// A node of the river network that chains join at: a junction (one per tile, keyed by its cell)
/// or a chain's open end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NetVertex {
    Junction(Axial),
    End(Axial, RiverEnd, usize),
}

impl NetVertex {
    fn of(&(cell, e, i): &ChainEnd) -> Self {
        if e == RiverEnd::Junction {
            NetVertex::Junction(cell)
        } else {
            NetVertex::End(cell, e, i)
        }
    }
}

/// The upstream end of every chain that reaches a junction (`river_fork`), by chain index (water
/// contract §6, junctions). Chains joined at junctions form one network, and every chain in it runs
/// downhill, measured in metres along the centre lines:
/// - toward the nearest mouth when the network reaches one, so a fork with one arm toward the sea
///   is a confluence and a fork with two arms that each reach the sea first is a split;
/// - else away from the nearest spring, so one spring through a fork splits and two springs join;
/// - else out through the lowest junction's local edge 0 arm, as if the sea lay past that arm's
///   open end (the lowest open end when that arm meets another junction): a lone fork is a
///   confluence whose rotation picks the outlet.
///
/// A chain whose two ends are equally far keeps the plain chain rule of [`oriented_rivers`].
fn junction_sources(
    chains: &[(Vec<(Axial, usize)>, Vec<ChainEnd>)],
    segs: &BTreeMap<(Axial, usize), Segment>,
    rotations: &HashMap<Axial, u8>,
) -> BTreeMap<usize, ChainEnd> {
    let length = |chain: &[(Axial, usize)]| -> f32 {
        chain
            .iter()
            .map(|k| {
                let pts = &segs[k].2;
                pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>()
            })
            .sum()
    };
    // Linear chains with a junction end: (chain index, end a, end b, length).
    let links: Vec<(usize, ChainEnd, ChainEnd, f32)> = chains
        .iter()
        .enumerate()
        .filter_map(|(ci, (chain, ends))| {
            let [a, b] = ends[..] else {
                return None;
            };
            (a.1 == RiverEnd::Junction || b.1 == RiverEnd::Junction)
                .then(|| (ci, a, b, length(chain)))
        })
        .collect();
    let mut out = BTreeMap::new();
    let mut done = vec![false; links.len()];
    for first in 0..links.len() {
        if done[first] {
            continue;
        }
        // The network: links joined at their junctions.
        done[first] = true;
        let mut net = vec![first];
        let mut i = 0;
        while i < net.len() {
            let (_, a, b, _) = links[net[i]];
            for v in [NetVertex::of(&a), NetVertex::of(&b)] {
                if !matches!(v, NetVertex::Junction(_)) {
                    continue;
                }
                for (j, l) in links.iter().enumerate() {
                    if !done[j] && (NetVertex::of(&l.1) == v || NetVertex::of(&l.2) == v) {
                        done[j] = true;
                        net.push(j);
                    }
                }
            }
            i += 1;
        }
        let ends: Vec<ChainEnd> = net
            .iter()
            .flat_map(|&j| [links[j].1, links[j].2])
            .filter(|e| e.1 != RiverEnd::Junction)
            .collect();
        let leaves = |kind: fn(&RiverEnd) -> bool| -> Vec<NetVertex> {
            ends.iter()
                .filter(|e| kind(&e.1))
                .map(NetVertex::of)
                .collect()
        };
        let mouths = leaves(|e| *e == RiverEnd::Mouth);
        let springs = leaves(|e| *e == RiverEnd::Spring);
        // The distance seeds, and whether the flow runs toward them or away from them.
        let (seeds, toward) = if !mouths.is_empty() {
            (mouths, true)
        } else if !springs.is_empty() {
            (springs, false)
        } else {
            let Some(outlet) = open_outlet(&net, &links, chains, segs, rotations) else {
                continue;
            };
            (vec![outlet], true)
        };
        let mut dist: BTreeMap<NetVertex, f32> = seeds.iter().map(|&v| (v, 0.0)).collect();
        loop {
            let mut changed = false;
            for &j in &net {
                let (_, a, b, len) = links[j];
                let (va, vb) = (NetVertex::of(&a), NetVertex::of(&b));
                for (x, y) in [(va, vb), (vb, va)] {
                    let Some(&dx) = dist.get(&x) else {
                        continue;
                    };
                    if dist.get(&y).is_none_or(|&dy| dx + len < dy - 1e-4) {
                        dist.insert(y, dx + len);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for &j in &net {
            let (ci, a, b, _) = links[j];
            let (Some(&da), Some(&db)) =
                (dist.get(&NetVertex::of(&a)), dist.get(&NetVertex::of(&b)))
            else {
                continue;
            };
            if (da - db).abs() < 1e-3 {
                continue;
            }
            out.insert(ci, if (da > db) == toward { a } else { b });
        }
    }
    out
}

/// The outlet of a junction network with neither a mouth nor a spring: the open end of the chain
/// that leaves the lowest junction through its local edge 0, else the network's lowest open end.
fn open_outlet(
    net: &[usize],
    links: &[(usize, ChainEnd, ChainEnd, f32)],
    chains: &[(Vec<(Axial, usize)>, Vec<ChainEnd>)],
    segs: &BTreeMap<(Axial, usize), Segment>,
    rotations: &HashMap<Axial, u8>,
) -> Option<NetVertex> {
    let junction = net
        .iter()
        .flat_map(|&j| [links[j].1, links[j].2])
        .filter(|e| e.1 == RiverEnd::Junction)
        .map(|e| e.0)
        .min()?;
    let out_edge = RiverEnd::Edge(world_edge(
        0,
        rotations.get(&junction).copied().unwrap_or(0),
    ));
    let arm = segs
        .range((junction, 0)..=(junction, usize::MAX))
        .find(|(_, (a, b, _))| *a == out_edge || *b == out_edge)
        .map(|(k, _)| *k);
    let far = arm.and_then(|arm| {
        let &(_, a, b, _) = links.iter().find(|l| chains[l.0].0.contains(&arm))?;
        let far = if a == (junction, RiverEnd::Junction, arm.1) {
            b
        } else {
            a
        };
        matches!(far.1, RiverEnd::Edge(_)).then(|| NetVertex::of(&far))
    });
    far.or_else(|| {
        net.iter()
            .flat_map(|&j| [links[j].1, links[j].2])
            .filter(|e| matches!(e.1, RiverEnd::Edge(_)))
            .min()
            .map(|e| NetVertex::of(&e))
    })
}
