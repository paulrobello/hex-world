//! Tunnels derived from portals (P4). A tunnel is never stored: it exists while two portals at the
//! same level face each other along a straight line whose cells between them hold `meadow` one
//! level higher. Those meadows are the tunnel's cover and are protected from removal.

use std::collections::BTreeSet;

use crate::axial::Axial;
use crate::catalog::TileCatalog;
use crate::tile::{TileDef, opposite_edge, world_edge};
use crate::world::{Orientation, World};

/// The only tile that may be tunnel cover.
pub const COVER_DEF: &str = "meadow";

/// A complete tunnel between two portals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tunnel {
    pub a: Axial,
    pub b: Axial,
    /// The world edge from `a` toward `b`, with `a < b` by `Axial` order.
    pub dir: u8,
    /// The cells between the portals, in order from `a`. May be empty.
    pub cover: Vec<Axial>,
    /// The level of both portals. Cover stands one level higher.
    pub level: u8,
}

/// What a portal ghost's tunnel would do if it were placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelPreview {
    /// The cells the tunnel would run under.
    pub cells: Vec<Axial>,
    /// Whether a facing portal at the same level closes the line.
    pub completes: bool,
}

/// The cover cells found walking from `from` across world edge `edge` while each cell holds
/// level-`level` meadow, and the first cell that is not such cover.
fn walk(world: &World, from: Axial, edge: u8, level: u8) -> (Vec<Axial>, Axial) {
    let mut cover = Vec::new();
    let mut at = from.neighbor(edge);
    while world
        .tile(at)
        .is_some_and(|t| t.def == COVER_DEF && t.level == level)
    {
        cover.push(at);
        at = at.neighbor(edge);
    }
    (cover, at)
}

/// The world edge of a portal's tunnel mouth, or `None` for a tile that is not a portal.
fn mouth(def: &TileDef, rotation: u8) -> Option<u8> {
    def.tunnel.as_ref().map(|t| world_edge(t.edge, rotation))
}

/// Whether the tile at `cell` is a portal at `level` whose mouth is world edge `edge`.
fn faces(world: &World, c: &TileCatalog, cell: Axial, level: u8, edge: u8) -> bool {
    world.tile(cell).is_some_and(|far| {
        far.level == level
            && c.get(&far.def)
                .and_then(|d| mouth(d, far.rotation))
                .is_some_and(|e| e == edge)
    })
}

/// Every complete tunnel, sorted by `a`.
pub fn tunnels(world: &World, c: &TileCatalog) -> Vec<Tunnel> {
    let mut out = Vec::new();
    for (&a, tile) in world.tiles() {
        let Some(edge) = c.get(&tile.def).and_then(|d| mouth(d, tile.rotation)) else {
            continue;
        };
        let (cover, end) = walk(world, a, edge, tile.level + 1);
        if end > a && faces(world, c, end, tile.level, opposite_edge(edge)) {
            out.push(Tunnel {
                a,
                b: end,
                dir: edge,
                cover,
                level: tile.level,
            });
        }
    }
    out
}

/// Portals that are in no complete tunnel, in cell order.
pub fn open_portals(world: &World, c: &TileCatalog) -> Vec<Axial> {
    let closed: BTreeSet<Axial> = tunnels(world, c).iter().flat_map(|t| [t.a, t.b]).collect();
    world
        .tiles()
        .iter()
        .filter(|(cell, t)| {
            c.get(&t.def).is_some_and(|d| d.tunnel.is_some()) && !closed.contains(*cell)
        })
        .map(|(cell, _)| *cell)
        .collect()
}

/// Whether `cell` is cover of a complete tunnel.
pub fn is_cover(world: &World, c: &TileCatalog, cell: Axial) -> bool {
    tunnels(world, c).iter().any(|t| t.cover.contains(&cell))
}

/// For a portal ghost of `def` at `cell` in orientation `o`: the cells its tunnel would run under
/// and whether it would complete. Empty and incomplete for a tile that is not a portal. The ghost
/// itself need not be in the world.
pub fn preview(
    world: &World,
    c: &TileCatalog,
    cell: Axial,
    def: &TileDef,
    o: Orientation,
) -> TunnelPreview {
    let Some(edge) = mouth(def, o.rotation) else {
        return TunnelPreview {
            cells: vec![],
            completes: false,
        };
    };
    let (cells, end) = walk(world, cell, edge, o.level + 1);
    let completes = faces(world, c, end, o.level, opposite_edge(edge));
    TunnelPreview { cells, completes }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::{BTreeMap, VecDeque};

    use super::*;
    use crate::world::{PlacedTile, cell_seed};

    pub(crate) fn catalog() -> TileCatalog {
        TileCatalog::builtin().expect("builtin catalog")
    }

    fn tile(def: &str, rotation: u8, level: u8, cell: Axial) -> PlacedTile {
        PlacedTile {
            def: def.to_owned(),
            rotation,
            level,
            seed: cell_seed(1, cell),
        }
    }

    /// A row along +q at r = 0: a portal at the origin facing east, then one cell per entry of
    /// `middle` (def, level), then a portal facing west at level `far_level`.
    pub(crate) fn row(middle: &[(&str, u8)], far_level: u8) -> World {
        let mut tiles = BTreeMap::new();
        let a = Axial::ORIGIN;
        tiles.insert(a, tile("road_tunnel_portal", 0, 0, a));
        for (i, (def, level)) in middle.iter().enumerate() {
            let cell = Axial::new(i as i32 + 1, 0);
            tiles.insert(cell, tile(def, 0, *level, cell));
        }
        let b = Axial::new(middle.len() as i32 + 1, 0);
        tiles.insert(b, tile("road_tunnel_portal", 3, far_level, b));
        World::from_parts(1, tiles, VecDeque::new(), vec![])
    }

    pub(crate) fn meadows(n: usize) -> Vec<(&'static str, u8)> {
        vec![("meadow", 1); n]
    }

    fn without(w: &World, cell: Axial) -> World {
        let mut tiles = w.tiles().clone();
        tiles.remove(&cell);
        World::from_parts(1, tiles, VecDeque::new(), vec![])
    }

    /// Portals facing each other along world edge `dir` from `from`, with `n` level-1 meadows
    /// between them.
    fn line(from: Axial, dir: u8, n: usize) -> BTreeMap<Axial, PlacedTile> {
        let mut tiles = BTreeMap::new();
        tiles.insert(from, tile("road_tunnel_portal", dir, 0, from));
        let mut at = from;
        for _ in 0..n {
            at = at.neighbor(dir);
            tiles.insert(at, tile("meadow", 0, 1, at));
        }
        let b = at.neighbor(dir);
        tiles.insert(b, tile("road_tunnel_portal", (dir + 3) % 6, 0, b));
        tiles
    }

    #[test]
    fn tunnels_form_along_every_world_edge() {
        let c = catalog();
        for dir in 0..6u8 {
            let w = World::from_parts(1, line(Axial::ORIGIN, dir, 2), VecDeque::new(), vec![]);
            let t = tunnels(&w, &c);
            assert_eq!(t.len(), 1, "dir {dir}");
            let t = &t[0];
            assert!(t.a < t.b, "dir {dir}");
            let mut cover = vec![t.a.neighbor(t.dir)];
            cover.push(cover[0].neighbor(t.dir));
            assert_eq!(t.cover, cover, "dir {dir}");
            assert_eq!(cover[1].neighbor(t.dir), t.b, "dir {dir}");
            assert_eq!(
                t.dir,
                if t.a == Axial::ORIGIN {
                    dir
                } else {
                    (dir + 3) % 6
                }
            );
        }
    }

    #[test]
    fn a_nonzero_local_tunnel_edge_turns_with_the_rotation() {
        let c = catalog();
        let mut def = c.get("road_tunnel_portal").unwrap().clone();
        def.tunnel.as_mut().unwrap().edge = 2;
        // Local edge 2 under rotation 1 is world edge 3.
        assert_eq!(mouth(&def, 1), Some(3));
        let far = Axial::ORIGIN.neighbor(3).neighbor(3).neighbor(3);
        let mut tiles = BTreeMap::new();
        for cell in [
            Axial::ORIGIN.neighbor(3),
            Axial::ORIGIN.neighbor(3).neighbor(3),
        ] {
            tiles.insert(cell, tile("meadow", 0, 1, cell));
        }
        tiles.insert(far, tile("road_tunnel_portal", 0, 0, far));
        let w = World::from_parts(1, tiles, VecDeque::new(), vec![]);
        let p = preview(&w, &c, Axial::ORIGIN, &def, Orientation::new(1, 0));
        assert_eq!(p.cells.len(), 2);
        assert!(p.completes);
        // Rotation 0 points the same tunnel at world edge 2: no cover there.
        let p = preview(&w, &c, Axial::ORIGIN, &def, Orientation::new(0, 0));
        assert!(p.cells.is_empty() && !p.completes);
    }

    #[test]
    fn two_tunnels_come_out_sorted_by_first_portal() {
        let c = catalog();
        let mut tiles = line(Axial::new(0, 3), 0, 1);
        tiles.extend(line(Axial::new(0, 0), 0, 2));
        let w = World::from_parts(1, tiles, VecDeque::new(), vec![]);
        let t = tunnels(&w, &c);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].a, t[0].cover.len()), (Axial::new(0, 0), 2));
        assert_eq!((t[1].a, t[1].cover.len()), (Axial::new(0, 3), 1));
        assert!(is_cover(&w, &c, t[1].cover[0]) && is_cover(&w, &c, t[0].cover[1]));
    }

    #[test]
    fn two_facing_portals_over_meadow_make_a_tunnel() {
        let c = catalog();
        let w = row(&meadows(3), 0);
        assert_eq!(
            tunnels(&w, &c),
            vec![Tunnel {
                a: Axial::ORIGIN,
                b: Axial::new(4, 0),
                dir: 0,
                cover: vec![Axial::new(1, 0), Axial::new(2, 0), Axial::new(3, 0)],
                level: 0,
            }]
        );
        assert!(open_portals(&w, &c).is_empty());
    }

    #[test]
    fn adjacent_portals_make_a_tunnel_with_no_cover() {
        let c = catalog();
        let t = tunnels(&row(&[], 0), &c);
        assert_eq!(t.len(), 1);
        assert!(t[0].cover.is_empty());
    }

    #[test]
    fn lone_portal_is_open() {
        let c = catalog();
        let w = without(&row(&[], 0), Axial::new(1, 0));
        assert!(tunnels(&w, &c).is_empty());
        assert_eq!(open_portals(&w, &c), vec![Axial::ORIGIN]);
    }

    #[test]
    fn forest_in_the_line_breaks_it() {
        let c = catalog();
        let w = row(&[("meadow", 1), ("forest", 1), ("meadow", 1)], 0);
        assert!(tunnels(&w, &c).is_empty());
        assert_eq!(open_portals(&w, &c).len(), 2);
    }

    #[test]
    fn different_levels_do_not_join() {
        let c = catalog();
        assert!(tunnels(&row(&meadows(3), 1), &c).is_empty());
        // Cover at the wrong level also breaks the line.
        let w = row(&[("meadow", 1), ("meadow", 0), ("meadow", 1)], 0);
        assert!(tunnels(&w, &c).is_empty());
    }

    #[test]
    fn portals_not_facing_each_other_do_not_join() {
        let c = catalog();
        let w = row(&meadows(2), 0);
        let b = Axial::new(3, 0);
        let mut tiles = w.tiles().clone();
        tiles.get_mut(&b).unwrap().rotation = 0;
        let w = World::from_parts(1, tiles, VecDeque::new(), vec![]);
        assert!(tunnels(&w, &c).is_empty());
    }

    #[test]
    fn preview_reports_cells_and_completion() {
        let c = catalog();
        // The ghost stands where the first portal is, over a world that lacks it.
        let w = without(&row(&meadows(3), 0), Axial::ORIGIN);
        let def = c.get("road_tunnel_portal").unwrap();
        let p = preview(&w, &c, Axial::ORIGIN, def, Orientation::new(0, 0));
        assert_eq!(
            p,
            TunnelPreview {
                cells: vec![Axial::new(1, 0), Axial::new(2, 0), Axial::new(3, 0)],
                completes: true,
            }
        );
        // Wrong level: the meadows are not cover, nothing completes.
        let p = preview(&w, &c, Axial::ORIGIN, def, Orientation::new(0, 1));
        assert!(p.cells.is_empty() && !p.completes);
        // Not a portal.
        let meadow = c.get("meadow").unwrap();
        let p = preview(&w, &c, Axial::ORIGIN, meadow, Orientation::new(0, 0));
        assert!(p.cells.is_empty() && !p.completes);
        // No facing portal.
        let w = without(&w, Axial::new(4, 0));
        let p = preview(&w, &c, Axial::ORIGIN, def, Orientation::new(0, 0));
        assert_eq!(p.cells.len(), 3);
        assert!(!p.completes);
    }
}
