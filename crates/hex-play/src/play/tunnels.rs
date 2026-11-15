//! Open tunnels (P4, owner 2026-11-14): a complete tunnel draws a stone-lined bore under every
//! cover cell, and its two portals open onto it.
//!
//! - [`bores`] spawns one bore piece (`road_tunnel_bore` or `rail_tunnel_bore`) per cover cell of
//!   every complete tunnel in the view, turned along the tunnel and standing at its level, and
//!   rebuilds them when the tunnels change or a pack arrives.
//! - [`portal_slots`] hides a portal's `*_back` slot (the dark back wall) while its tunnel is
//!   complete and shows its `*_open` slots (the lining and ridge that run on to the cell edge)
//!   only then, so a lone portal looks as it was approved.
//! - [`cut_walls`] opens the soil walls that would plug the bore: the skirt and cliff column of
//!   every cover cell on its two tunnel-facing sides, and the portal's skirt on its mouth side.

use std::collections::{HashMap, HashSet};

use bevy::{
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
};
use hex_core::{Axial, HEX_RADIUS, LEVEL_STEP, NetworkKind, Tunnel};
use hex_play_state::{PlayCatalog, PlayTileRoot, RisePart};

use super::{PlayKit, PlayView, piece_rotation, spawn_kit};
use hex_render::{ActiveScenario, Scenario, SceneEntity};

/// A bore entity, drawn under a cover cell.
#[derive(Component)]
pub struct Bore;

/// A portal part that the tunnel being complete changes (by its manifest slot's suffix).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelSlot {
    /// `*_back`: the dark wall behind the arch, hidden while the tunnel is complete.
    Back,
    /// `*_open`: the lining and ridge that run on to the cell edge, shown only while complete.
    Open,
}

impl TunnelSlot {
    /// The role of a part drawn from manifest slot `slot`, if it has one.
    pub fn of(slot: &str) -> Option<Self> {
        if slot.ends_with("_back") {
            Some(Self::Back)
        } else if slot.contains("_open") {
            Some(Self::Open)
        } else {
            None
        }
    }

    /// Whether the part is drawn while its portal's tunnel is (`complete`) or is not complete.
    pub fn shown(self, complete: bool) -> bool {
        match self {
            Self::Back => !complete,
            Self::Open => complete,
        }
    }
}

/// Marks a part this module hid, so it can give the part back to the rise animation.
#[derive(Component)]
pub struct HiddenByTunnel;

/// The bore piece a tunnel of `kind` draws.
pub fn bore_piece(kind: NetworkKind) -> &'static str {
    match kind {
        NetworkKind::Rail => "rail_tunnel_bore",
        _ => "road_tunnel_bore",
    }
}

/// The root transform of the bore under `cell` of `tunnel`: the cell's centre at the tunnel's
/// floor level, the piece's +x along the tunnel.
pub fn bore_transform(tunnel: &Tunnel, cell: Axial) -> Transform {
    let [x, z] = cell.to_world(HEX_RADIUS);
    Transform::from_xyz(x, f32::from(tunnel.level) * LEVEL_STEP, z)
        .with_rotation(piece_rotation(tunnel.dir))
}

/// What [`bores`] last drew.
#[derive(Default)]
pub struct BoreState {
    tunnels: Vec<Tunnel>,
    drawn: bool,
}

/// Draws the bores of the view's complete tunnels.
pub fn bores(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    kit: Option<Res<PlayKit>>,
    existing: Query<Entity, With<Bore>>,
    mut state: Local<BoreState>,
) {
    let play = active.0 == Scenario::Play;
    let want: &[Tunnel] = if play { &view.tunnels } else { &[] };
    let kit_changed = kit.as_ref().is_some_and(|k| k.is_changed());
    if state.tunnels == want && !kit_changed && (state.drawn || want.is_empty()) {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    state.tunnels = want.to_vec();
    state.drawn = false;
    let Some(kit) = kit else {
        return;
    };
    let (PlayKit::Loading(pieces) | PlayKit::Ready(pieces)) = &*kit;
    let mut all = true;
    for t in want {
        let piece = bore_piece(t.kind);
        if !pieces.contains_key(piece) {
            all = false;
            continue;
        }
        for &cell in &t.cover {
            commands
                .spawn((
                    Bore,
                    bore_transform(t, cell),
                    Visibility::default(),
                    SceneEntity,
                ))
                .with_children(|p| {
                    spawn_kit(p, pieces, piece, Transform::IDENTITY, None);
                });
        }
    }
    // A bore whose pack is still streaming is drawn when it arrives (the kit changes then).
    state.drawn = all;
}

/// Hides and shows the portal parts that depend on the tunnel being complete. A part hidden here
/// is handed back to the rise animation (`Inherited`) the frame its tunnel stops needing it.
pub fn portal_slots(
    view: Res<PlayView>,
    mut commands: Commands,
    mut parts: Query<(
        Entity,
        &TunnelSlot,
        &hex_play_state::RisePart,
        &mut Visibility,
        Has<HiddenByTunnel>,
    )>,
) {
    if parts.is_empty() {
        return;
    }
    let complete: HashSet<Axial> = view.tunnels.iter().flat_map(|t| [t.a, t.b]).collect();
    for (e, slot, rise, mut visibility, hidden) in &mut parts {
        if slot.shown(complete.contains(&rise.cell)) {
            if hidden {
                *visibility = Visibility::Inherited;
                commands.entity(e).remove::<HiddenByTunnel>();
            }
        } else {
            visibility.set_if_neq(Visibility::Hidden);
            if !hidden {
                commands.entity(e).insert(HiddenByTunnel);
            }
        }
    }
}

/// A tile's soil wall (its skirt or cliff column): [`cut_walls`] may open sides of it while a
/// tunnel runs through the tile. A portal's skirt is tagged with its own mouth, so a cut that only
/// concerns the portal side cannot touch its other sides.
#[derive(Component)]
pub struct WallPart;

/// A wall's own mesh and the sides cut out of what it draws now, empty while it draws it whole.
#[derive(Component)]
pub struct WallCut {
    original: Handle<Mesh>,
    cut: Vec<u8>,
}

/// Distance (m) a wall vertex may stand from its side's plane and still lie on that side.
const WALL_TOL: f32 = 0.3;

/// The outward unit normal of local edge `e` in the tile frame (x, z).
fn edge_normal(e: u8) -> Vec2 {
    let a = (60.0 * f32::from(e % 6)).to_radians();
    Vec2::new(a.cos(), a.sin())
}

/// A copy of `mesh` without the triangles that lie wholly on the planes of local edges `edges`
/// (the hex's apothem out along each edge's normal). `None` when the mesh has none, or no indices.
pub fn cut_edges(mesh: &Mesh, edges: &[u8]) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let apothem = HEX_RADIUS * 0.866_025_4;
    let on = |i: u32, e: u8| {
        let p = pos[i as usize];
        Vec2::new(p[0], p[2]).dot(edge_normal(e)) >= apothem - WALL_TOL
    };
    let tris: Vec<[u32; 3]> = match mesh.indices()? {
        Indices::U32(ix) => ix.as_chunks::<3>().0.to_vec(),
        Indices::U16(ix) => ix
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(u32::from))
            .collect(),
    };
    let kept: Vec<u32> = tris
        .iter()
        .filter(|t| !edges.iter().any(|&e| t.iter().all(|&i| on(i, e))))
        .flatten()
        .copied()
        .collect();
    if kept.len() == tris.len() * 3 {
        return None;
    }
    let mut out = mesh.clone();
    out.insert_indices(Indices::U32(kept));
    Some(out)
}

/// Half the width (m) of the turf ridge over a complete rail tunnel: it eases down to the 4 m cover
/// by 7 m from the axis and dips just under it by 7.4 m (`rail_tunnel_bore`).
pub const RIDGE_HALF: f32 = 7.4;

/// The strip of a cell's ground the rail tunnel's turf ridge covers (owner Q4: nothing grows or
/// stands on it), in the tile's own frame: `from` to `to` along `axis`, `RIDGE_HALF` either side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ridge {
    pub axis: Vec2,
    pub from: f32,
    pub to: f32,
}

impl Ridge {
    /// Whether a point `p` (tile frame, x and z) is within `pad` of the ridge.
    pub fn holds(&self, p: Vec2, pad: f32) -> bool {
        let along = p.dot(self.axis);
        let across = p.dot(self.axis.perp()).abs();
        (self.from - pad..=self.to + pad).contains(&along) && across <= RIDGE_HALF + pad
    }

    /// The ridge as a rectangle: its centre, half extents along its own x and z, and its +x axis.
    pub fn as_box(&self) -> (Vec2, Vec2, Vec2) {
        let mid = (self.from + self.to) / 2.0;
        (
            self.axis * mid,
            Vec2::new((self.to - self.from) / 2.0, RIDGE_HALF),
            self.axis,
        )
    }

    /// A stable fingerprint, mixed into a tile's scatter key so its layout is redone when a ridge
    /// appears or goes.
    pub fn sig(&self) -> u64 {
        [self.axis.x, self.axis.y, self.from, self.to]
            .iter()
            .fold(0xCBF2_9CE4_8422_2325u64, |h, v| {
                (h ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01B3)
            })
    }
}

/// The ridges over complete rail tunnels, by cell: the whole cell for a cover cell, and a portal's
/// side from just in front of its face to its edge. A road tunnel has none (its cover is not
/// broken).
pub fn ridges(view: &PlayView) -> HashMap<Axial, Ridge> {
    let apothem = HEX_RADIUS * 0.866_025_4;
    let rotation: HashMap<Axial, u8> = view.tiles.iter().map(|t| (t.cell, t.rotation)).collect();
    let local =
        |cell: &Axial, world: u8| (world + 6 - rotation.get(cell).copied().unwrap_or(0) % 6) % 6;
    let mut out = HashMap::new();
    for t in view.tunnels.iter().filter(|t| t.kind == NetworkKind::Rail) {
        for cell in &t.cover {
            out.insert(
                *cell,
                Ridge {
                    axis: edge_normal(local(cell, t.dir)),
                    from: -apothem - 0.5,
                    to: apothem + 0.5,
                },
            );
        }
        for (cell, world) in [(t.a, t.dir), (t.b, (t.dir + 3) % 6)] {
            out.insert(
                cell,
                Ridge {
                    axis: edge_normal(local(&cell, world)),
                    from: apothem - 6.0 - 1.0,
                    to: apothem + 0.5,
                },
            );
        }
    }
    out
}

/// The local edges to cut per cell: a cover cell's two tunnel-facing sides, and each portal's
/// mouth side, in the tile's own frame (the tile's rotation turns the world edge into a local one).
fn cuts_of(view: &PlayView, catalog: &PlayCatalog) -> HashMap<Axial, Vec<u8>> {
    let rotation: HashMap<Axial, u8> = view.tiles.iter().map(|t| (t.cell, t.rotation)).collect();
    let local =
        |cell: &Axial, world: u8| (world + 6 - rotation.get(cell).copied().unwrap_or(0) % 6) % 6;
    let mut out: HashMap<Axial, Vec<u8>> = HashMap::new();
    for t in &view.tunnels {
        for cell in &t.cover {
            out.insert(
                *cell,
                vec![local(cell, t.dir), local(cell, (t.dir + 3) % 6)],
            );
        }
        for (cell, world) in [(t.a, t.dir), (t.b, (t.dir + 3) % 6)] {
            out.insert(cell, vec![local(&cell, world)]);
        }
    }
    let _ = catalog;
    out
}

/// Opens the walls of the cells a complete tunnel runs through, and gives them back whole when
/// it stops being complete.
#[allow(clippy::too_many_arguments)]
pub fn cut_walls(
    mut commands: Commands,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    mut meshes: ResMut<Assets<Mesh>>,
    roots: Query<&RisePart, With<PlayTileRoot>>,
    mut walls: Query<(Entity, &ChildOf, &mut Mesh3d, Option<&mut WallCut>), With<WallPart>>,
    added: Query<(), Added<WallPart>>,
    mut last: Local<Vec<Tunnel>>,
    mut cache: Local<HashMap<(AssetId<Mesh>, Vec<u8>), Option<Handle<Mesh>>>>,
) {
    if *last == view.tunnels && added.is_empty() {
        return;
    }
    last.clone_from(&view.tunnels);
    let cuts = cuts_of(&view, &catalog);
    for (e, parent, mut mesh, cut) in &mut walls {
        let Ok(root) = roots.get(parent.parent()) else {
            continue;
        };
        let want: Vec<u8> = cuts.get(&root.cell).cloned().unwrap_or_default();
        let (original, drawn) = match &cut {
            Some(c) => (c.original.clone(), c.cut.clone()),
            None => (mesh.0.clone(), vec![]),
        };
        if want == drawn {
            continue;
        }
        let handle = if want.is_empty() {
            original.clone()
        } else {
            let key = (original.id(), want.clone());
            cache
                .entry(key)
                .or_insert_with(|| {
                    meshes
                        .get(&original)
                        .and_then(|m| cut_edges(m, &want))
                        .map(|m| meshes.add(m))
                })
                .clone()
                .unwrap_or_else(|| original.clone())
        };
        mesh.0 = handle;
        match cut {
            Some(mut c) => c.cut = want,
            None => {
                commands.entity(e).insert(WallCut {
                    original,
                    cut: want,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_read_their_role_from_the_suffix() {
        assert_eq!(
            TunnelSlot::of("rail_tunnel_portal_back"),
            Some(TunnelSlot::Back)
        );
        assert_eq!(
            TunnelSlot::of("road_tunnel_portal_back"),
            Some(TunnelSlot::Back)
        );
        assert_eq!(
            TunnelSlot::of("rail_tunnel_portal_open_lining"),
            Some(TunnelSlot::Open)
        );
        assert_eq!(TunnelSlot::of("rail_tunnel_portal_stone"), None);
        assert!(TunnelSlot::Back.shown(false) && !TunnelSlot::Back.shown(true));
        assert!(TunnelSlot::Open.shown(true) && !TunnelSlot::Open.shown(false));
    }

    /// The soil walls open on the cut sides only: a hex prism wall cut on local edges 0 and 3 loses
    /// two of its six sides, and cutting nothing leaves the mesh alone.
    #[test]
    fn cutting_edges_removes_those_wall_sides_only() {
        let wall = super::super::slope_ground::column_mesh(8.0);
        let tris = |m: &Mesh| m.indices().map_or(0, |i| i.len()) / 3;
        assert_eq!(tris(&wall), 12);
        assert!(cut_edges(&wall, &[]).is_none());
        let cut = cut_edges(&wall, &[0, 3]).expect("two sides go");
        assert_eq!(tris(&cut), 8);
        let one = cut_edges(&wall, &[0]).expect("one side goes");
        assert_eq!(tris(&one), 10);
        assert_eq!(tris(&wall), 12, "the original is untouched");
    }

    /// A complete rail tunnel's ridge clears its cover cells and the mouth side of its portals; a
    /// road tunnel has none.
    #[test]
    fn rail_tunnels_have_a_ridge_to_keep_clear() {
        let tunnel = |kind| Tunnel {
            a: Axial::new(0, 0),
            b: Axial::new(3, 0),
            dir: 0,
            cover: vec![Axial::new(1, 0), Axial::new(2, 0)],
            level: 0,
            kind,
        };
        let view = |kind| PlayView {
            tunnels: vec![tunnel(kind)],
            ..Default::default()
        };
        assert!(ridges(&view(NetworkKind::Road)).is_empty());
        let r = ridges(&view(NetworkKind::Rail));
        assert_eq!(r.len(), 4);
        let cover = r[&Axial::new(1, 0)];
        assert!(cover.holds(Vec2::new(0.0, 0.0), 0.0));
        assert!(cover.holds(Vec2::new(10.0, 7.0), 0.0));
        assert!(!cover.holds(Vec2::new(0.0, 8.0), 0.0), "beside the ridge");
        let portal = r[&Axial::new(0, 0)];
        assert!(
            !portal.holds(Vec2::new(-5.0, 0.0), 0.0),
            "the portal's back is clear"
        );
        assert!(
            portal.holds(Vec2::new(12.0, 0.0), 0.0),
            "the mouth side is ridge"
        );
        assert_ne!(cover.sig(), portal.sig());
    }

    #[test]
    fn a_bore_stands_under_its_cover_cell_along_the_tunnel() {
        let t = Tunnel {
            a: Axial::new(0, 0),
            b: Axial::new(3, 0),
            dir: 0,
            cover: vec![Axial::new(1, 0), Axial::new(2, 0)],
            level: 2,
            kind: NetworkKind::Rail,
        };
        let tf = bore_transform(&t, Axial::new(1, 0));
        let [x, z] = Axial::new(1, 0).to_world(HEX_RADIUS);
        assert_eq!(tf.translation, Vec3::new(x, 2.0 * LEVEL_STEP, z));
        assert_eq!(bore_piece(NetworkKind::Rail), "rail_tunnel_bore");
        assert_eq!(bore_piece(NetworkKind::Road), "road_tunnel_bore");
    }
}
