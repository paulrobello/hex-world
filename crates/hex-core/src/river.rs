//! River flow (water contract §6): which way each placed river segment runs, and the waterfall
//! seams where a river drops a level. Tile-local geometry is plain `[f32; 2]` math whose operation
//! order matches the glam code this rule was ported from, so equal arms measure bit-equal lengths.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::socket::river_fall;
use crate::{
    Axial, Endpoint, HEX_RADIUS, NetworkKind, SocketKind, TileDef, opposite_edge, world_edge,
};

/// Distance from a tile centre to an edge midpoint (a lane point at offset 0).
const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;
/// Largest angle one river arc sample spans (2°), as the water meshes sample it.
const ARC_STEP: f32 = std::f32::consts::PI / 90.0;

/// Where a river segment ends: a world edge's lane, a spring (a node on a tile without a
/// coastline), a mouth (a node on a coast tile) or a junction (a node where two or more of the
/// tile's River segments meet; the tile's arms continue each other there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiverEnd {
    Edge(u8),
    Spring,
    Mouth,
    Junction,
}

impl RiverEnd {
    pub fn edge(self) -> Option<u8> {
        match self {
            RiverEnd::Edge(e) => Some(e),
            _ => None,
        }
    }

    /// Which end a chain's flow starts from: a spring, else an open edge, else a mouth. Chains
    /// that reach a junction are oriented by [`junction_sources`] instead.
    fn source_rank(self) -> u8 {
        match self {
            RiverEnd::Spring => 0,
            RiverEnd::Edge(_) | RiverEnd::Junction => 1,
            RiverEnd::Mouth => 2,
        }
    }
}

/// A placed tile as the river rule sees it.
#[derive(Clone, Copy, Debug)]
pub struct RiverTile<'a> {
    pub cell: Axial,
    pub def: &'a TileDef,
    pub rotation: u8,
    pub level: u8,
}

/// One oriented river segment: where the flow enters and leaves the tile, the centre line's
/// length in metres, and whether the flow runs against the segment's authored `from -> to`.
#[derive(Clone, Debug, PartialEq)]
pub struct RiverSeg {
    pub up: RiverEnd,
    pub down: RiverEnd,
    pub length: f32,
    pub reversed: bool,
}

/// A river dropping from `upper` across its world edge `edge` into `lower`, `drop` levels down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Waterfall {
    pub upper: Axial,
    pub edge: u8,
    pub lower: Axial,
    pub drop: u8,
}

/// The oriented river network: every segment keyed by (cell, index among the tile's river
/// segments), and the waterfall seams.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rivers {
    pub segs: BTreeMap<(Axial, usize), RiverSeg>,
    pub falls: Vec<Waterfall>,
}

type P = [f32; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: P, s: f32) -> P {
    [a[0] * s, a[1] * s]
}

fn dot(a: P, b: P) -> f32 {
    (a[0] * b[0]) + (a[1] * b[1])
}

fn perp_dot(a: P, b: P) -> f32 {
    (a[0] * b[1]) - (a[1] * b[0])
}

fn distance(a: P, b: P) -> f32 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn normalize(a: P) -> P {
    scale(a, 1.0 / dot(a, a).sqrt())
}

fn edge_dir(e: u8) -> P {
    let a = (60.0 * (e % 6) as f32).to_radians();
    [a.cos(), a.sin()]
}

/// The lane point of local edge `local` (tile-local, unrotated).
fn lane_point(def: &TileDef, local: u8) -> P {
    let s = &def.edges[local as usize];
    let n = edge_dir(local);
    add(
        scale(n, APOTHEM),
        scale([-n[1], n[0]], s.lane.map_or(0.0, |l| l.offset)),
    )
}

/// Whether `def` placed at `rotation` has a coastline: two beach edges, one with land on the
/// next edge clockwise and one with land on the edge before it, 2, 3 or 4 edges apart.
fn has_coastline(def: &TileDef, rotation: u8) -> bool {
    let kind = |w: u8| def.edge(w % 6, rotation).kind;
    let land = |w: u8| {
        matches!(
            kind(w),
            SocketKind::Grass | SocketKind::Forest | SocketKind::River
        )
    };
    let beach: Vec<u8> = (0..6).filter(|&w| kind(w) == SocketKind::Beach).collect();
    let [a, b] = beach[..] else {
        return false;
    };
    let (right, left) = if land(a + 1) { (a, b) } else { (b, a) };
    land(right + 1) && land(left + 5) && matches!((left + 6 - right) % 6, 2..=4)
}

/// Whether node `id` of `def` joins two or more River segments: neither a spring nor a mouth.
fn river_junction(def: &TileDef, id: &str) -> bool {
    let at = |e: &Endpoint| matches!(e, Endpoint::Node(k) if k == id);
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River && (at(&n.from) || at(&n.to)))
        .count()
        >= 2
}

/// The circle through `a`, `m` and `b`, sampled from `a` to `b` at most [`ARC_STEP`] apart;
/// straight if collinear.
fn arc_through(a: P, m: P, b: P) -> Vec<P> {
    let turn = perp_dot(sub(m, a), sub(b, m));
    let d = 2.0 * perp_dot(sub(m, a), sub(b, a));
    let ab = sub(a, b);
    if turn.abs() <= 1e-6 * dot(ab, ab) || d == 0.0 {
        return vec![a, m, b];
    }
    let (u, v) = (sub(m, a), sub(b, a));
    let (uu, vv) = (dot(u, u), dot(v, v));
    let c = add(
        a,
        [(v[1] * uu - u[1] * vv) / d, (u[0] * vv - v[0] * uu) / d],
    );
    let radius = distance(a, c);
    let angle = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]);
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
        out.push(add(c, scale([t.cos(), t.sin()], radius)));
    }
    out.push(b);
    out
}

/// The River segments of `def` placed at `rotation`, in authored order, as (from end, to end,
/// centre line length in metres). A mouth's centre line carries on past its node by an apothem,
/// out to sea, as the fan's bed does, and its length includes that reach.
pub fn river_segment_ends(def: &TileDef, rotation: u8) -> Vec<(RiverEnd, RiverEnd, f32)> {
    river_segment_points(def, rotation)
        .into_iter()
        .map(|(a, b, local)| {
            let length = local.windows(2).map(|w| distance(w[0], w[1])).sum::<f32>();
            (a, b, length)
        })
        .collect()
}

/// The River segments of `def` placed at `rotation`, in the order of [`river_segment_ends`], as
/// (from end, to end, centre line from -> to in tile-local unrotated metres). Bends are sampled
/// at most 2° apart; a mouth's line carries on past its node by an apothem.
pub fn river_segment_points(
    def: &TileDef,
    rotation: u8,
) -> Vec<(RiverEnd, RiverEnd, Vec<[f32; 2]>)> {
    let node_end = if has_coastline(def, rotation) {
        RiverEnd::Mouth
    } else {
        RiverEnd::Spring
    };
    let end = |e: &Endpoint| match e {
        Endpoint::Edge(k) => Some((
            RiverEnd::Edge(world_edge(*k, rotation)),
            lane_point(def, *k),
        )),
        Endpoint::Node(id) => def.nodes.iter().find(|n| &n.id == id).map(|n| {
            let kind = if river_junction(def, id) {
                RiverEnd::Junction
            } else {
                node_end
            };
            (kind, n.at)
        }),
    };
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .filter_map(|n| {
            let ((a, pa), (b, pb)) = (end(&n.from)?, end(&n.to)?);
            let mut local = match n.points.as_slice() {
                [] => vec![pa, pb],
                [m] => arc_through(pa, *m, pb),
                pts => [vec![pa], pts.to_vec(), vec![pb]].concat(),
            };
            let k = local.len();
            if b == RiverEnd::Mouth {
                let dir = normalize(sub(local[k - 1], local[k - 2]));
                local.push(add(pb, scale(dir, APOTHEM)));
            } else if a == RiverEnd::Mouth {
                let dir = normalize(sub(local[0], local[1]));
                local.insert(0, add(pa, scale(dir, APOTHEM)));
            }
            Some((a, b, local))
        })
        .collect()
}

/// One river segment of a placed tile: (from end, to end, length).
type Segment = (RiverEnd, RiverEnd, f32);
/// A chain end: (cell, the end, segment index on that cell).
type ChainEnd = (Axial, RiverEnd, usize);
/// A connected chain of segments and its open ends.
type Chain = (Vec<(Axial, usize)>, Vec<ChainEnd>);

/// A river seam that would make a network run uphill: the upper cell and world edge of the first
/// offending waterfall, in (cell, edge) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uphill {
    pub cell: Axial,
    pub edge: u8,
}

/// A segment key: (cell, index among the tile's river segments).
type Key = (Axial, usize);

/// Every placed river segment, oriented so the flow runs one way along each connected chain, and
/// the waterfall seams (spec §1, rivers at levels).
///
/// A chain crossing a waterfall runs from the fall's high side to its low side. A chain without
/// one starts at its spring, else at its open end with the lowest (cell, edge), else at its
/// mouth; among several of the same kind, at the lowest cell. A loop starts at its lowest cell.
/// Chains that meet at a junction are oriented together by [`junction_sources`]. A chain a
/// waterfall fixes keeps that direction, even against a chain with no spring or mouth (RQ12 O1),
/// and seeds the free arms of its junction network.
///
/// A network without a waterfall is always legal. With one, it is [`Uphill`] when a chain
/// crosses waterfalls facing both ways (a loop always does) or when a fixed chain flows into a
/// spring at its end or out of a mouth at its end. Rule 2 of the spec (no directed cycle through a
/// waterfall) needs no check of its own: river edges sit at height 0, so a linked seam between
/// tiles at different levels is always a waterfall, levels strictly fall along a fixed chain and
/// hold along a free one, and no cycle can return to the level it fell from.
pub fn orient(tiles: &[RiverTile]) -> Result<Rivers, Uphill> {
    let mut segs: BTreeMap<Key, Segment> = BTreeMap::new();
    let mut placed: HashMap<Axial, RiverTile> = HashMap::new();
    for t in tiles {
        for (i, s) in river_segment_ends(t.def, t.rotation)
            .into_iter()
            .enumerate()
        {
            segs.insert((t.cell, i), s);
        }
        placed.insert(t.cell, *t);
    }
    let rotations: HashMap<Axial, u8> = placed.iter().map(|(c, t)| (*c, t.rotation)).collect();
    let chains = chains(&segs);
    let falls = waterfalls(&segs, &placed);
    let fixed = fixed_starts(&chains, &segs, &falls)?;
    let picked = junction_sources(&chains, &segs, &rotations, &fixed);
    let mut out = BTreeMap::new();
    for (ci, (chain, ends)) in chains.iter().enumerate() {
        let start = fixed
            .get(&ci)
            .or_else(|| picked.get(&ci))
            .copied()
            .or_else(|| {
                ends.iter()
                    .map(|&(cell, e, i)| (e.source_rank(), cell, e, i))
                    .min()
                    .map(|(_, cell, e, i)| (cell, e, i))
            });
        let (key, up) = match start {
            Some((cell, e, i)) => ((cell, i), e),
            None => {
                let k = *chain.iter().min().expect("non-empty chain");
                let (a, b, _) = segs[&k];
                (k, a.min(b))
            }
        };
        out.extend(walk(&segs, key, up));
    }
    Ok(Rivers {
        segs: out,
        falls: falls.into_values().collect(),
    })
}

/// The segment across `end` of `cell`, if it is an edge with a segment ending across it.
fn across(segs: &BTreeMap<Key, Segment>, cell: Axial, end: RiverEnd) -> Option<Key> {
    let e = end.edge()?;
    let n = cell.neighbor(e);
    let back = RiverEnd::Edge(opposite_edge(e));
    segs.range((n, 0)..=(n, usize::MAX))
        .find(|(_, (a, b, _))| *a == back || *b == back)
        .map(|(k, _)| *k)
}

/// The connected chains of segments, each with its open ends (none for a loop), in key order.
fn chains(segs: &BTreeMap<Key, Segment>) -> Vec<Chain> {
    let mut chains: Vec<Chain> = vec![];
    let mut seen: BTreeSet<Key> = BTreeSet::new();
    for &key in segs.keys() {
        if !seen.insert(key) {
            continue;
        }
        let mut chain = vec![key];
        let mut i = 0;
        while i < chain.len() {
            let (a, b, _) = segs[&chain[i]];
            for e in [a, b] {
                if let Some(k) = across(segs, chain[i].0, e)
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
                    .filter(|&e| across(segs, k.0, e).is_none())
                    .map(move |e| (k.0, e, k.1))
            })
            .collect();
        chains.push((chain, ends));
    }
    chains
}

/// The chain from segment `key` entering at `up`, oriented downstream, segment by segment.
fn walk(segs: &BTreeMap<Key, Segment>, key: Key, up: RiverEnd) -> Vec<(Key, RiverSeg)> {
    let (mut cur, mut up) = (key, up);
    let mut out: Vec<(Key, RiverSeg)> = vec![];
    let mut walked: BTreeSet<Key> = BTreeSet::new();
    loop {
        walked.insert(cur);
        let (a, b, length) = segs[&cur];
        let reversed = a != up;
        let down = if reversed { a } else { b };
        out.push((
            cur,
            RiverSeg {
                up,
                down,
                length,
                reversed,
            },
        ));
        match (across(segs, cur.0, down), down) {
            (Some(next), RiverEnd::Edge(e)) if !walked.contains(&next) => {
                up = RiverEnd::Edge(opposite_edge(e));
                cur = next;
            }
            _ => break,
        }
    }
    out
}

/// Every waterfall seam, keyed by (upper cell, world edge): two linked segments whose River lane
/// edges meet as a waterfall ([`river_fall`]).
fn waterfalls(
    segs: &BTreeMap<Key, Segment>,
    placed: &HashMap<Axial, RiverTile>,
) -> BTreeMap<(Axial, u8), Waterfall> {
    let mut out = BTreeMap::new();
    for (&(cell, _), &(a, b, _)) in segs {
        for e in [a, b].into_iter().filter_map(RiverEnd::edge) {
            let Some((n, _)) = across(segs, cell, RiverEnd::Edge(e)) else {
                continue;
            };
            let (hi, lo) = (placed[&cell], placed[&n]);
            if let Some((drop, true)) = river_fall(
                hi.def.edge(e, hi.rotation),
                hi.level,
                lo.def.edge(opposite_edge(e), lo.rotation),
                lo.level,
            ) {
                out.insert(
                    (cell, e),
                    Waterfall {
                        upper: cell,
                        edge: e,
                        lower: n,
                        drop,
                    },
                );
            }
        }
    }
    out
}

/// The (cell, world edge) seams a downstream walk crosses, in flow order.
fn crossings(walked: &[(Key, RiverSeg)]) -> BTreeSet<(Axial, u8)> {
    walked
        .iter()
        .skip(1)
        .map(|((cell, _), seg)| {
            let e = seg
                .up
                .edge()
                .expect("a walk enters a later segment across an edge");
            (cell.neighbor(e), opposite_edge(e))
        })
        .collect()
}

/// The upstream end of every chain a waterfall fixes, by chain index, or the first offending seam
/// when the waterfalls admit no legal orientation (see [`orient`]).
fn fixed_starts(
    chains: &[Chain],
    segs: &BTreeMap<Key, Segment>,
    falls: &BTreeMap<(Axial, u8), Waterfall>,
) -> Result<BTreeMap<usize, ChainEnd>, Uphill> {
    let chain_of: HashMap<Key, usize> = chains
        .iter()
        .enumerate()
        .flat_map(|(ci, (chain, _))| chain.iter().map(move |&k| (k, ci)))
        .collect();
    let mut chain_falls: BTreeMap<usize, Vec<(Axial, u8)>> = BTreeMap::new();
    for &(cell, e) in falls.keys() {
        let key = segs
            .range((cell, 0)..=(cell, usize::MAX))
            .find(|(_, (a, b, _))| *a == RiverEnd::Edge(e) || *b == RiverEnd::Edge(e))
            .map(|(k, _)| *k)
            .expect("a waterfall leaves a segment");
        chain_falls
            .entry(chain_of[&key])
            .or_default()
            .push((cell, e));
    }
    let mut fixed: BTreeMap<usize, (ChainEnd, ChainEnd)> = BTreeMap::new();
    let mut bad: Vec<(Axial, u8)> = vec![];
    for (&ci, seams) in &chain_falls {
        let ends = &chains[ci].1;
        let descends = |&(cell, e, i): &ChainEnd| {
            let crossed = crossings(&walk(segs, (cell, i), e));
            seams.iter().all(|s| crossed.contains(s))
        };
        let pick = match ends[..] {
            [a, b] if descends(&a) => Some((a, b)),
            [a, b] if descends(&b) => Some((b, a)),
            _ => None,
        };
        match pick {
            Some((up, down)) if up.1 != RiverEnd::Mouth && down.1 != RiverEnd::Spring => {
                fixed.insert(ci, (up, down));
            }
            _ => bad.extend(seams),
        }
    }
    match bad.into_iter().min() {
        Some((cell, edge)) => Err(Uphill { cell, edge }),
        None => Ok(fixed.into_iter().map(|(ci, (up, _))| (ci, up)).collect()),
    }
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

/// The upstream end of every chain that reaches a junction, by chain index (water contract §6,
/// junctions). Chains joined at junctions form one network, and every chain in it runs downhill,
/// measured in metres along the centre lines:
/// - toward the nearest mouth when the network reaches one, so a fork with one arm toward the sea
///   is a confluence and a fork with two arms that each reach the sea first is a split;
/// - else away from the nearest spring, so one spring through a fork splits and two springs join;
/// - else out through the lowest junction's local edge 0 arm, as if the sea lay past that arm's
///   open end (the lowest open end when that arm meets another junction): a lone fork is a
///   confluence whose rotation picks the outlet.
///
/// A chain whose two ends are equally far keeps the plain chain rule of [`orient`].
///
/// A chain a waterfall fixes (`fixed`, its upstream end by chain index) is cut out of its network,
/// and each part left is oriented on its own: the fixed chain's upstream junction counts as a mouth
/// there and its downstream junction as a spring, so water runs to the lip and on from the pool.
fn junction_sources(
    chains: &[Chain],
    segs: &BTreeMap<(Axial, usize), Segment>,
    rotations: &HashMap<Axial, u8>,
    fixed: &BTreeMap<usize, ChainEnd>,
) -> BTreeMap<usize, ChainEnd> {
    let length = |chain: &[(Axial, usize)]| -> f32 { chain.iter().map(|k| segs[k].2).sum() };
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
    // A fixed link is cut out of the distance graph: its sides are oriented apart.
    let mut done: Vec<bool> = links.iter().map(|l| fixed.contains_key(&l.0)).collect();
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
        let mut mouths = leaves(|e| *e == RiverEnd::Mouth);
        let mut springs = leaves(|e| *e == RiverEnd::Spring);
        // A fixed link drains its upstream junction, as a mouth would, and feeds its downstream
        // one, as a spring would.
        let joints: BTreeSet<NetVertex> = net
            .iter()
            .flat_map(|&j| [NetVertex::of(&links[j].1), NetVertex::of(&links[j].2)])
            .filter(|v| matches!(v, NetVertex::Junction(_)))
            .collect();
        for &(ci, a, b, _) in &links {
            let Some(&up) = fixed.get(&ci) else {
                continue;
            };
            let down = if up == a { b } else { a };
            for (end, seeds) in [(up, &mut mouths), (down, &mut springs)] {
                let v = NetVertex::of(&end);
                if joints.contains(&v) && !seeds.contains(&v) {
                    seeds.push(v);
                }
            }
        }
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

/// The outlet of a junction network (`net`, indices into `links`) with neither a mouth nor a spring: the open end of the chain
/// that leaves the lowest junction through its local edge 0, else the network's lowest open end.
fn open_outlet(
    net: &[usize],
    links: &[(usize, ChainEnd, ChainEnd, f32)],
    chains: &[Chain],
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
        let &(_, a, b, _) = net
            .iter()
            .map(|&j| &links[j])
            .find(|l| chains[l.0].0.contains(&arm))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileCatalog;
    use std::sync::OnceLock;

    fn catalog() -> &'static TileCatalog {
        static CATALOG: OnceLock<TileCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| TileCatalog::builtin().expect("builtin catalog"))
    }

    fn ax(q: i32, r: i32) -> Axial {
        Axial::new(q, r)
    }

    fn tiles(list: &[(&str, (i32, i32), u8)]) -> Vec<RiverTile<'static>> {
        let at: Vec<_> = list.iter().map(|&(id, c, k)| (id, c, k, 0)).collect();
        leveled(&at)
    }

    /// Tiles as (id, (q, r), rotation, level).
    fn leveled(list: &[(&str, (i32, i32), u8, u8)]) -> Vec<RiverTile<'static>> {
        list.iter()
            .map(|&(id, (q, r), rotation, level)| RiverTile {
                cell: ax(q, r),
                def: catalog().get(id).expect("tile in catalog"),
                rotation,
                level,
            })
            .collect()
    }

    fn len(id: &str) -> f32 {
        let def = catalog().get(id).expect("tile in catalog");
        river_segment_ends(def, 0)[0].2
    }

    #[test]
    fn a_spring_chain_flows_away_from_its_spring() {
        // river_spring at (0,0) rot 0, river_straight at (1,0) rot 0 (edges 0 and 3 face each other)
        let r = orient(&tiles(&[
            ("river_spring", (0, 0), 0),
            ("river_straight", (1, 0), 0),
        ]))
        .expect("flat");
        assert_eq!(r.segs[&(ax(0, 0), 0)].up, RiverEnd::Spring);
        assert!(r.segs[&(ax(0, 0), 0)].reversed);
        assert_eq!(r.segs[&(ax(1, 0), 0)].down, RiverEnd::Edge(0));
        assert!(r.falls.is_empty());
    }

    #[test]
    fn an_open_chain_starts_at_its_lowest_open_end() {
        let r = orient(&tiles(&[
            ("river_straight", (1, 0), 0),
            ("river_straight", (0, 0), 0),
        ]))
        .expect("flat");
        let first = &r.segs[&(ax(0, 0), 0)];
        assert_eq!(
            (first.up, first.down),
            (RiverEnd::Edge(3), RiverEnd::Edge(0))
        );
        assert!(first.reversed);
        let second = &r.segs[&(ax(1, 0), 0)];
        assert_eq!(
            (second.up, second.down),
            (RiverEnd::Edge(3), RiverEnd::Edge(0))
        );
        assert!(r.falls.is_empty());
    }

    #[test]
    fn a_mouth_chain_flows_into_the_mouth() {
        // river_mouth's River edge is local 3, facing the straight's edge 0.
        let r = orient(&tiles(&[
            ("river_straight", (0, 0), 0),
            ("river_mouth", (1, 0), 0),
        ]))
        .expect("flat");
        assert_eq!(r.segs[&(ax(0, 0), 0)].up, RiverEnd::Edge(3));
        let mouth = &r.segs[&(ax(1, 0), 0)];
        assert_eq!((mouth.up, mouth.down), (RiverEnd::Edge(3), RiverEnd::Mouth));
        assert!(!mouth.reversed);
    }

    #[test]
    fn a_lone_fork_leaves_through_local_edge_zero() {
        for k in 0..6 {
            let r = orient(&tiles(&[("river_fork", (0, 0), k)])).expect("flat");
            assert_eq!(r.segs.len(), 3);
            for seg in r.segs.values() {
                if seg.up == RiverEnd::Junction {
                    assert_eq!(seg.down, RiverEnd::Edge(k), "rotation {k}");
                } else {
                    assert_eq!(seg.down, RiverEnd::Junction, "rotation {k}");
                    assert_ne!(seg.up, RiverEnd::Edge(k), "rotation {k}");
                }
            }
            let outflows = r
                .segs
                .values()
                .filter(|s| s.up == RiverEnd::Junction)
                .count();
            assert_eq!(outflows, 1, "rotation {k}");
        }
    }

    #[test]
    fn segment_lengths_match_the_lane_geometry() {
        // river_straight: 2 * apothem = 27.7128; river_bend: the 24 m arc through 60 deg = 25.1327
        assert!((len("river_straight") - 27.712_8).abs() < 1e-3);
        assert!((len("river_bend") - 25.132_7).abs() < 1e-3);
    }

    /// The rotation of `id` whose river segments end on both world edges `edges`.
    fn rot_for(id: &str, edges: [u8; 2]) -> u8 {
        let def = catalog().get(id).expect("tile in catalog");
        (0..6)
            .find(|&k| {
                let ends: Vec<RiverEnd> = river_segment_ends(def, k)
                    .into_iter()
                    .flat_map(|(a, b, _)| [a, b])
                    .collect();
                edges.iter().all(|&e| ends.contains(&RiverEnd::Edge(e)))
            })
            .unwrap_or_else(|| panic!("{id} has no rotation joining {edges:?}"))
    }

    fn fall(upper: (i32, i32), edge: u8, lower: (i32, i32), drop: u8) -> Waterfall {
        Waterfall {
            upper: ax(upper.0, upper.1),
            edge,
            lower: ax(lower.0, lower.1),
            drop,
        }
    }

    #[test]
    fn a_fall_fixes_its_chain_downhill() {
        // The plain rule would start at (0,0)'s open edge 3; the fall from (1,0) runs the other way.
        let r = orient(&leveled(&[
            ("river_straight", (0, 0), 0, 0),
            ("river_straight", (1, 0), 0, 1),
        ]))
        .expect("legal");
        assert_eq!(r.falls, vec![fall((1, 0), 3, (0, 0), 1)]);
        let upper = &r.segs[&(ax(1, 0), 0)];
        assert_eq!(
            (upper.up, upper.down),
            (RiverEnd::Edge(0), RiverEnd::Edge(3))
        );
        let lower = &r.segs[&(ax(0, 0), 0)];
        assert_eq!(
            (lower.up, lower.down),
            (RiverEnd::Edge(0), RiverEnd::Edge(3))
        );
    }

    #[test]
    fn falls_facing_both_ways_are_uphill() {
        let r = orient(&leveled(&[
            ("river_straight", (0, 0), 0, 0),
            ("river_straight", (1, 0), 0, 1),
            ("river_straight", (2, 0), 0, 0),
        ]));
        assert_eq!(
            r,
            Err(Uphill {
                cell: ax(1, 0),
                edge: 0
            })
        );
    }

    #[test]
    fn a_fall_onto_a_spring_is_uphill() {
        // river_spring at rot 0 opens on edge 0, facing the straight at (1,0).
        let below = orient(&leveled(&[
            ("river_spring", (0, 0), 0, 0),
            ("river_straight", (1, 0), 0, 1),
        ]));
        assert_eq!(
            below,
            Err(Uphill {
                cell: ax(1, 0),
                edge: 3
            })
        );
        // A spring above the fall feeds it.
        let above = orient(&leveled(&[
            ("river_spring", (0, 0), 0, 1),
            ("river_straight", (1, 0), 0, 0),
        ]))
        .expect("a spring may feed a fall");
        assert_eq!(above.falls, vec![fall((0, 0), 0, (1, 0), 1)]);
        assert_eq!(above.segs[&(ax(0, 0), 0)].up, RiverEnd::Spring);
    }

    #[test]
    fn a_fall_onto_a_mouth_is_legal_and_a_mouth_never_feeds_a_fall() {
        let onto = orient(&leveled(&[
            ("river_straight", (0, 0), 0, 2),
            ("river_mouth", (1, 0), 0, 0),
        ]))
        .expect("a fall may run on to the sea");
        assert_eq!(onto.falls, vec![fall((0, 0), 0, (1, 0), 2)]);
        assert_eq!(onto.segs[&(ax(1, 0), 0)].down, RiverEnd::Mouth);
        let out_of = orient(&leveled(&[
            ("river_straight", (0, 0), 0, 0),
            ("river_mouth", (1, 0), 0, 1),
        ]));
        assert_eq!(
            out_of,
            Err(Uphill {
                cell: ax(1, 0),
                edge: 3
            })
        );
    }

    #[test]
    fn a_loop_through_a_fall_is_uphill() {
        // Six bends round (0,0), each joining its two ring neighbours; one stands a level up.
        let centre = ax(0, 0);
        let ring: Vec<Axial> = (0..6).map(|d| centre.neighbor(d)).collect();
        let mut list = vec![];
        for (d, &c) in ring.iter().enumerate() {
            let edges: Vec<u8> = (0..6).filter(|&e| ring.contains(&c.neighbor(e))).collect();
            let id = ["river_bend", "river_bend_60"]
                .into_iter()
                .find(|id| {
                    let def = catalog().get(id).expect("tile in catalog");
                    (0..6).any(|k| {
                        let ends: Vec<RiverEnd> = river_segment_ends(def, k)
                            .into_iter()
                            .flat_map(|(a, b, _)| [a, b])
                            .collect();
                        edges.iter().all(|&e| ends.contains(&RiverEnd::Edge(e)))
                    })
                })
                .expect("a bend joins two ring neighbours");
            let k = rot_for(id, [edges[0], edges[1]]);
            list.push((id, (c.q, c.r), k, u8::from(d == 0)));
        }
        let flat: Vec<_> = list.iter().map(|&(id, c, k, _)| (id, c, k, 0)).collect();
        let ring_flat = orient(&leveled(&flat)).expect("a flat ring is legal");
        assert_eq!(ring_flat.segs.len(), 6);
        let err = orient(&leveled(&list)).expect_err("a loop cannot fall");
        assert_eq!(err.cell, ring[0]);
    }

    #[test]
    fn falls_name_the_exact_seam_on_bends_and_forks() {
        // river_bend joins local edges 0 and 2: at rotation 1, world edges 1 and 3.
        let n = ax(0, 0).neighbor(1);
        let bend = orient(&leveled(&[
            ("river_bend", (0, 0), 1, 2),
            ("river_straight", (n.q, n.r), 1, 0),
        ]))
        .expect("legal");
        assert_eq!(bend.falls, vec![fall((0, 0), 1, (n.q, n.r), 2)]);
        // river_fork's arms leave local edges 0, 2 and 4; the straight above meets edge 2.
        let n = ax(0, 0).neighbor(2);
        let fork = orient(&leveled(&[
            ("river_fork", (0, 0), 0, 0),
            ("river_straight", (n.q, n.r), 2, 3),
        ]))
        .expect("legal");
        assert_eq!(fork.falls, vec![fall((n.q, n.r), 5, (0, 0), 3)]);
    }

    #[test]
    fn a_bridge_tile_may_be_lip_or_pool() {
        let def = catalog().get("river_road_bridge").expect("tile in catalog");
        let e = river_segment_ends(def, 0)[0]
            .0
            .edge()
            .expect("the bridge's river runs edge to edge");
        let n = ax(0, 0).neighbor(e);
        let k = rot_for("river_straight", [opposite_edge(e), e]);
        for (bridge, straight) in [(1, 0), (0, 1)] {
            let r = orient(&leveled(&[
                ("river_road_bridge", (0, 0), 0, bridge),
                ("river_straight", (n.q, n.r), k, straight),
            ]))
            .expect("legal");
            let want = if bridge > straight {
                fall((0, 0), e, (n.q, n.r), 1)
            } else {
                fall((n.q, n.r), opposite_edge(e), (0, 0), 1)
            };
            assert_eq!(r.falls, vec![want]);
        }
    }

    #[test]
    fn a_flat_chain_without_source_reverses_when_a_waterfall_fixes_it() {
        let two = [
            ("river_straight", (0, 0), 0, 1),
            ("river_straight", (1, 0), 0, 1),
        ];
        let before = orient(&leveled(&two)).expect("flat");
        assert_eq!(before.segs[&(ax(0, 0), 0)].up, RiverEnd::Edge(3));
        let mut three = two.to_vec();
        three.push(("river_straight", (2, 0), 0, 2));
        let after = orient(&leveled(&three)).expect("legal");
        for q in 0..3 {
            let s = &after.segs[&(ax(q, 0), 0)];
            assert_eq!(
                (s.up, s.down),
                (RiverEnd::Edge(0), RiverEnd::Edge(3)),
                "({q}, 0)"
            );
        }
        assert_eq!(after.falls, vec![fall((2, 0), 3, (1, 0), 1)]);
    }

    /// The world edges of a `river_fork` arm at rotation 0, outlet (local edge 0) first.
    fn fork_arms() -> Vec<u8> {
        let def = catalog().get("river_fork").expect("tile in catalog");
        let mut arms: Vec<u8> = river_segment_ends(def, 0)
            .into_iter()
            .flat_map(|(a, b, _)| [a, b])
            .filter_map(RiverEnd::edge)
            .collect();
        arms.sort();
        assert_eq!(arms[0], 0);
        arms
    }

    #[test]
    fn a_fall_into_a_fork_flows_out_through_its_other_arms() {
        // The straight above feeds the fork's default outlet arm, which turns that arm inward.
        let r = orient(&leveled(&[
            ("river_fork", (0, 0), 0, 0),
            ("river_straight", (1, 0), 0, 1),
        ]))
        .expect("legal");
        assert_eq!(r.falls, vec![fall((1, 0), 3, (0, 0), 1)]);
        for seg in r
            .segs
            .range((ax(0, 0), 0)..=(ax(0, 0), usize::MAX))
            .map(|(_, s)| s)
        {
            if seg.up == RiverEnd::Edge(0) || seg.down == RiverEnd::Edge(0) {
                assert_eq!((seg.up, seg.down), (RiverEnd::Edge(0), RiverEnd::Junction));
            } else {
                assert_eq!(seg.up, RiverEnd::Junction, "{seg:?}");
            }
        }
    }

    #[test]
    fn a_fall_out_of_a_fork_is_its_outlet() {
        let e = fork_arms()[1];
        let n = ax(0, 0).neighbor(e);
        let k = rot_for("river_straight", [e, opposite_edge(e)]);
        let r = orient(&leveled(&[
            ("river_fork", (0, 0), 0, 1),
            ("river_straight", (n.q, n.r), k, 0),
        ]))
        .expect("legal");
        assert_eq!(r.falls, vec![fall((0, 0), e, (n.q, n.r), 1)]);
        for seg in r
            .segs
            .range((ax(0, 0), 0)..=(ax(0, 0), usize::MAX))
            .map(|(_, s)| s)
        {
            if seg.up == RiverEnd::Edge(e) || seg.down == RiverEnd::Edge(e) {
                assert_eq!((seg.up, seg.down), (RiverEnd::Junction, RiverEnd::Edge(e)));
            } else {
                assert_eq!(seg.down, RiverEnd::Junction, "{seg:?}");
            }
        }
    }

    /// Whether `to` is reachable from `from` along the directed `arcs` (trivially, from itself).
    fn reaches(arcs: &[(NetVertex, NetVertex)], from: NetVertex, to: NetVertex) -> bool {
        let mut seen = BTreeSet::from([from]);
        let mut stack = vec![from];
        while let Some(v) = stack.pop() {
            if v == to {
                return true;
            }
            for &(x, y) in arcs {
                if x == v && seen.insert(y) {
                    stack.push(y);
                }
            }
        }
        false
    }

    /// The cells of junctions with an inflow but no outflow.
    fn sinks(r: &Rivers) -> Vec<Axial> {
        let mut flow: BTreeMap<Axial, (usize, usize)> = BTreeMap::new();
        for (&(cell, _), s) in &r.segs {
            let f = flow.entry(cell).or_default();
            f.0 += usize::from(s.down == RiverEnd::Junction);
            f.1 += usize::from(s.up == RiverEnd::Junction);
        }
        flow.into_iter()
            .filter(|&(_, (i, o))| i > 0 && o == 0)
            .map(|(c, _)| c)
            .collect()
    }

    /// A `river_fork` rotation with an arm on world edge `e`.
    fn fork_rot_with(e: u8) -> u8 {
        let def = catalog().get("river_fork").expect("tile in catalog");
        (0..6)
            .find(|&k| {
                river_segment_ends(def, k)
                    .into_iter()
                    .any(|(a, b, _)| a == RiverEnd::Edge(e) || b == RiverEnd::Edge(e))
            })
            .expect("a fork rotation has an arm there")
    }

    #[test]
    fn a_fall_between_two_forks_leaves_an_outlet() {
        // Fork A (level 1) drains to a mouth on its outlet arm; its next arm falls straight onto
        // fork B (level 0), whose other two arms are open.
        let e = fork_arms()[1];
        let b = ax(0, 0).neighbor(e);
        let r = orient(&leveled(&[
            ("river_fork", (0, 0), 0, 1),
            ("river_mouth", (1, 0), 0, 1),
            ("river_fork", (b.q, b.r), fork_rot_with(opposite_edge(e)), 0),
        ]))
        .expect("legal");
        assert_eq!(r.falls, vec![fall((0, 0), e, (b.q, b.r), 1)]);
        assert_eq!(sinks(&r), vec![], "{:#?}", r.segs);
    }

    /// splitmix64: a seeded generator for the oracle's worlds.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// A seeded cluster of 2 to `most` tiles drawn from `ids`, each beside an earlier one, at levels
    /// 0 to 3.
    fn random_world(seed: u64, ids: &[&str], most: usize) -> Vec<RiverTile<'static>> {
        let mut rng = Rng(seed);
        let count = 2 + rng.below(most as u64 - 1) as usize;
        let mut cells = vec![ax(0, 0)];
        while cells.len() < count {
            let from = cells[rng.below(cells.len() as u64) as usize];
            let c = from.neighbor(rng.below(6) as u8);
            if !cells.contains(&c) {
                cells.push(c);
            }
        }
        cells
            .into_iter()
            .map(|cell| RiverTile {
                cell,
                def: catalog()
                    .get(ids[rng.below(ids.len() as u64) as usize])
                    .expect("tile in catalog"),
                rotation: rng.below(6) as u8,
                level: rng.below(4) as u8,
            })
            .collect()
    }

    struct Net {
        segs: BTreeMap<Key, Segment>,
        chains: Vec<Chain>,
        falls: BTreeMap<(Axial, u8), Waterfall>,
    }

    fn net(tiles: &[RiverTile]) -> Net {
        let mut segs = BTreeMap::new();
        let mut placed = HashMap::new();
        for t in tiles {
            for (i, s) in river_segment_ends(t.def, t.rotation)
                .into_iter()
                .enumerate()
            {
                segs.insert((t.cell, i), s);
            }
            placed.insert(t.cell, *t);
        }
        let chains = chains(&segs);
        let falls = waterfalls(&segs, &placed);
        Net {
            segs,
            chains,
            falls,
        }
    }

    /// Chain `ci` walked from its first end (or, for a loop, its lowest segment's lower end) when
    /// `flip` is false, else from the other.
    fn walk_chain(n: &Net, ci: usize, flip: bool) -> Vec<(Key, RiverSeg)> {
        let (chain, ends) = &n.chains[ci];
        match ends[..] {
            [a, b] => {
                let (cell, e, i) = if flip { b } else { a };
                walk(&n.segs, (cell, i), e)
            }
            _ => {
                let k = *chain.iter().min().expect("non-empty chain");
                let (a, b, _) = n.segs[&k];
                walk(&n.segs, k, if flip { a.max(b) } else { a.min(b) })
            }
        }
    }

    /// Spec §1 rules 1 to 3 on one orientation of every chain (`walks[ci]` downstream).
    fn legal(n: &Net, walks: &[Vec<(Key, RiverSeg)>]) -> bool {
        let fall_chains: BTreeSet<usize> = n
            .falls
            .keys()
            .map(|&(cell, e)| {
                walks
                    .iter()
                    .position(|w| {
                        w.iter().any(|((c, _), s)| {
                            *c == cell && (s.up == RiverEnd::Edge(e) || s.down == RiverEnd::Edge(e))
                        })
                    })
                    .expect("a fall lies on a chain")
            })
            .collect();
        // 1: every fall is crossed from high to low.
        let descends = n.falls.keys().all(|&(cell, e)| {
            walks
                .iter()
                .flatten()
                .any(|((c, _), s)| *c == cell && s.down == RiverEnd::Edge(e))
        });
        if !descends {
            return false;
        }
        let arcs: Vec<(NetVertex, NetVertex)> = n
            .chains
            .iter()
            .zip(walks)
            .filter(|((_, ends), _)| ends.len() == 2)
            .map(|(_, w)| {
                let ((c0, i0), s0) = &w[0];
                let ((c1, i1), s1) = w.last().expect("non-empty walk");
                (
                    NetVertex::of(&(*c0, s0.up, *i0)),
                    NetVertex::of(&(*c1, s1.down, *i1)),
                )
            })
            .collect();
        fall_chains.into_iter().all(|ci| {
            let w = &walks[ci];
            if n.chains[ci].1.is_empty() {
                return false; // 2: a loop through a fall
            }
            let ((c0, i0), s0) = &w[0];
            let ((c1, i1), s1) = w.last().expect("non-empty walk");
            // 3: a fixed chain never flows into a spring or out of a mouth at its ends.
            s0.up != RiverEnd::Mouth
                && s1.down != RiverEnd::Spring
                // 2: no directed cycle through the fall.
                && !reaches(&arcs, NetVertex::of(&(*c1, s1.down, *i1)), NetVertex::of(&(*c0, s0.up, *i0)))
        })
    }

    /// The junction cells a chain crossing a waterfall reaches.
    fn fall_junctions(n: &Net) -> BTreeSet<Axial> {
        n.chains
            .iter()
            .filter(|(chain, _)| {
                n.falls.values().any(|f| {
                    chain.iter().any(|&(c, _)| c == f.upper)
                        && chain.iter().any(|&(c, _)| c == f.lower)
                })
            })
            .flat_map(|(_, ends)| {
                ends.iter()
                    .filter(|e| e.1 == RiverEnd::Junction)
                    .map(|e| e.0)
            })
            .collect()
    }

    /// Brute force: whether any orientation of the chains that bear on the rules is legal.
    fn oracle_exists_legal_orientation(n: &Net) -> bool {
        let fall_chain = |ci: usize| {
            n.chains[ci]
                .0
                .iter()
                .any(|&(cell, _)| n.falls.values().any(|f| f.upper == cell || f.lower == cell))
        };
        let free: Vec<usize> = (0..n.chains.len())
            .filter(|&ci| {
                fall_chain(ci) || n.chains[ci].1.iter().any(|e| e.1 == RiverEnd::Junction)
            })
            .collect();
        assert!(free.len() <= 20, "{} chains to enumerate", free.len());
        (0..1u32 << free.len()).any(|mask| {
            let walks: Vec<_> = (0..n.chains.len())
                .map(|ci| {
                    let flip = free
                        .iter()
                        .position(|&f| f == ci)
                        .is_some_and(|b| mask >> b & 1 == 1);
                    walk_chain(n, ci, flip)
                })
                .collect();
            legal(n, &walks)
        })
    }

    #[test]
    fn waterfall_legality_matches_brute_force_oracle() {
        let (mut with_falls, mut uphill) = (0, 0);
        const MIXED: [&str; 6] = [
            "river_straight",
            "river_bend",
            "river_bend_60",
            "river_fork",
            "river_spring",
            "river_mouth",
        ];
        // Fork-heavy worlds put falls between junctions.
        const FORKS: [&str; 4] = ["river_fork", "river_fork", "river_straight", "river_mouth"];
        for seed in 0..3072u64 {
            let tiles = if seed < 2048 {
                random_world(seed, &MIXED, 8)
            } else {
                random_world(seed, &FORKS, 5)
            };
            let n = net(&tiles);
            let got = orient(&tiles);
            assert_eq!(
                got.is_ok(),
                oracle_exists_legal_orientation(&n),
                "seed {seed}"
            );
            with_falls += usize::from(!n.falls.is_empty());
            match got {
                Ok(r) => {
                    assert_eq!(r.falls, n.falls.values().copied().collect::<Vec<_>>());
                    let walks: Vec<_> = (0..n.chains.len())
                        .map(|ci| {
                            let flip = walk_chain(&n, ci, false)
                                .iter()
                                .any(|(k, s)| r.segs[k] != *s);
                            let w = walk_chain(&n, ci, flip);
                            assert!(w.iter().all(|(k, s)| r.segs[k] == *s), "seed {seed}");
                            w
                        })
                        .collect();
                    assert!(
                        legal(&n, &walks),
                        "seed {seed}: orient chose an illegal flow"
                    );
                    let fed = fall_junctions(&n);
                    let stuck: Vec<Axial> =
                        sinks(&r).into_iter().filter(|c| fed.contains(c)).collect();
                    assert!(
                        stuck.is_empty(),
                        "seed {seed}: sinks {stuck:?} beside a fall"
                    );
                    if n.falls.is_empty() {
                        let flat: Vec<RiverTile> =
                            tiles.iter().map(|t| RiverTile { level: 0, ..*t }).collect();
                        assert_eq!(Some(r), orient(&flat).ok(), "seed {seed}");
                    }
                }
                Err(u) => {
                    uphill += 1;
                    assert!(
                        n.falls.contains_key(&(u.cell, u.edge)),
                        "seed {seed}: {u:?}"
                    );
                }
            }
        }
        assert!(
            with_falls > 200 && uphill > 50,
            "{with_falls} with falls, {uphill} uphill"
        );
    }
}
