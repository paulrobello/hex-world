//! Developer overlays (owner request 2026-10-14): the Settings Debug tab, the `debug` bridge command
//! and the `debug=<list>` flag drive one [`DebugOverlays`] resource.
//!
//! - **LOD tint** swaps every LOD-tagged kit or car part's material for one of three shared
//!   [`StandardMaterial`]s by its level and puts the original back when it turns off. The kit forward
//!   pipeline is untouched, so it gains no sampled texture (it sits at WebGPU's 16 next to
//!   refracting water).
//! - **Force LOD** pins every LOD-tagged part to one level by swapping its [`VisibilityRange`] and
//!   restores [`lod_range`] on Auto. The instanced gate layout picks its LOD per chunk on the GPU
//!   path and is not covered.
//! - **Perf** samples frame times over the last second, counts entities, and with the LOD overlays
//!   counts drawn LOD parts and their triangles.
//! - **Grid**, **sockets**, **colliders** and **scatter** draw [`DebugGizmos`] lines over the play
//!   world: hex outlines of placed tiles and free slots, each placed tile's edges in its socket
//!   colours, every walk obstacle footprint, and each scatter instance's ground footprint with the
//!   disc grass keeps clear around it. The axial labels are the web overlay's (projected per cell).
//!   `hex-app` draws the **networks** overlay (the car and river routing graphs it owns) into the
//!   same gizmo group.
//!
//! Every overlay is off by default, nothing persists, and an off overlay runs no system: each one
//! is gated by a run condition on [`DebugOverlays`] (see `off_overlays_run_nothing`).
use std::collections::{HashMap, VecDeque};

use bevy::{camera::visibility::VisibilityRange, prelude::*};
use hex_core::{Axial, HEX_RADIUS};
use hex_render::kit_instances::KitLodDebug;

use crate::{
    car_material::CarMaterial,
    kit_material::KitMaterial,
    kit_scene::piece_rotation,
    play::{
        PlayCatalog, PlayView,
        affordances::socket_rgb,
        grass::{Clear, exclusions},
        scatter::{ScatterLayouts, footprint},
    },
    scene::{Lod, lod_range},
    walk::WalkGround,
};

/// The overlay switches. `Default` is everything off.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugOverlays {
    /// Colour every LOD-tagged part by its level.
    pub lod_tint: bool,
    /// Pin every LOD-tagged part to this level; `None` is Auto (distance bands).
    pub force_lod: Option<u8>,
    /// The performance readout.
    pub perf: bool,
    /// Hex outlines of placed tiles and free slots (the web overlay adds the axial labels).
    pub grid: bool,
    /// Each placed tile's six edges in its socket colours.
    pub sockets: bool,
    /// Walk obstacle footprints (the colliders first-person walking stops at).
    pub colliders: bool,
    /// Scatter instance footprints and the grass exclusion discs around them.
    pub scatter: bool,
    /// The road and river routing graphs: link centerlines and nodes (drawn by `hex-app`).
    pub networks: bool,
}

/// The names `debug=<list>` and the bridge use, in tab order.
pub const OVERLAYS: [&str; 7] = [
    "lod",
    "perf",
    "grid",
    "sockets",
    "colliders",
    "scatter",
    "networks",
];

impl DebugOverlays {
    /// Turns the overlay `name` (one of [`OVERLAYS`]) on or off; false for an unknown name.
    pub fn set(&mut self, name: &str, on: bool) -> bool {
        match name {
            "lod" => self.lod_tint = on,
            "perf" => self.perf = on,
            "grid" => self.grid = on,
            "sockets" => self.sockets = on,
            "colliders" => self.colliders = on,
            "scatter" => self.scatter = on,
            "networks" => self.networks = on,
            _ => return false,
        }
        true
    }

    /// `auto`, `0`, `1` or `2`; `None` for anything else.
    pub fn parse_force(s: &str) -> Option<Option<u8>> {
        match s {
            "auto" => Some(None),
            "0" => Some(Some(0)),
            "1" => Some(Some(1)),
            "2" => Some(Some(2)),
            _ => None,
        }
    }

    /// A comma list of [`OVERLAYS`] names plus `lod0`, `lod1` or `lod2` (Force LOD), as the
    /// `debug` flag takes it. Unknown names come back for a warning.
    pub fn parse_list(list: &str) -> (DebugOverlays, Vec<String>) {
        let mut d = DebugOverlays::default();
        let mut unknown = vec![];
        for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let forced = name
                .strip_prefix("lod")
                .and_then(Self::parse_force)
                .flatten();
            if let Some(l) = forced {
                d.force_lod = Some(l);
            } else if !d.set(name, true) {
                unknown.push(name.to_owned());
            }
        }
        (d, unknown)
    }

    /// The LOD census runs for the tint, a forced level and the perf readout.
    fn counts_lods(&self) -> bool {
        self.lod_tint || self.force_lod.is_some() || self.perf
    }

    /// `state().debug`'s switches and the latest report.
    pub fn json(&self, r: &DebugReport) -> serde_json::Value {
        let round = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
        serde_json::json!({
            "lod_tint": self.lod_tint,
            "force_lod": self.force_lod.map_or_else(|| "auto".to_owned(), |l| l.to_string()),
            "perf": self.perf,
            "grid": self.grid,
            "sockets": self.sockets,
            "colliders": self.colliders,
            "scatter": self.scatter,
            "networks": self.networks,
            "lod": { "by_lod": r.by_lod, "tris_by_lod": r.tris_by_lod },
            "frame": {
                "fps": round(r.fps),
                "mean_ms": round(r.mean_ms),
                "worst_ms": round(r.worst_ms),
                "entities": r.entities,
            },
            "work": r.work,
        })
    }
}

/// What the overlays measured. Counts update every [`CENSUS_FRAMES`] frames while an overlay that
/// needs them is on, and hold their last values while it is off.
#[derive(Resource, Clone, Debug, Default)]
pub struct DebugReport {
    /// Drawn LOD-tagged parts per level (visible in any view, the camera or a shadow cascade).
    pub by_lod: [u32; 3],
    /// Their triangles per level.
    pub tris_by_lod: [u64; 3],
    pub fps: f32,
    /// Mean and worst frame time over the last second.
    pub mean_ms: f32,
    pub worst_ms: f32,
    pub entities: u32,
    /// Overlay system runs since start: stays 0 while every overlay is off.
    pub work: u64,
    /// Parts wear a tint material that has not been put back yet.
    tinted: bool,
    /// Parts carry a forced `VisibilityRange` that has not been put back yet.
    forced: bool,
    frames: VecDeque<(f32, f32)>,
}

const CENSUS_FRAMES: u32 = 15;

/// LOD0, LOD1 and LOD2 tints: three viridis stops, ordered by lightness so they also read without
/// hue (colour-blind safe). The web legend uses the same values.
pub const LOD_TINTS: [[u8; 3]; 3] = [[253, 231, 37], [33, 145, 140], [68, 1, 84]];

/// Lines over the play world: never depth-tested away under a tile top.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DebugGizmos;

#[derive(Resource)]
struct TintMaterials([Handle<StandardMaterial>; 3]);

/// A part's own material while it wears its LOD tint.
#[derive(Component)]
struct TintSaved<M: Material>(Handle<M>);

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugOverlays>()
            .init_resource::<DebugReport>()
            .init_resource::<KitLodDebug>()
            .init_gizmo_group::<DebugGizmos>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (tint_on::<KitMaterial>, tint_on::<CarMaterial>)
                        .run_if(|d: Res<DebugOverlays>| d.lod_tint),
                    (tint_off::<KitMaterial>, tint_off::<CarMaterial>, tint_done)
                        .chain()
                        .run_if(|d: Res<DebugOverlays>, r: Res<DebugReport>| {
                            !d.lod_tint && r.tinted
                        }),
                    force_all.run_if(|d: Res<DebugOverlays>, r: Res<DebugReport>| {
                        d.is_changed() && (d.force_lod.is_some() || r.forced)
                    }),
                    force_new.run_if(|d: Res<DebugOverlays>| d.force_lod.is_some()),
                    sync_kit_lod_debug.run_if(resource_changed::<DebugOverlays>),
                    count_lods.run_if(|d: Res<DebugOverlays>| d.counts_lods()),
                    sample_frames.run_if(|d: Res<DebugOverlays>| d.perf),
                    draw_grid.run_if(|d: Res<DebugOverlays>| d.grid),
                    draw_sockets.run_if(|d: Res<DebugOverlays>| d.sockets),
                    draw_colliders.run_if(|d: Res<DebugOverlays>| d.colliders),
                    draw_scatter.run_if(|d: Res<DebugOverlays>| d.scatter),
                ),
            );
    }
}

fn setup(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    mut store: ResMut<GizmoConfigStore>,
) {
    let tints = LOD_TINTS.map(|[r, g, b]| {
        let c = Color::srgb_u8(r, g, b);
        materials.add(StandardMaterial {
            base_color: c,
            emissive: c.to_linear() * 0.25,
            perceptual_roughness: 0.9,
            ..default()
        })
    });
    commands.insert_resource(TintMaterials(tints));
    let (config, _) = store.config_mut::<DebugGizmos>();
    config.depth_bias = -1.0;
    config.line.width = 2.5;
}

fn tint_on<M: Material>(
    mut commands: Commands,
    parts: Query<(Entity, &Lod, &MeshMaterial3d<M>)>,
    tints: Res<TintMaterials>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for (e, lod, mat) in &parts {
        let tint = tints.0[(lod.0 as usize).min(2)].clone();
        commands
            .entity(e)
            .remove::<MeshMaterial3d<M>>()
            .insert((TintSaved::<M>(mat.0.clone()), MeshMaterial3d(tint)));
        report.tinted = true;
    }
}

fn tint_off<M: Material>(
    mut commands: Commands,
    parts: Query<(Entity, &TintSaved<M>)>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for (e, saved) in &parts {
        commands
            .entity(e)
            .remove::<(TintSaved<M>, MeshMaterial3d<StandardMaterial>)>()
            .insert(MeshMaterial3d(saved.0.clone()));
    }
}

fn tint_done(mut report: ResMut<DebugReport>) {
    report.tinted = false;
}

/// Always visible, or never.
fn pinned_range(show: bool) -> VisibilityRange {
    let far = 1.0e9;
    VisibilityRange {
        start_margin: if show { 0.0..0.0 } else { far..far },
        end_margin: far..far,
        use_aabb: false,
    }
}

fn range_for(lod: u8, force: Option<u8>) -> VisibilityRange {
    match force {
        Some(f) => pinned_range(f == lod),
        None => lod_range(lod as usize),
    }
}

fn force_all(
    d: Res<DebugOverlays>,
    mut parts: Query<(&Lod, &mut VisibilityRange)>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for (lod, mut range) in &mut parts {
        *range = range_for(lod.0, d.force_lod);
    }
    report.forced = d.force_lod.is_some();
}

/// Parts spawned while a level is forced (placements, pair edges, cars) take it at once.
fn force_new(
    d: Res<DebugOverlays>,
    mut parts: Query<(&Lod, &mut VisibilityRange), Added<Lod>>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for (lod, mut range) in &mut parts {
        *range = range_for(lod.0, d.force_lod);
    }
}

/// Settled kit parts draw instanced, with no per-part entity for the force or the tint to act
/// on; the instanced path reads both from this resource instead.
fn sync_kit_lod_debug(d: Res<DebugOverlays>, mut kit: ResMut<KitLodDebug>) {
    let want = KitLodDebug {
        force: d.force_lod,
        tint: d.lod_tint,
    };
    if *kit != want {
        *kit = want;
    }
}

fn mesh_tris(m: &Mesh) -> u64 {
    m.indices()
        .map_or(m.count_vertices(), |i| i.len())
        .div_euclid(3) as u64
}

fn count_lods(
    parts: Query<(&Lod, &ViewVisibility, &Mesh3d)>,
    meshes: Res<Assets<Mesh>>,
    mut tris: Local<HashMap<AssetId<Mesh>, u64>>,
    mut frame: Local<u32>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    *frame += 1;
    if *frame % CENSUS_FRAMES != 1 {
        return;
    }
    let (mut by_lod, mut by_tris) = ([0u32; 3], [0u64; 3]);
    for (lod, vis, mesh) in &parts {
        if !vis.get() {
            continue;
        }
        let l = (lod.0 as usize).min(2);
        by_lod[l] += 1;
        let id = mesh.id();
        let t = match tris.get(&id) {
            Some(t) => *t,
            None => {
                let Some(m) = meshes.get(id) else { continue };
                let t = mesh_tris(m);
                tris.insert(id, t);
                t
            }
        };
        by_tris[l] += t;
    }
    report.by_lod = by_lod;
    report.tris_by_lod = by_tris;
}

fn sample_frames(
    time: Res<Time<Real>>,
    all: Query<Entity>,
    mut frame: Local<u32>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    let now = time.elapsed_secs();
    let ms = time.delta_secs() * 1000.0;
    report.frames.push_back((now, ms));
    while report.frames.front().is_some_and(|(t, _)| now - t > 1.0) {
        report.frames.pop_front();
    }
    *frame += 1;
    if *frame % CENSUS_FRAMES != 1 {
        return;
    }
    let n = report.frames.len().max(1) as f32;
    let sum: f32 = report.frames.iter().map(|(_, ms)| ms).sum();
    report.mean_ms = sum / n;
    report.worst_ms = report.frames.iter().map(|(_, ms)| *ms).fold(0.0, f32::max);
    report.fps = if report.mean_ms > 0.0 {
        1000.0 / report.mean_ms
    } else {
        0.0
    };
    report.entities = all.iter().count() as u32;
}

/// Ground lines sit this high so they never z-fight a flat tile top.
pub const LINE_Y: f32 = 0.25;

/// Corner `i` of a hex at the origin: corner `e - 1` and corner `e` bound world edge `e`.
fn corner(i: u8) -> Vec2 {
    let a = (30.0 + 60.0 * f32::from(i % 6)).to_radians();
    Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
}

fn cell_centre(c: Axial) -> Vec2 {
    let [x, z] = c.to_world(HEX_RADIUS);
    Vec2::new(x, z)
}

fn at(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
}

fn hex_loop(c: Axial, inset: f32) -> impl Iterator<Item = Vec3> {
    let o = cell_centre(c);
    (0..=6u8).map(move |i| at(o + corner(i) * inset, LINE_Y))
}

const GRID_TILE: Color = Color::srgb(0.94, 0.95, 0.97);
const GRID_SLOT: Color = Color::srgb(0.55, 0.6, 0.68);

fn draw_grid(
    view: Res<PlayView>,
    mut gizmos: Gizmos<DebugGizmos>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for t in &view.tiles {
        gizmos.linestrip(hex_loop(t.cell, 1.0), GRID_TILE);
    }
    for &c in &view.frontier {
        gizmos.linestrip(hex_loop(c, 0.97), GRID_SLOT);
    }
}

fn draw_sockets(
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    mut gizmos: Gizmos<DebugGizmos>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for t in &view.tiles {
        let Some(def) = catalog.0.get(&t.tile) else {
            continue;
        };
        let o = cell_centre(t.cell);
        for e in 0..6u8 {
            let [r, g, b] = socket_rgb(def.edge(e, t.rotation).kind);
            let c = Color::srgb_u8(r, g, b);
            // Inside the outline, so two tiles' marks on a shared edge sit side by side.
            let (a, b) = (o + corner(e + 5) * 0.86, o + corner(e) * 0.86);
            gizmos.line(at(a, LINE_Y), at(b, LINE_Y), c);
            let mid = (a + b) / 2.0;
            gizmos.line(at(mid, LINE_Y), at(mid + (o - mid) * 0.12, LINE_Y), c);
        }
    }
}

const COLLIDER: Color = Color::srgb(1.0, 0.36, 0.36);

fn draw_colliders(
    ground: Res<WalkGround>,
    mut gizmos: Gizmos<DebugGizmos>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    for o in ground.obstacles() {
        let Some(&first) = o.poly.first() else {
            continue;
        };
        let y = ground.top(first).unwrap_or(0.0) + LINE_Y;
        gizmos.linestrip(o.poly.iter().chain([&first]).map(|p| at(*p, y)), COLLIDER);
    }
}

const FOOTPRINT: Color = Color::srgb(0.9, 0.62, 0.0);
const GRASS_CLEAR: Color = Color::srgb(0.34, 0.71, 0.91);

/// A flat circle of `radius` about `c`.
pub fn ring(gizmos: &mut Gizmos<DebugGizmos>, c: Vec3, radius: f32, color: Color) {
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    gizmos
        .circle(Isometry3d::new(c, flat), radius, color)
        .resolution(20);
}

/// Each drawn scatter instance's ground footprint (orange) and every disc or box no grass blade
/// roots in (blue: scatter instances and hero props, `grass::exclusions`).
fn draw_scatter(
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    layouts: Res<ScatterLayouts>,
    mut gizmos: Gizmos<DebugGizmos>,
    mut report: ResMut<DebugReport>,
) {
    report.work += 1;
    let turns: HashMap<Axial, u8> = view.tiles.iter().map(|t| (t.cell, t.rotation)).collect();
    for (cell, s) in &layouts.tiles {
        let (Some(&rot), Some(def)) = (turns.get(cell), catalog.0.get(&s.tile)) else {
            continue;
        };
        let o = at(cell_centre(*cell), 0.0);
        let turn = piece_rotation(rot);
        let world = |p: Vec3| o + turn * p;
        for i in s
            .layout
            .instances
            .iter()
            .filter(|i| s.drawn[i.rule as usize])
        {
            let r = footprint(s.piece_of(i)).ground * i.scale;
            ring(&mut gizmos, world(i.pos) + Vec3::Y * LINE_Y, r, FOOTPRINT);
        }
        for c in exclusions(def, *cell, &s.layout.instances, &s.pieces) {
            match c {
                Clear::Disc { centre, radius } => ring(
                    &mut gizmos,
                    world(Vec3::new(centre.x, LINE_Y, centre.y)),
                    radius,
                    GRASS_CLEAR,
                ),
                Clear::Box { centre, half, axis } => {
                    let corner = |sx: f32, sz: f32| {
                        let p = centre + axis * (sx * half.x) + axis.perp() * (sz * half.y);
                        world(Vec3::new(p.x, LINE_Y, p.y))
                    };
                    gizmos.linestrip(
                        [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.), (-1., -1.)]
                            .map(|(sx, sz)| corner(sx, sz)),
                        GRASS_CLEAR,
                    );
                }
                Clear::Rock {
                    centre,
                    axis,
                    scale,
                    outline,
                } => {
                    let at = |k: usize| {
                        let [x, z] = outline[k % outline.len()];
                        let p = centre + (axis * x + axis.perp() * z) * scale;
                        world(Vec3::new(p.x, LINE_Y, p.y))
                    };
                    gizmos.linestrip((0..=outline.len()).map(at), GRASS_CLEAR);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_flag_list() {
        let (d, unknown) = DebugOverlays::parse_list("lod, grid,lod2,bogus,,perf");
        assert!(d.lod_tint && d.grid && d.perf && !d.sockets && !d.colliders && !d.scatter);
        assert_eq!(d.force_lod, Some(2));
        assert_eq!(unknown, ["bogus"]);
        assert_eq!(DebugOverlays::parse_list("").0, DebugOverlays::default());
        for name in OVERLAYS {
            assert!(DebugOverlays::default().set(name, true), "{name}");
        }
    }

    #[test]
    fn forced_ranges_show_one_level_and_auto_restores_the_bands() {
        for f in 0..3u8 {
            for l in 0..3u8 {
                let r = range_for(l, Some(f));
                let shown = r.start_margin.start <= 1.0e6;
                assert_eq!(shown, f == l, "force {f} lod {l}");
            }
        }
        assert!(range_for(1, None) == lod_range(1));
    }

    /// A headless app with LOD parts: every overlay off runs no overlay system, swaps no material
    /// and pins no range; turning them on does each.
    #[test]
    fn off_overlays_run_nothing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<bevy::gizmos::GizmoAsset>()
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<KitMaterial>()
            .init_asset::<CarMaterial>()
            .init_resource::<PlayView>()
            .init_resource::<PlayCatalog>()
            .init_resource::<WalkGround>()
            .init_resource::<ScatterLayouts>()
            .add_plugins(DebugPlugin);
        let kit = app
            .world_mut()
            .resource_mut::<Assets<KitMaterial>>()
            .add(KitMaterial {
                base: StandardMaterial::default(),
                extension: Default::default(),
            });
        let parts: Vec<Entity> = (0..3u8)
            .map(|l| {
                app.world_mut()
                    .spawn((
                        Lod(l),
                        lod_range(l as usize),
                        MeshMaterial3d(kit.clone()),
                        Mesh3d(Handle::default()),
                        ViewVisibility::default(),
                    ))
                    .id()
            })
            .collect();
        for _ in 0..10 {
            app.update();
        }
        let report = app.world().resource::<DebugReport>();
        assert_eq!(report.work, 0, "an off overlay ran a system");
        for &e in &parts {
            let w = app.world();
            assert!(w.get::<TintSaved<KitMaterial>>(e).is_none());
            assert!(w.get::<MeshMaterial3d<KitMaterial>>(e).is_some());
        }
        {
            let mut d = app.world_mut().resource_mut::<DebugOverlays>();
            d.lod_tint = true;
            d.force_lod = Some(2);
            d.perf = true;
            d.grid = true;
            d.sockets = true;
            d.colliders = true;
            d.scatter = true;
        }
        app.update();
        app.update();
        let w = app.world();
        assert!(w.resource::<DebugReport>().work > 0);
        // Settled kit parts draw instanced, with no entity to tint or force: they follow too.
        assert_eq!(
            *w.resource::<KitLodDebug>(),
            KitLodDebug {
                force: Some(2),
                tint: true
            }
        );
        for (l, &e) in parts.iter().enumerate() {
            assert!(
                w.get::<TintSaved<KitMaterial>>(e).is_some(),
                "lod {l} tinted"
            );
            assert!(
                w.get::<MeshMaterial3d<KitMaterial>>(e).is_none(),
                "lod {l} drawn once"
            );
            assert!(*w.get::<VisibilityRange>(e).unwrap() == range_for(l as u8, Some(2)));
        }
        *app.world_mut().resource_mut::<DebugOverlays>() = DebugOverlays::default();
        app.update();
        app.update();
        let work = app.world().resource::<DebugReport>().work;
        for (l, &e) in parts.iter().enumerate() {
            let w = app.world();
            assert!(
                w.get::<MeshMaterial3d<KitMaterial>>(e).is_some(),
                "lod {l} restored"
            );
            assert!(w.get::<MeshMaterial3d<StandardMaterial>>(e).is_none());
            assert!(*w.get::<VisibilityRange>(e).unwrap() == lod_range(l));
        }
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<DebugReport>().work,
            work,
            "systems ran after everything turned off"
        );
    }
}
