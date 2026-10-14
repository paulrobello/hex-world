//! A* over a [`Network`] (PRD §7.2). Edge cost is centerline length in meters and the heuristic is
//! straight-line distance, which never overestimates because a link is never shorter than its chord.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::geom::{self, Vec2};
use crate::network::{LinkId, Network, NodeId};

/// A path through the graph: `links[i]` joins `nodes[i]` to `nodes[i + 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub nodes: Vec<NodeId>,
    pub links: Vec<LinkId>,
    /// Sum of the link lengths, in meters.
    pub length: f32,
}

impl Route {
    /// The route centerline in travel order, each link oriented from `nodes[i]` to `nodes[i + 1]`.
    /// Shared node points appear once.
    pub fn centerline(&self, net: &Network) -> Vec<Vec2> {
        let mut out: Vec<Vec2> = vec![];
        for (i, id) in self.links.iter().enumerate() {
            let Some(link) = net.link(*id) else {
                continue;
            };
            let forward = link.ends().0 == self.nodes[i];
            let points = link.points();
            let skip = usize::from(!out.is_empty());
            if forward {
                out.extend(points.iter().skip(skip));
            } else {
                out.extend(points.iter().rev().skip(skip));
            }
        }
        if out.is_empty()
            && let Some(node) = self.nodes.first().and_then(|n| net.node(*n))
        {
            out.push(node.pos());
        }
        out
    }
}

/// Open-set entry, ordered so the max-heap pops the lowest `f`, then the lowest node id.
struct Open {
    f: f32,
    node: NodeId,
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Open {}

/// The shortest route from `from` to `to` by A*, uncached. `None` when either node is missing or
/// they are not connected. Ties resolve by node id and link id, so the result is deterministic.
pub fn find_route(net: &Network, from: NodeId, to: NodeId) -> Option<Route> {
    let goal = net.node(to)?.pos();
    net.node(from)?;
    let h = |id: NodeId| net.node(id).map_or(0.0, |n| geom::dist(n.pos(), goal));

    let mut g: HashMap<NodeId, f32> = HashMap::from([(from, 0.0)]);
    let mut came: HashMap<NodeId, (NodeId, LinkId)> = HashMap::new();
    let mut closed: HashSet<NodeId> = HashSet::new();
    let mut open = BinaryHeap::from([Open {
        f: h(from),
        node: from,
    }]);

    while let Some(Open { node, .. }) = open.pop() {
        if !closed.insert(node) {
            continue;
        }
        if node == to {
            let mut nodes = vec![to];
            let mut links = vec![];
            let mut cur = to;
            while let Some(&(prev, link)) = came.get(&cur) {
                nodes.push(prev);
                links.push(link);
                cur = prev;
            }
            nodes.reverse();
            links.reverse();
            return Some(Route {
                nodes,
                links,
                length: g[&to],
            });
        }
        let base = g[&node];
        for &link_id in net.node(node)?.links() {
            let link = net.link(link_id)?;
            let next = link.other(node);
            if closed.contains(&next) {
                continue;
            }
            let cost = base + link.length();
            if g.get(&next).is_none_or(|&old| cost < old) {
                g.insert(next, cost);
                came.insert(next, (node, link_id));
                open.push(Open {
                    f: cost + h(next),
                    node: next,
                });
            }
        }
    }
    None
}
