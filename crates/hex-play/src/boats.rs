//! Boats in `scenario=play` (owner D4, 2026-10-20): rowing boats on the rivers and sailing boats on
//! the sea, some moored at a landing stage or anchored off a beach. [`plan`] decides them and moves
//! them on the fixed tick; this module keeps them in step with the view, draws them riding the
//! water surface, swings their oars and sails, tells the walker where they are and the water where
//! not to draw.
//!
//! - **Derived.** The plan is rebuilt from the view's water tiles (and bridges) and the world seed
//!   when either changes, and never saved: a loaded world starts its boats afresh.
//! - **Riding the water.** A boat stands on the water surface at its hull: the water's Gerstner
//!   sum (a CPU port of `water_common.wesl`, damped as `wave_atten`) and the beach swash, on the
//!   water clock ([`WaterClock`]), heaved to the mean of four hull samples and pitched and rolled
//!   to their slope. Rivers are flat. The water also drops its surface inside each hull
//!   ([`WaterParams::hulls`]), so swell never shows through the floor.
//! - **Parts.** The oars and sails are part nodes (`hex_assets::part_of`) drawn at
//!   `placement * node * swing`: oars stroke with the boat's stroke phase (sweep ±35°, the blade
//!   buried 4.3° on the drive and lifted 6° on the recovery) or trail (80° aft, 5° up) while moored
//!   or passing a mooring; the sails swing to leeward by half the angle between the wind and the
//!   heading (main up to 70°, jib 45°), and the sailing boat heels a little to leeward.
//! - **Walker.** Each drawn boat blocks as `boat` by its collider footprint, the landing stage's
//!   deck is ground ([`WalkGround::pose_boats`]) and its bollards block as `stage`.
use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::{
    gltf::{Gltf, GltfMesh, GltfNode},
    prelude::*,
};
use hex_assets::part_of;
use hex_core::Axial;

pub mod plan;

use plan::{BoatCue, BoatKind, BoatPlan, Motion, TICK_SECONDS};

use crate::{
    ActiveScenario, AgentsPaused, Scenario, SceneEntity,
    boids::plan::cell_at,
    kit_material::{KitMaterial, build_material, slot_requests},
    play::{PlayCatalog, PlayKit, PlaySystems, PlayView, packs::PlayPacks, rise::PlayRises},
    scene::{Lod, lod_range},
    walk::WalkGround,
    water::{
        HULL_SLOTS, MAX_HULLS, WASH_AMP, WaterClock, WaterField, WaterMaterial, tiles::WATER_Y,
        tiles::water_kind,
    },
};

/// The landing stage piece.
pub const STAGE_PIECE: &str = "landing_stage";

/// Plan-view hull ellipses the water clips (half length, half beam, centre offset along the bow),
/// inside each hull's waterline planform (`hull_ellipses_lie_inside_the_hulls`), and the height
/// over the boat's origin below which the water is clipped (under the sheer).
pub fn hull_ellipse(kind: BoatKind) -> (f32, f32, f32, f32) {
    match kind {
        BoatKind::Rowing => (1.2, 0.42, -0.25, 0.40),
        BoatKind::Sailing => (1.85, 0.5, -0.35, 0.55),
    }
}

/// The boats of the view, re-derived when its water changes.
#[derive(Resource, Default)]
pub struct Boats {
    pub plan: BoatPlan,
    /// Each boat's pose before the latest tick (world XZ, heading), by index.
    prev: Vec<(Vec2, Vec2)>,
    key: Vec<(Axial, String, u8)>,
    epoch: u64,
    seed: u64,
}

impl Boats {
    /// `play.boats` in the state JSON: every boat's kind, state, cell and world position, and the
    /// landing stages.
    pub fn state_json(&self) -> serde_json::Value {
        let round = |v: f32| (v * 1000.0).round() / 1000.0;
        let list: Vec<serde_json::Value> = self
            .plan
            .boats
            .iter()
            .map(|b| {
                let (p, h) = self.plan.pose(b);
                let c = cell_at(p);
                serde_json::json!({
                    "id": b.id,
                    "kind": b.kind.name(),
                    "state": if b.moving() { "moving" } else { "moored" },
                    "cell": [c.q, c.r],
                    "pos": [round(p.x), round(p.y)],
                    "heading": [round(h.x), round(h.y)],
                })
            })
            .collect();
        let stages: Vec<serde_json::Value> = self
            .plan
            .stages
            .iter()
            .map(|s| {
                serde_json::json!({
                    "cell": [s.cell.q, s.cell.r],
                    "pos": [round(s.origin.x), round(s.origin.y)],
                })
            })
            .collect();
        let count = |k: BoatKind| self.plan.boats.iter().filter(|b| b.kind == k).count();
        serde_json::json!({
            "rowing": count(BoatKind::Rowing),
            "sailing": count(BoatKind::Sailing),
            "list": list,
            "stages": stages,
            "ticks": self.plan.ticks,
        })
    }
}

/// Sound cues asked for by the latest ticks, drained by `hex-app` into its event stream.
#[derive(Resource, Default)]
pub struct BoatCues(pub Vec<BoatCue>);

/// A drawn boat (its id) or landing stage (`u64::MAX - index`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoatEntity(pub u64);

/// Which animatable part a child draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    OarPort,
    OarStbd,
    Main,
    Jib,
}

impl PartKind {
    fn of(name: &str) -> Option<PartKind> {
        Some(match name {
            "oar_port" => PartKind::OarPort,
            "oar_stbd" => PartKind::OarStbd,
            "main" => PartKind::Main,
            "jib" => PartKind::Jib,
            _ => return None,
        })
    }
}

/// A part child: its node transform in the piece frame, swung each frame.
#[derive(Component, Clone, Copy, Debug)]
pub struct BoatPart {
    pub kind: PartKind,
    pub base: Transform,
}

/// One drawable primitive of a part node.
#[derive(Clone)]
struct PartMesh {
    kind: PartKind,
    lod: usize,
    base: Transform,
    mesh: Handle<Mesh>,
    material: Handle<KitMaterial>,
}

/// The boats' part meshes per piece, built once their pack is in.
#[derive(Resource, Default)]
pub struct BoatParts(HashMap<String, Vec<PartMesh>>);

/// The drawn sail swing per boat (main, jib in radians), eased toward the wanted swing.
#[derive(Resource, Default)]
pub struct SailSwing(HashMap<u64, (f32, f32)>);

pub struct BoatsPlugin;

impl Plugin for BoatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Boats>()
            .init_resource::<BoatCues>()
            .init_resource::<BoatParts>()
            .init_resource::<SailSwing>()
            .add_systems(FixedUpdate, tick)
            .add_systems(
                Update,
                (sync, build_parts, draw)
                    .chain()
                    .after(PlaySystems)
                    .after(crate::walk::sync),
            );
    }
}

/// The view's tiles that decide its boats: water tiles (bridges among them).
fn water_key(view: &PlayView, catalog: &PlayCatalog) -> Vec<(Axial, String, u8)> {
    view.tiles
        .iter()
        .filter(|t| {
            catalog
                .0
                .get(&t.tile)
                .is_some_and(|d| water_kind(d, t.rotation).is_some())
        })
        .map(|t| (t.cell, t.tile.clone(), t.rotation))
        .collect()
}

/// Leaves the boats and landing stages out of play: the thumbnail renderer (`--thumbs`) draws
/// each tile at rest, and a boat on a river tile's image would be an agent, not the tile.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NoBoats;

/// Re-derives the boats when the view's water, epoch or seed changes.
pub fn sync(
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    no_boats: Option<Res<NoBoats>>,
    mut boats: ResMut<Boats>,
) {
    if active.0 != Scenario::Play || no_boats.is_some() {
        if !boats.plan.boats.is_empty() || !boats.plan.stages.is_empty() {
            *boats = Boats::default();
        }
        return;
    }
    if !view.is_changed() && !active.is_changed() {
        return;
    }
    let key = water_key(&view, &catalog);
    if key == boats.key && boats.epoch == view.epoch && boats.seed == view.world_seed {
        return;
    }
    let plan = BoatPlan::new(&view.tiles, &catalog.0, view.world_seed);
    boats.prev = plan.poses().iter().map(|p| (p.2, p.3)).collect();
    boats.plan = plan;
    boats.key = key;
    boats.epoch = view.epoch;
    boats.seed = view.world_seed;
}

/// The fixed tick, while the play scenario runs and photo mode has not paused the agents.
pub fn tick(
    active: Res<ActiveScenario>,
    paused: Res<AgentsPaused>,
    mut boats: ResMut<Boats>,
    mut cues: ResMut<BoatCues>,
) {
    if active.0 != Scenario::Play || paused.0 || boats.plan.boats.is_empty() {
        return;
    }
    boats.prev = boats.plan.poses().iter().map(|p| (p.2, p.3)).collect();
    let out = boats.plan.tick();
    cues.0.extend(out);
    if cues.0.len() > 64 {
        let n = cues.0.len() - 64;
        cues.0.drain(..n);
    }
}

/// Builds each boat piece's part meshes once its GLB and body parts are in, reusing the body's
/// material per slot.
fn build_parts(
    kit: Option<Res<PlayKit>>,
    packs: Option<Res<PlayPacks>>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    mut parts: ResMut<BoatParts>,
    (server, mut materials, config): (
        Res<AssetServer>,
        ResMut<Assets<KitMaterial>>,
        Res<crate::workload::GateConfig>,
    ),
) {
    let (Some(kit), Some(packs)) = (kit, packs) else {
        return;
    };
    let PlayKit::Ready(pieces) = &*kit else {
        return;
    };
    for kind in [BoatKind::Rowing, BoatKind::Sailing] {
        let id = kind.piece();
        if parts.0.contains_key(id) {
            continue;
        }
        let (Some(body), Some(g)) = (pieces.get(id), packs.glb(id).and_then(|h| gltfs.get(h)))
        else {
            continue;
        };
        let Some(entry) = packs.manifest.pieces.iter().find(|e| e.id == id) else {
            continue;
        };
        // A slot only a part carries (the sails' cloth) gets its own material, as `play` builds them.
        let mut own: HashMap<String, Option<Handle<KitMaterial>>> = HashMap::new();
        let mut out = vec![];
        for lod in 0..3usize {
            for (name, h) in &g.named_nodes {
                let Some(kind) = part_of(id, lod, name).and_then(PartKind::of) else {
                    continue;
                };
                let Some(node) = gltf_nodes.get(h) else {
                    continue;
                };
                let Some(gm) = node.mesh.as_ref().and_then(|m| gltf_meshes.get(m)) else {
                    continue;
                };
                for prim in &gm.primitives {
                    let slot = prim
                        .material
                        .as_ref()
                        .and_then(|mh| {
                            g.named_materials
                                .iter()
                                .find(|(_, v)| *v == mh)
                                .map(|(k, _)| k.to_string())
                        })
                        .unwrap_or_default();
                    let material = match body.iter().find(|b| b.slot == slot) {
                        Some(b) => Some(b.material.clone()),
                        None => own
                            .entry(slot.clone())
                            .or_insert_with(|| {
                                let s = entry.slots.get(&slot)?;
                                match slot_requests(&packs.manifest, entry, s, packs.family) {
                                    Ok(r) => Some(materials.add(build_material(
                                        &server,
                                        s,
                                        &r,
                                        config.kit_hooks,
                                    ))),
                                    Err(e) => {
                                        warn!("boats: {e}, slot {slot} skipped");
                                        None
                                    }
                                }
                            })
                            .clone(),
                    };
                    if let Some(material) = material {
                        out.push(PartMesh {
                            kind,
                            lod,
                            base: node.transform,
                            mesh: prim.mesh.clone(),
                            material,
                        });
                    }
                }
            }
        }
        parts.0.insert(id.to_owned(), out);
    }
}

// --- The water surface (a CPU port of `water_common.wesl`) ---

fn hash11(n: f32) -> f32 {
    let x = (n * 127.1 + 31.7).sin() * 43758.547;
    x - x.floor()
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The Gerstner sum's height at rest point `p` and time `t` over `count` waves.
pub fn gerstner_height(p: Vec2, t: f32, count: u32) -> f32 {
    const WIND_ANGLE: f32 = 2.8;
    const WAVE_L0: f32 = 7.5;
    const WAVE_RATIO: f32 = 0.74;
    const WAVE_AMP_PER_M: f32 = 0.015;
    let mut y = 0.0;
    for i in 0..count {
        let fi = i as f32;
        let a = WIND_ANGLE + (hash11(fi + 1.0) - 0.5) * 1.4;
        let d = Vec2::new(a.cos(), a.sin());
        let l = WAVE_L0 * WAVE_RATIO.powf(fi);
        let k = std::f32::consts::TAU / l;
        let w = (9.81 * k).sqrt();
        let amp = WAVE_AMP_PER_M * l;
        let f = k * d.dot(p) - w * t + hash11(fi + 7.3) * std::f32::consts::TAU;
        y += amp * f.sin();
    }
    y
}

/// The beach swash (`water_common::swash`).
fn swash(shore: f32, t: f32) -> f32 {
    let a = 0.6 * (0.95 * t - 0.45 * shore).sin() + 0.4 * (0.61 * t - 0.3 * shore + 1.7).sin();
    a * 0.7 + 0.3
}

/// The drawn water surface's height at world XZ `p` (the water's rest level off the water).
pub fn surface_y(
    cells: &BTreeMap<Axial, crate::water::tiles::CellWater>,
    p: Vec2,
    t: f32,
    count: u32,
) -> f32 {
    let Some(cw) = cells.get(&cell_at(p)) else {
        return WATER_Y;
    };
    let s = cw.field().water(p.x, p.y);
    let atten = smoothstep(0.05, 2.5, s.depth) * (1.0 - s.river);
    let wash = (1.0 - smoothstep(0.3, 2.5, s.depth)) * (1.0 - s.river);
    WATER_Y + gerstner_height(p, t, count) * atten + WASH_AMP * swash(s.shore, t) * wash
}

/// A boat's transform riding the surface at `p` heading `h`: heaved to the mean of its bow,
/// stern and side samples and pitched and rolled to their slope, heeled by `heel` (radians,
/// + to starboard).
pub fn ride(
    kind: BoatKind,
    p: Vec2,
    h: Vec2,
    heel: f32,
    height: impl Fn(Vec2) -> f32,
) -> Transform {
    let (a, b) = match kind {
        BoatKind::Rowing => (1.5, 0.6),
        BoatKind::Sailing => (2.2, 0.8),
    };
    let side = Vec2::new(-h.y, h.x);
    let (bow, stern) = (height(p + h * a), height(p - h * a));
    let (stbd, port) = (height(p + side * b), height(p - side * b));
    let y = (bow + stern + stbd + port) / 4.0;
    let pitch = ((bow - stern) / (2.0 * a)).atan();
    let roll = -((stbd - port) / (2.0 * b)).atan() + heel;
    let yaw = h.y.atan2(h.x);
    Transform::from_xyz(p.x, y, p.y).with_rotation(
        Quat::from_rotation_y(-yaw) * Quat::from_rotation_z(pitch) * Quat::from_rotation_x(roll),
    )
}

/// The oars' (sweep, dip) in degrees for the port oar at stroke phase `phase` with `trail` (0
/// rowing, 1 trailed); the starboard oar mirrors both.
pub fn oar_angles(phase: f32, trail: f32) -> (f32, f32) {
    let a = std::f32::consts::TAU * phase;
    let s = a.sin();
    let sweep = -35.0 * a.cos();
    let dip = if s > 0.0 { -4.3 * s } else { -6.0 * s };
    (sweep + (80.0 - sweep) * trail, dip + (5.0 - dip) * trail)
}

/// The sails' wanted swing (main, jib) in radians for heading `h` with the boom on `side`.
pub fn sail_angles(h: Vec2, side: f32) -> (f32, f32) {
    let from = -plan::WIND_TO;
    let off = h.dot(from).clamp(-1.0, 1.0).acos() / 2.0;
    let main = off.clamp(10f32.to_radians(), 70f32.to_radians());
    let jib = off.clamp(8f32.to_radians(), 45f32.to_radians());
    (main * side, jib * side)
}

fn swing(kind: PartKind, oar: (f32, f32), sail: (f32, f32)) -> Quat {
    let (sweep, dip) = (oar.0.to_radians(), oar.1.to_radians());
    match kind {
        PartKind::OarPort => Quat::from_rotation_y(sweep) * Quat::from_rotation_x(dip),
        PartKind::OarStbd => Quat::from_rotation_y(-sweep) * Quat::from_rotation_x(-dip),
        PartKind::Main => Quat::from_rotation_y(sail.0),
        PartKind::Jib => Quat::from_rotation_y(sail.1),
    }
}

/// Spawns and despawns the boat and stage entities, poses them between the last two ticks on the
/// water surface, swings their parts, and hands their footprints to the walker and their hulls to
/// the water.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    paused: Res<AgentsPaused>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    boats: Res<Boats>,
    kit: Option<Res<PlayKit>>,
    parts: Res<BoatParts>,
    rises: Res<PlayRises>,
    clock: Option<Res<WaterClock>>,
    mut water: ResMut<Assets<WaterMaterial>>,
    mut swing_state: ResMut<SailSwing>,
    mut drawn: Query<(Entity, &BoatEntity, &mut Transform, &mut Visibility)>,
    mut part_q: Query<(&BoatPart, &ChildOf, &mut Transform), Without<BoatEntity>>,
    ground: Option<ResMut<WalkGround>>,
    mut held: Local<f32>,
) {
    let pieces = match kit.as_deref() {
        Some(PlayKit::Ready(p)) if active.0 == Scenario::Play => Some(p),
        _ => None,
    };
    if !paused.0 {
        *held = fixed.overstep_fraction();
    }
    let frac = *held;
    let t = clock.map_or(0.0, |c| c.t);
    let count = water.iter().next().map_or(8, |(_, m)| m.params.wave_count);
    let cells = &boats.plan.cells;
    // Wanted entities: boats by id, stages by u64::MAX - index.
    let mut want: HashMap<u64, (Transform, BoatKind, Option<(f32, f32, f32, f32)>, bool)> =
        HashMap::new();
    let mut hulls: Vec<(BoatKind, Transform)> = vec![];
    let mut posed: Vec<(&'static str, Transform)> = vec![];
    let mut stages: Vec<Transform> = vec![];
    if pieces.is_some() {
        for (i, s) in boats.plan.stages.iter().enumerate() {
            let tr = Transform::from_xyz(s.origin.x, WATER_Y, s.origin.y)
                .with_rotation(Quat::from_rotation_y(-s.yaw()));
            let shown = rises.settled_cell(s.cell);
            if shown {
                stages.push(tr);
            }
            want.insert(u64::MAX - i as u64, (tr, BoatKind::Rowing, None, shown));
        }
        let dt = time.delta_secs().min(0.1);
        for (i, b) in boats.plan.boats.iter().enumerate() {
            let (p1, h1) = boats.plan.pose(b);
            let (p0, h0) = boats.prev.get(i).copied().unwrap_or((p1, h1));
            let p = p0 + (p1 - p0) * frac;
            let h = (h0 + (h1 - h0) * frac).normalize_or(h1);
            let (oar, sail, heel) = match &b.motion {
                Motion::Rowing(r) => {
                    let phase = r.stroke + TICK_SECONDS * plan::ROW_SPEED / plan::STROKE_M * frac;
                    (oar_angles(phase - phase.floor(), r.trail), (0.0, 0.0), 0.0)
                }
                Motion::Moored { .. } => (oar_angles(0.0, 1.0), (0.0, 0.0), 0.0),
                Motion::Sailing(s) => {
                    let wanted = sail_angles(h, s.boom);
                    let cur = swing_state.0.entry(b.id).or_insert(wanted);
                    let step = 40f32.to_radians() * dt;
                    cur.0 += (wanted.0 - cur.0).clamp(-step, step);
                    cur.1 += (wanted.1 - cur.1).clamp(-step, step);
                    ((0.0, 0.0), *cur, 0.1 * s.boom)
                }
                Motion::Anchored { heading, .. } => ((0.0, 0.0), sail_angles(*heading, 1.0), 0.0),
            };
            let tr = ride(b.kind, p, h, heel, |q| surface_y(cells, q, t, count));
            let shown = rises.settled_cell(cell_at(p));
            if shown {
                hulls.push((b.kind, tr));
                posed.push((b.kind.piece(), tr));
            }
            want.insert(
                b.id,
                (tr, b.kind, Some((oar.0, oar.1, sail.0, sail.1)), shown),
            );
        }
    }
    // Despawn the unwanted; move the rest.
    let mut have = HashSet::new();
    let mut swings: HashMap<Entity, (BoatKind, (f32, f32, f32, f32))> = HashMap::new();
    for (e, be, mut tr, mut vis) in &mut drawn {
        match want.get(&be.0) {
            Some((t, kind, angles, shown)) if have.insert(be.0) => {
                if *tr != *t {
                    *tr = *t;
                }
                let v = if *shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *vis != v {
                    *vis = v;
                }
                if let Some(a) = angles {
                    swings.insert(e, (*kind, *a));
                }
            }
            _ => commands.entity(e).try_despawn(),
        }
    }
    for (part, parent, mut tr) in &mut part_q {
        if let Some((_, a)) = swings.get(&parent.parent()) {
            let q = part.base.rotation * swing(part.kind, (a.0, a.1), (a.2, a.3));
            let next = Transform {
                rotation: q,
                ..part.base
            };
            if *tr != next {
                *tr = next;
            }
        }
    }
    if let Some(pieces) = pieces {
        for (id, (tr, kind, angles, shown)) in &want {
            if have.contains(id) {
                continue;
            }
            let piece = if angles.is_some() {
                kind.piece()
            } else {
                STAGE_PIECE
            };
            let Some(body) = pieces.get(piece) else {
                continue;
            };
            let part_meshes = parts.0.get(piece).cloned().unwrap_or_default();
            if angles.is_some() && part_meshes.is_empty() {
                continue;
            }
            commands
                .spawn((
                    BoatEntity(*id),
                    *tr,
                    if *shown {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    SceneEntity,
                ))
                .with_children(|c| {
                    for part in body {
                        c.spawn((
                            Mesh3d(part.mesh.clone()),
                            MeshMaterial3d(part.material.clone()),
                            Transform::default(),
                            lod_range(part.lod),
                            Lod(part.lod as u8),
                        ));
                    }
                    for pm in &part_meshes {
                        c.spawn((
                            BoatPart {
                                kind: pm.kind,
                                base: pm.base,
                            },
                            Mesh3d(pm.mesh.clone()),
                            MeshMaterial3d(pm.material.clone()),
                            pm.base,
                            lod_range(pm.lod),
                            Lod(pm.lod as u8),
                        ));
                    }
                });
        }
    }
    // The water's hull clip.
    let mut packed = [Vec4::ZERO; HULL_SLOTS];
    let n = hulls.len().min(MAX_HULLS);
    for (i, (kind, tr)) in hulls.iter().take(MAX_HULLS).enumerate() {
        let (hl, hb, off, top) = hull_ellipse(*kind);
        let fwd = tr.rotation * Vec3::X;
        let h = Vec2::new(fwd.x, fwd.z).normalize_or(Vec2::X);
        packed[2 * i] = Vec4::new(tr.translation.x, tr.translation.z, h.x, h.y);
        packed[2 * i + 1] = Vec4::new(hl, hb, tr.translation.y + top, off);
    }
    let stale: Vec<_> = water
        .iter()
        .filter(|(_, m)| m.params.boat_count != n as u32 || m.params.hulls != packed)
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut m) = water.get_mut(id) {
            m.params.hulls = packed;
            m.params.boat_count = n as u32;
        }
    }
    if let Some(mut ground) = ground {
        ground.bypass_change_detection().pose_boats(&posed, &stages);
    }
}

#[cfg(test)]
mod tests;
