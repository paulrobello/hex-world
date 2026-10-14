//! One stitched graph per network kind (PRD §7.1), updated incrementally on place and bulldoze.
//!
//! A segment ending on a lane edge ends at a seam node keyed by the seam itself, so the tiles on
//! both sides of a seam share that node and are stitched by key, never by float position. A seam
//! node with only one placed side is a half-seam of degree 1 facing an empty cell.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use hex_core::{Axial, Destination, Endpoint, NetworkKind, TileDef, opposite_edge, world_edge};

use crate::geom::{self, Vec2};
use crate::route::{Route, find_route};

/// Cached routes beyond this count are dropped wholesale; the cache only saves A* work.
const ROUTE_CACHE_CAP: usize = 4096;

/// Graph node id. Ids only ever increase and are never reused after a node is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// A link is segment `segment` (its index in [`TileDef::networks`]) of the tile at `cell`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkId {
    pub cell: Axial,
    pub segment: u16,
}

/// What a node is, independent of float positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeKey {
    /// The lane crossing world edge `edge` of `cell`, in the canonical form of [`NodeKey::seam`].
    Seam { cell: Axial, edge: u8 },
    /// A node declared in [`TileDef::nodes`] of the tile at `cell`.
    Local { cell: Axial, id: String },
}

impl NodeKey {
    /// The key of the seam across world edge `edge` of `cell`. Both tiles at a seam get the same
    /// key: the smaller of `(cell, edge)` and `(neighbor, opposite edge)`.
    pub fn seam(cell: Axial, edge: u8) -> Self {
        let edge = edge % 6;
        let other = (cell.neighbor(edge), opposite_edge(edge));
        let (cell, edge) = (cell, edge).min(other);
        NodeKey::Seam { cell, edge }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    key: NodeKey,
    pos: Vec2,
    destination: Option<Destination>,
    links: BTreeSet<LinkId>,
}

impl Node {
    pub fn key(&self) -> &NodeKey {
        &self.key
    }

    /// World XZ position. A seam node keeps the position computed by the tile that created it.
    pub fn pos(&self) -> Vec2 {
        self.pos
    }

    pub fn destination(&self) -> Option<Destination> {
        self.destination
    }

    /// Links touching this node, sorted.
    pub fn links(&self) -> &BTreeSet<LinkId> {
        &self.links
    }
}

/// An undirected graph edge: one tile segment sampled into a world-space centerline from `a` to `b`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    a: NodeId,
    b: NodeId,
    points: Vec<Vec2>,
    length: f32,
}

impl Link {
    pub fn ends(&self) -> (NodeId, NodeId) {
        (self.a, self.b)
    }

    /// The far end of the link from `node`.
    pub fn other(&self, node: NodeId) -> NodeId {
        if node == self.a { self.b } else { self.a }
    }

    /// Centerline from `a` to `b` in world XZ meters.
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Centerline length in meters; the routing cost.
    pub fn length(&self) -> f32 {
        self.length
    }
}

/// Connected components of a graph and the destinations in each, sorted by node id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Components {
    label: HashMap<NodeId, usize>,
    destinations: Vec<Vec<NodeId>>,
    connected: Vec<NodeId>,
}

impl Components {
    fn compute(net: &Network) -> Self {
        let mut label: HashMap<NodeId, usize> = HashMap::with_capacity(net.nodes.len());
        let mut destinations: Vec<Vec<NodeId>> = vec![];
        for &start in net.nodes.keys() {
            if label.contains_key(&start) {
                continue;
            }
            let comp = destinations.len();
            let mut found = vec![];
            let mut stack = vec![start];
            label.insert(start, comp);
            while let Some(id) = stack.pop() {
                let node = &net.nodes[&id];
                if node.destination.is_some() {
                    found.push(id);
                }
                for link in &node.links {
                    let next = net.links[link].other(id);
                    if let Entry::Vacant(slot) = label.entry(next) {
                        slot.insert(comp);
                        stack.push(next);
                    }
                }
            }
            found.sort();
            destinations.push(found);
        }
        let mut connected: Vec<NodeId> = destinations
            .iter()
            .filter(|d| d.len() >= 2)
            .flatten()
            .copied()
            .collect();
        connected.sort();
        Self {
            label,
            destinations,
            connected,
        }
    }

    pub fn count(&self) -> usize {
        self.destinations.len()
    }

    /// The component index of `node`, if it is in the graph.
    pub fn of(&self, node: NodeId) -> Option<usize> {
        self.label.get(&node).copied()
    }

    pub fn connected(&self, a: NodeId, b: NodeId) -> bool {
        matches!((self.of(a), self.of(b)), (Some(x), Some(y)) if x == y)
    }

    /// Destinations in component `comp`, sorted.
    pub fn destinations(&self, comp: usize) -> &[NodeId] {
        self.destinations.get(comp).map_or(&[], Vec::as_slice)
    }

    /// Destinations that share their component with at least one other destination, sorted: the
    /// destinations a car can start from.
    pub fn connected_destinations(&self) -> &[NodeId] {
        &self.connected
    }
}

/// The stitched graph of one network kind.
#[derive(Clone, Debug)]
pub struct Network {
    kind: NetworkKind,
    version: u64,
    next_node: u32,
    ids: HashMap<NodeKey, NodeId>,
    nodes: BTreeMap<NodeId, Node>,
    links: BTreeMap<LinkId, Link>,
    routes: HashMap<(NodeId, NodeId), Option<Arc<Route>>>,
    components: Option<Components>,
}

impl Network {
    pub fn new(kind: NetworkKind) -> Self {
        Self {
            kind,
            version: 0,
            next_node: 0,
            ids: HashMap::new(),
            nodes: BTreeMap::new(),
            links: BTreeMap::new(),
            routes: HashMap::new(),
            components: None,
        }
    }

    pub fn kind(&self) -> NetworkKind {
        self.kind
    }

    /// Bumped by every edit that changes the graph.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Adds the segments of `def` placed at `cell` with `rotation` that belong to this network,
    /// stitching lane ends to the seam nodes of placed neighbors. A tile already at `cell` is
    /// removed first. Returns whether the graph changed.
    pub fn insert_tile(&mut self, cell: Axial, def: &TileDef, rotation: u8) -> bool {
        let mut changed = self.remove_tile(cell);
        let center = geom::cell_center(cell);
        for (i, seg) in def.networks.iter().enumerate() {
            if seg.kind != self.kind {
                continue;
            }
            let (Some(from), Some(to)) = (
                endpoint(cell, def, rotation, &seg.from),
                endpoint(cell, def, rotation, &seg.to),
            ) else {
                continue;
            };
            let interior: Vec<Vec2> = seg
                .points
                .iter()
                .map(|&p| geom::add(center, geom::rotate(p, rotation)))
                .collect();
            let points = geom::segment_shape(from.pos, &interior, to.pos);
            let id = LinkId {
                cell,
                segment: i as u16,
            };
            let a = self.node_for(from, id);
            let b = self.node_for(to, id);
            self.links.insert(
                id,
                Link {
                    a,
                    b,
                    length: geom::polyline_length(&points),
                    points,
                },
            );
            changed = true;
        }
        if changed {
            self.touch();
        }
        changed
    }

    /// Removes every link of the tile at `cell` and any node left without links. Returns whether
    /// the graph changed.
    pub fn remove_tile(&mut self, cell: Axial) -> bool {
        let ids: Vec<LinkId> = self.links_of(cell).map(|(id, _)| id).collect();
        for id in &ids {
            let link = self.links.remove(id).expect("listed above");
            for end in [link.a, link.b] {
                let Some(node) = self.nodes.get_mut(&end) else {
                    continue;
                };
                node.links.remove(id);
                if node.links.is_empty() {
                    let node = self.nodes.remove(&end).expect("present above");
                    self.ids.remove(&node.key);
                }
            }
        }
        if !ids.is_empty() {
            self.touch();
        }
        !ids.is_empty()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Nodes in id order.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().map(|(id, n)| (*id, n))
    }

    pub fn node_id(&self, key: &NodeKey) -> Option<NodeId> {
        self.ids.get(key).copied()
    }

    /// The seam node on world edge `edge` of `cell`, if a placed segment ends there.
    pub fn seam_node(&self, cell: Axial, edge: u8) -> Option<NodeId> {
        self.node_id(&NodeKey::seam(cell, edge))
    }

    /// The node `id` declared by the tile at `cell`, if a placed segment uses it.
    pub fn local_node(&self, cell: Axial, id: &str) -> Option<NodeId> {
        self.node_id(&NodeKey::Local {
            cell,
            id: id.to_owned(),
        })
    }

    pub fn link(&self, id: LinkId) -> Option<&Link> {
        self.links.get(&id)
    }

    /// Links in id order.
    pub fn links(&self) -> impl Iterator<Item = (LinkId, &Link)> {
        self.links.iter().map(|(id, l)| (*id, l))
    }

    /// Links of the tile at `cell`, in segment order.
    pub fn links_of(&self, cell: Axial) -> impl Iterator<Item = (LinkId, &Link)> {
        let lo = LinkId { cell, segment: 0 };
        let hi = LinkId {
            cell,
            segment: u16::MAX,
        };
        self.links.range(lo..=hi).map(|(id, l)| (*id, l))
    }

    /// Destination nodes, in id order.
    pub fn destinations(&self) -> impl Iterator<Item = NodeId> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.destination.is_some())
            .map(|(id, _)| *id)
    }

    /// Connected components, recomputed lazily after the graph changes.
    pub fn components(&mut self) -> &Components {
        if self.components.is_none() {
            self.components = Some(Components::compute(self));
        }
        self.components.as_ref().expect("filled above")
    }

    /// The shortest route from `from` to `to`, or `None` when they are not connected. Results are
    /// cached until the graph next changes.
    pub fn route(&mut self, from: NodeId, to: NodeId) -> Option<Arc<Route>> {
        if let Some(hit) = self.routes.get(&(from, to)) {
            return hit.clone();
        }
        let found = if self.components().connected(from, to) {
            find_route(self, from, to).map(Arc::new)
        } else {
            None
        };
        if self.routes.len() >= ROUTE_CACHE_CAP {
            self.routes.clear();
        }
        self.routes.insert((from, to), found.clone());
        found
    }

    /// Number of cached routes, for cache tests.
    pub fn cached_routes(&self) -> usize {
        self.routes.len()
    }

    /// Records a graph change: bumps the version and drops every derived cache.
    fn touch(&mut self) {
        self.version += 1;
        self.routes.clear();
        self.components = None;
    }

    /// The node for `end`, created if no placed segment uses its key yet, with `link` attached.
    fn node_for(&mut self, end: End, link: LinkId) -> NodeId {
        let id = match self.ids.get(&end.key) {
            Some(&id) => id,
            None => {
                let id = NodeId(self.next_node);
                self.next_node += 1;
                self.ids.insert(end.key.clone(), id);
                self.nodes.insert(
                    id,
                    Node {
                        key: end.key,
                        pos: end.pos,
                        destination: end.destination,
                        links: BTreeSet::new(),
                    },
                );
                id
            }
        };
        self.nodes
            .get_mut(&id)
            .expect("inserted above")
            .links
            .insert(link);
        id
    }
}

/// A segment endpoint resolved into world space.
struct End {
    key: NodeKey,
    pos: Vec2,
    destination: Option<Destination>,
}

/// `None` when the endpoint names a node the tile does not declare (never true for a validated
/// catalog).
fn endpoint(cell: Axial, def: &TileDef, rotation: u8, end: &Endpoint) -> Option<End> {
    match end {
        Endpoint::Edge(local) => {
            let edge = world_edge(*local, rotation);
            let offset = def
                .edges
                .get(*local as usize)?
                .lane
                .map_or(0.0, |l| l.offset);
            Some(End {
                key: NodeKey::seam(cell, edge),
                pos: geom::lane_point(cell, edge, offset),
                destination: None,
            })
        }
        Endpoint::Node(id) => {
            let node = def.nodes.iter().find(|n| &n.id == id)?;
            Some(End {
                key: NodeKey::Local {
                    cell,
                    id: id.clone(),
                },
                pos: geom::add(geom::cell_center(cell), geom::rotate(node.at, rotation)),
                destination: node.destination,
            })
        }
    }
}
