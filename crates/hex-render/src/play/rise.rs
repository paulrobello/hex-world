//! The staged rise of a placed tile (PRD §4.4), driven by its `TileDef.rise` timeline:
//!
//! 1. Column: the tile root pushes up from below the ground plane, with a dust puff and a camera
//!    micro-shake. Water beds follow the column, so the cut-away reads during the rise.
//! 2. Cap: the column overshoots its rest height and settles.
//! 3. Edges: centre and edge pieces snap in, and water surfaces and cut faces fill from the tile
//!    base up to the water level.
//! 4. Props: hero props grow in a staggered order, trees by scale and buildings floor by floor.
//! 5. Agents: from this stage on [`PlayRises::agents_ready`] lets `hex-app` mirror the tile into the
//!    car simulation.
//!
//! Every stage start is announced as a [`RiseMessage`] (the audio layer's rumble cue). A new
//! placement fast-forwards every running rise to its end state, and a removed tile sinks for
//! [`SINK_SECONDS`]. Under [`ReducedMotion`] a rise is an instant cut with no dust or shake.
use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS, LEVEL_STEP, RiseStage, RiseTimeline};

use crate::{ActiveScenario, Scenario, SceneEntity};

/// Metres below rest at which a rising column starts: the cap starts under the ground plane.
pub const RISE_DEPTH: f32 = LEVEL_STEP + 0.6;
/// Peak height of the cap's overshoot above rest (m).
pub const OVERSHOOT: f32 = 0.35;
/// Undo and bulldoze sink a tile this long (s), ease-in (`design/ui/legend/in-world.md`, Place).
pub const SINK_SECONDS: f32 = 0.4;
/// Tile base (m), where water starts to fill from.
const FLOOR_Y: f32 = -LEVEL_STEP;
/// Peak camera shake as a fraction of the camera's distance to the rising tile (~3 px at 1280 px).
const SHAKE: f32 = 0.0016;

/// Reduced motion (`prefers-reduced-motion` or the in-game override), set by the overlay through
/// the `set_reduced_motion` command: rises and affordance transitions become instant cuts.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReducedMotion(pub bool);

/// What a [`RiseMessage`] announces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiseCue {
    /// A stage starts.
    Stage(RiseStage),
    /// The rise reached its end state.
    Settled,
    /// The tile left the world (undo or bulldoze) and sinks.
    Sink,
}

impl RiseCue {
    pub fn name(self) -> &'static str {
        match self {
            RiseCue::Stage(s) => stage_name(s),
            RiseCue::Settled => "settled",
            RiseCue::Sink => "sink",
        }
    }
}

pub fn stage_name(s: RiseStage) -> &'static str {
    match s {
        RiseStage::Column => "column",
        RiseStage::Cap => "cap",
        RiseStage::Edges => "edges",
        RiseStage::Props => "props",
        RiseStage::Agents => "agents",
    }
}

/// One rise cue for the audio layer and the HUD. `skipped` marks cues emitted by a fast-forward or
/// under reduced motion, all in one frame, so audio can drop the stacked rumble.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct RiseMessage {
    pub cell: Axial,
    pub tile: String,
    pub cue: RiseCue,
    pub skipped: bool,
}

/// One running rise.
#[derive(Clone, Debug, PartialEq)]
pub struct Rise {
    pub tile: String,
    pub timeline: RiseTimeline,
    /// Seconds since the rise started.
    pub t: f32,
    /// Cues announced so far, in [`Rise::cues`] order.
    announced: usize,
}

/// Where a rising tile is: what every animated part reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RisePose {
    /// Vertical offset of the column (root, beds) from rest (m).
    pub column_y: f32,
    /// Edges stage progress 0..1 (`None` before it starts).
    pub edges: Option<f32>,
    /// Water fill 0..1 (0 before the edges stage).
    pub fill: f32,
    /// Props stage progress 0..1 (`None` before it starts).
    pub props: Option<f32>,
    pub agents: bool,
}

impl RisePose {
    pub const REST: RisePose = RisePose {
        column_y: 0.0,
        edges: Some(1.0),
        fill: 1.0,
        props: Some(1.0),
        agents: true,
    };
}

impl Rise {
    pub fn new(tile: &str, timeline: &RiseTimeline) -> Self {
        Self {
            tile: tile.to_owned(),
            timeline: timeline.clone(),
            t: 0.0,
            announced: 0,
        }
    }

    fn window(&self, stage: RiseStage) -> Option<(f32, f32)> {
        self.timeline
            .steps
            .iter()
            .find(|s| s.stage == stage)
            .map(|s| (s.start, s.duration))
    }

    /// Progress through `stage` at `t`: `None` before it starts, 1 after it ends or when the
    /// timeline has no such step.
    fn progress(&self, stage: RiseStage, t: f32) -> Option<f32> {
        let Some((start, dur)) = self.window(stage) else {
            return Some(1.0);
        };
        if t < start {
            return None;
        }
        Some(if dur <= 0.0 {
            1.0
        } else {
            ((t - start) / dur).clamp(0.0, 1.0)
        })
    }

    pub fn total(&self) -> f32 {
        self.timeline.total()
    }

    pub fn done(&self) -> bool {
        self.t >= self.total()
    }

    /// The stage the rise is in (the latest one started).
    pub fn stage(&self) -> RiseStage {
        self.timeline
            .steps
            .iter()
            .filter(|s| self.t >= s.start)
            .map(|s| s.stage)
            .max()
            .unwrap_or(RiseStage::Column)
    }

    /// Stage starts then the end, with the time each falls due.
    fn cues(&self) -> Vec<(f32, RiseCue)> {
        let mut out: Vec<(f32, RiseCue)> = self
            .timeline
            .steps
            .iter()
            .map(|s| (s.start, RiseCue::Stage(s.stage)))
            .collect();
        out.push((self.total(), RiseCue::Settled));
        out
    }

    /// Cues that fell due by `self.t` and were not announced yet.
    fn due(&mut self) -> Vec<RiseCue> {
        let cues = self.cues();
        let due: Vec<RiseCue> = cues
            .iter()
            .skip(self.announced)
            .take_while(|(at, _)| *at <= self.t)
            .map(|(_, c)| *c)
            .collect();
        self.announced += due.len();
        due
    }

    pub fn pose(&self) -> RisePose {
        let t = self.t;
        let col = self.progress(RiseStage::Column, t).unwrap_or(0.0);
        let cap = self.progress(RiseStage::Cap, t).unwrap_or(0.0);
        let edges = self.progress(RiseStage::Edges, t);
        RisePose {
            column_y: -RISE_DEPTH * (1.0 - ease_out_cubic(col)) + OVERSHOOT * cap_bump(cap),
            edges,
            fill: edges.map_or(0.0, smoothstep01),
            props: self.progress(RiseStage::Props, t),
            agents: self.progress(RiseStage::Agents, t).is_some(),
        }
    }

    /// Column-stage shake weight 1..0 (0 outside the column stage).
    fn shake(&self) -> f32 {
        match self.progress(RiseStage::Column, self.t) {
            Some(u) if u < 1.0 => (1.0 - u) * (1.0 - u),
            _ => 0.0,
        }
    }
}

pub fn ease_out_cubic(u: f32) -> f32 {
    1.0 - (1.0 - u).powi(3)
}

pub fn ease_out_back(u: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2)
}

fn smoothstep01(u: f32) -> f32 {
    u * u * (3.0 - 2.0 * u)
}

/// The cap's settle: up to 1 (scaled to [`OVERSHOOT`]) early in the stage, a slight dip below rest
/// late in it, and exactly 0 at both ends.
pub fn cap_bump(u: f32) -> f32 {
    if u <= 0.0 || u >= 1.0 {
        return 0.0;
    }
    // sin(1.5πu)(1-u)² peaks at ~0.522 near u = 0.22.
    (1.5 * std::f32::consts::PI * u).sin() * (1.0 - u).powi(2) / 0.522
}

/// A prop's progress within the props stage: prop `i` of `n` starts later the higher its index.
pub fn prop_progress(props: f32, i: u8, n: u8) -> Option<f32> {
    let n = n.max(1) as f32;
    let lead = if n > 1.0 {
        0.45 * i as f32 / (n - 1.0)
    } else {
        0.0
    };
    let u = (props - lead) / 0.55;
    (u > 0.0).then(|| u.min(1.0))
}

/// A building's height fraction at progress `u`: one eased step per floor.
pub fn floors_up(u: f32, floors: u8) -> f32 {
    let k = floors.max(1) as f32;
    let x = (u * k).min(k);
    let whole = x.floor().min(k - 1.0);
    (whole + ease_out_back((x - whole).min(1.0))) / k
}

/// How a part of a placed tile moves during its rise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RiseRole {
    /// The tile root: the column and cap, and every child that has no role of its own.
    Root,
    /// A centre or edge piece (roads, lanes, pads): snaps in at the edges stage.
    Edge,
    /// A hero prop: grows at the props stage, `order` of `count`.
    Prop {
        order: u8,
        count: u8,
        kind: PropKind,
    },
    /// A water tile's placeholder bed: moves with the column.
    Bed,
    /// A water surface or cut face: moves with the column and fills at the edges stage.
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropKind {
    Tree,
    Building { floors: u8 },
}

/// A tile part the rise animates, with its rest transform.
#[derive(Component, Clone, Copy, Debug)]
pub struct RisePart {
    pub cell: Axial,
    pub role: RiseRole,
    pub rest: Transform,
}

impl RisePart {
    pub fn new(cell: Axial, role: RiseRole, rest: Transform) -> Self {
        Self { cell, role, rest }
    }

    /// The transform and visibility of this part at `pose`.
    pub fn at(&self, pose: &RisePose) -> (Transform, bool) {
        let mut t = self.rest;
        match self.role {
            RiseRole::Root | RiseRole::Bed => {
                t.translation.y += pose.column_y;
                (t, true)
            }
            RiseRole::Edge => match pose.edges {
                None => (t, false),
                Some(u) => {
                    let s = 0.5 + 0.5 * ease_out_back(u);
                    t.scale = self.rest.scale * Vec3::new(s, s.min(1.0), s).max(Vec3::splat(1e-3));
                    t.translation.y += 0.9 * (1.0 - ease_out_cubic(u));
                    (t, true)
                }
            },
            RiseRole::Prop { order, count, kind } => {
                let Some(u) = pose.props.and_then(|p| prop_progress(p, order, count)) else {
                    return (t, false);
                };
                let k = match kind {
                    PropKind::Tree => Vec3::splat(ease_out_back(u)),
                    PropKind::Building { floors } => Vec3::new(1.0, floors_up(u, floors), 1.0),
                };
                t.scale = self.rest.scale * k.max(Vec3::splat(1e-3));
                (t, true)
            }
            RiseRole::Water => {
                if pose.fill <= 1e-3 {
                    return (t, false);
                }
                let s = pose.fill;
                t.scale.y = self.rest.scale.y * s;
                t.translation.y = self.rest.translation.y + pose.column_y + FLOOR_Y * (1.0 - s);
                (t, true)
            }
        }
    }
}

/// A removed tile on its way down: its root and its released water entities.
struct Sink {
    cell: Axial,
    entities: Vec<Entity>,
    /// Rest heights captured on the first frame.
    bases: Vec<(Entity, f32)>,
    t: f32,
}

/// Rise state of every drawn tile. A cell missing from it is not drawn yet; `None` is settled.
#[derive(Resource, Default)]
pub struct PlayRises {
    cells: HashMap<Axial, Option<Rise>>,
    /// Cells that settled (or fast-forwarded) since the last animate pass: put at rest once more.
    settled: HashSet<Axial>,
    sinks: Vec<Sink>,
}

impl PlayRises {
    /// Whether the tile at `cell` has reached its agents stage (drawn and past the stage start, or
    /// settled). False for a cell not drawn yet.
    pub fn agents_ready(&self, cell: Axial) -> bool {
        match self.cells.get(&cell) {
            None => false,
            Some(None) => true,
            Some(Some(r)) => r.pose().agents,
        }
    }

    /// Running rises, by cell.
    pub fn rising(&self) -> impl Iterator<Item = (Axial, &Rise)> {
        self.cells
            .iter()
            .filter_map(|(c, r)| r.as_ref().map(|r| (*c, r)))
    }

    pub fn is_animating(&self) -> bool {
        self.cells.values().any(Option::is_some) || !self.settled.is_empty()
    }

    /// Every drawn cell at rest (a rebuild, a scenario switch); sinking tiles go at once.
    pub(crate) fn reset(
        &mut self,
        cells: impl IntoIterator<Item = Axial>,
        commands: &mut Commands,
    ) {
        self.cells = cells.into_iter().map(|c| (c, None)).collect();
        self.settled.clear();
        for s in self.sinks.drain(..) {
            for e in s.entities {
                commands.entity(e).try_despawn();
            }
        }
    }

    /// Jumps every running rise to its end state, announcing the cues it skipped.
    pub(crate) fn fast_forward(&mut self, out: &mut Vec<RiseMessage>) {
        for (cell, slot) in self.cells.iter_mut() {
            if let Some(mut r) = slot.take() {
                r.t = r.total();
                for cue in r.due() {
                    out.push(RiseMessage {
                        cell: *cell,
                        tile: r.tile.clone(),
                        cue,
                        skipped: true,
                    });
                }
                self.settled.insert(*cell);
            }
        }
    }

    /// Starts the rise of a tile drawn this frame; under reduced motion it is at rest at once.
    pub(crate) fn begin(
        &mut self,
        cell: Axial,
        tile: &str,
        timeline: &RiseTimeline,
        reduced: bool,
        out: &mut Vec<RiseMessage>,
    ) {
        let mut r = Rise::new(tile, timeline);
        if reduced {
            r.t = r.total();
            for cue in r.due() {
                out.push(RiseMessage {
                    cell,
                    tile: tile.to_owned(),
                    cue,
                    skipped: true,
                });
            }
            self.cells.insert(cell, None);
            self.settled.insert(cell);
        } else {
            self.cells.insert(cell, Some(r));
        }
    }

    /// A tile drawn at rest (no rise), e.g. a rebuilt tile.
    pub(crate) fn rest(&mut self, cell: Axial) {
        self.cells.insert(cell, None);
    }

    /// A placed tile that is not drawn yet (hidden until its pack arrives).
    pub(crate) fn undrawn(&mut self, cell: Axial) {
        self.cells.remove(&cell);
        self.settled.remove(&cell);
    }

    /// The tile at `cell` left the view: its root and water entities sink, then despawn.
    pub(crate) fn remove(
        &mut self,
        cell: Axial,
        tile: &str,
        entities: Vec<Entity>,
        reduced: bool,
        commands: &mut Commands,
        out: &mut Vec<RiseMessage>,
    ) {
        self.cells.remove(&cell);
        self.settled.remove(&cell);
        self.cancel_sink(cell, commands);
        out.push(RiseMessage {
            cell,
            tile: tile.to_owned(),
            cue: RiseCue::Sink,
            skipped: reduced,
        });
        if reduced {
            for e in entities {
                commands.entity(e).try_despawn();
            }
        } else {
            self.sinks.push(Sink {
                cell,
                entities,
                bases: vec![],
                t: 0.0,
            });
        }
    }

    /// A tile placed again where one is still sinking replaces it at once.
    pub(crate) fn cancel_sink(&mut self, cell: Axial, commands: &mut Commands) {
        self.sinks.retain(|s| {
            if s.cell != cell {
                return true;
            }
            for e in &s.entities {
                commands.entity(*e).try_despawn();
            }
            false
        });
    }

    /// `(cell, tile, stage, seconds)` of each running rise, sorted by cell, for the state JSON.
    pub fn report(&self) -> Vec<(Axial, String, &'static str, f32)> {
        let mut out: Vec<_> = self
            .rising()
            .map(|(c, r)| (c, r.tile.clone(), stage_name(r.stage()), r.t))
            .collect();
        out.sort_by_key(|(c, ..)| (c.q, c.r));
        out
    }
}

/// Advances every running rise by the frame time and announces the cues that fell due.
pub fn advance(
    time: Res<Time>,
    active: Res<ActiveScenario>,
    mut rises: ResMut<PlayRises>,
    mut out: MessageWriter<RiseMessage>,
) {
    if active.0 != Scenario::Play || !rises.bypass_change_detection().is_animating() {
        return;
    }
    let dt = time.delta_secs();
    let rises = rises.as_mut();
    for (cell, slot) in rises.cells.iter_mut() {
        let Some(r) = slot.as_mut() else {
            continue;
        };
        r.t += dt;
        for cue in r.due() {
            out.write(RiseMessage {
                cell: *cell,
                tile: r.tile.clone(),
                cue,
                skipped: false,
            });
        }
        if r.done() {
            *slot = None;
            rises.settled.insert(*cell);
        }
    }
}

/// Poses every part of every rising tile; parts of a tile that just settled go to rest.
pub fn animate(
    mut rises: ResMut<PlayRises>,
    mut parts: Query<(&RisePart, &mut Transform, &mut Visibility)>,
) {
    if !rises.bypass_change_detection().is_animating() {
        return;
    }
    let poses: HashMap<Axial, RisePose> = rises
        .rising()
        .map(|(c, r)| (c, r.pose()))
        .chain(rises.settled.iter().map(|c| (*c, RisePose::REST)))
        .collect();
    for (part, mut t, mut vis) in &mut parts {
        let Some(pose) = poses.get(&part.cell) else {
            continue;
        };
        let (pose_t, shown) = part.at(pose);
        if *t != pose_t {
            *t = pose_t;
        }
        let want = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
    rises.settled.clear();
}

/// Sinks removed tiles over [`SINK_SECONDS`] (ease-in), then despawns them.
pub fn sink(
    time: Res<Time>,
    mut commands: Commands,
    mut rises: ResMut<PlayRises>,
    mut transforms: Query<&mut Transform>,
) {
    if rises.bypass_change_detection().sinks.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    rises.sinks.retain_mut(|s| {
        if s.bases.is_empty() {
            s.bases = s
                .entities
                .iter()
                .filter_map(|e| transforms.get(*e).ok().map(|t| (*e, t.translation.y)))
                .collect();
        }
        s.t += dt;
        let u = (s.t / SINK_SECONDS).min(1.0);
        if u >= 1.0 {
            for e in &s.entities {
                commands.entity(*e).try_despawn();
            }
            return false;
        }
        for (e, y) in &s.bases {
            if let Ok(mut t) = transforms.get_mut(*e) {
                t.translation.y = y - RISE_DEPTH * u * u;
            }
        }
        true
    });
}

/// One dust puff: shared mesh, its own fading material, CPU-moved particles.
struct Puff {
    entities: Vec<Entity>,
    material: Handle<StandardMaterial>,
    particles: Vec<Particle>,
    t: f32,
    alpha: f32,
}

#[derive(Clone, Copy)]
struct Particle {
    origin: Vec3,
    vel: Vec3,
    size: (f32, f32),
    life: f32,
}

/// Live dust puffs and the shared particle mesh.
#[derive(Resource, Default)]
pub struct PlayDust {
    puffs: Vec<Puff>,
    mesh: Option<Handle<Mesh>>,
}

fn hash01(cell: Axial, i: u32, salt: u32) -> f32 {
    let mut h = (cell.q as u32).wrapping_mul(0x8da6_b343)
        ^ (cell.r as u32).wrapping_mul(0xd816_3841)
        ^ i.wrapping_mul(0xcb1a_b31f)
        ^ salt.wrapping_mul(0x9e37_79b9);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Dust particles for a puff at `cell`: `n` along the hex rim, thrown outward and up.
fn particles(cell: Axial, n: u32, strength: f32) -> Vec<Particle> {
    let [cx, cz] = cell.to_world(HEX_RADIUS);
    (0..n)
        .map(|i| {
            let a = (i as f32 + hash01(cell, i, 1)) / n as f32 * std::f32::consts::TAU;
            let dir = Vec3::new(a.cos(), 0.0, a.sin());
            let r = HEX_RADIUS * (0.86 + 0.12 * hash01(cell, i, 2));
            Particle {
                origin: Vec3::new(cx, 0.15, cz) + dir * r,
                vel: (dir * (2.5 + 3.0 * hash01(cell, i, 3))
                    + Vec3::Y * (1.2 + 2.2 * hash01(cell, i, 4)))
                    * strength,
                size: (
                    0.3 + 0.3 * hash01(cell, i, 5),
                    1.2 + 1.2 * hash01(cell, i, 6),
                ),
                life: 0.9 + 0.5 * hash01(cell, i, 7),
            }
        })
        .collect()
}

/// Spawns a puff on each column start and sink, and moves and fades the live ones.
#[allow(clippy::too_many_arguments)]
pub fn dust(
    time: Res<Time>,
    mut commands: Commands,
    reduced: Res<ReducedMotion>,
    mut cues: MessageReader<RiseMessage>,
    mut dust: ResMut<PlayDust>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform>,
) {
    for m in cues.read() {
        let (n, strength, alpha) = match m.cue {
            RiseCue::Stage(RiseStage::Column) if !m.skipped => (48, 1.0, 0.32),
            RiseCue::Sink if !m.skipped => (24, 0.6, 0.26),
            _ => continue,
        };
        if reduced.0 {
            continue;
        }
        let mesh = dust
            .mesh
            .get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().ico(1).expect("ico 1")))
            .clone();
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.66, 0.58, 0.46, alpha),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let ps = particles(m.cell, n, strength);
        let entities = ps
            .iter()
            .map(|p| {
                commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(p.origin).with_scale(Vec3::splat(p.size.0)),
                        bevy::light::NotShadowCaster,
                        SceneEntity,
                    ))
                    .id()
            })
            .collect();
        dust.puffs.push(Puff {
            entities,
            material,
            particles: ps,
            t: 0.0,
            alpha,
        });
    }
    if dust.puffs.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    dust.puffs.retain_mut(|puff| {
        puff.t += dt;
        let t = puff.t;
        let mut alive = false;
        for (e, p) in puff.entities.iter().zip(&puff.particles) {
            let u = (t / p.life).min(1.0);
            alive |= u < 1.0;
            if let Ok(mut tr) = transforms.get_mut(*e) {
                // Drag 3/s on the throw, a slow lift as the dust spreads.
                let drift = p.vel * (1.0 - (-3.0 * t).exp()) / 3.0 + Vec3::Y * 0.4 * t;
                tr.translation = p.origin + drift;
                tr.scale = Vec3::splat(p.size.0 + (p.size.1 - p.size.0) * ease_out_cubic(u));
            }
        }
        if let Some(mut m) = materials.get_mut(&puff.material) {
            let u = (t / 1.2).min(1.0);
            m.base_color.set_alpha(puff.alpha * (1.0 - u) * (1.0 - u));
        }
        if !alive {
            for e in &puff.entities {
                commands.entity(*e).try_despawn();
            }
        }
        alive
    });
}

/// The camera offset the shake applied this frame, removed before the next frame's systems run.
#[derive(Resource, Default)]
pub struct CameraShake {
    applied: Option<(Entity, Vec3)>,
}

/// First: takes last frame's shake off the camera, so input, picking, scripted camera moves and
/// state dumps all see the unshaken transform.
pub fn unshake(mut shake: ResMut<CameraShake>, mut cams: Query<&mut Transform, With<Camera3d>>) {
    if let Some((e, off)) = shake.applied.take()
        && let Ok(mut t) = cams.get_mut(e)
    {
        t.translation -= off;
    }
}

/// PostUpdate, before transform propagation: a small decaying jitter of the camera while any
/// column is pushing up. Off under reduced motion.
pub fn shake(
    time: Res<Time>,
    active: Res<ActiveScenario>,
    reduced: Res<ReducedMotion>,
    rises: Res<PlayRises>,
    mut shake: ResMut<CameraShake>,
    mut cams: Query<(Entity, &mut Transform), With<Camera3d>>,
) {
    if active.0 != Scenario::Play || reduced.0 {
        return;
    }
    let Some((e, mut t)) = cams.iter_mut().next() else {
        return;
    };
    let mut amp = 0.0f32;
    for (cell, r) in rises.rising() {
        let w = r.shake();
        if w > 0.0 {
            let [x, z] = cell.to_world(HEX_RADIUS);
            amp = amp.max(w * t.translation.distance(Vec3::new(x, 0.0, z)) * SHAKE);
        }
    }
    if amp <= 0.0 {
        return;
    }
    let s = time.elapsed_secs();
    let off = (t.right() * (s * 113.0).sin() + t.up() * (s * 89.0 + 1.3).sin() * 0.7) * amp;
    t.translation += off;
    shake.applied = Some((e, off));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rise() -> Rise {
        Rise::new("meadow", &RiseTimeline::default())
    }

    #[test]
    fn column_starts_below_ground_overshoots_and_rests() {
        let mut r = rise();
        let start = r.pose();
        assert!((start.column_y + RISE_DEPTH).abs() < 1e-5);
        assert_eq!(start.edges, None);
        assert_eq!(start.fill, 0.0);
        assert!(!start.agents);
        let mut peak = f32::MIN;
        let mut prev = start.column_y;
        while r.t < 0.85 {
            r.t += 1.0 / 60.0;
            let y = r.pose().column_y;
            peak = peak.max(y);
            if r.t <= 0.45 {
                assert!(y >= prev - 1e-5, "column keeps rising in the push");
            }
            prev = y;
        }
        assert!(
            peak > 0.6 * OVERSHOOT && peak < 1.05 * OVERSHOOT,
            "peak {peak}"
        );
        r.t = r.total();
        assert_eq!(r.pose(), RisePose::REST);
    }

    #[test]
    fn cues_fall_due_in_stage_order_once() {
        let mut r = rise();
        let mut seen = vec![];
        while !r.done() {
            seen.extend(r.due());
            r.t += 1.0 / 60.0;
        }
        seen.extend(r.due());
        seen.extend(r.due());
        let names: Vec<_> = seen.iter().map(|c| c.name()).collect();
        assert_eq!(
            names,
            ["column", "cap", "edges", "props", "agents", "settled"]
        );
    }

    #[test]
    fn agents_gate_opens_at_the_agents_stage() {
        let mut rises = PlayRises::default();
        let c = Axial::new(1, 0);
        assert!(!rises.agents_ready(c), "not drawn yet");
        let mut out = vec![];
        rises.begin(
            c,
            "road_straight",
            &RiseTimeline::default(),
            false,
            &mut out,
        );
        assert!(out.is_empty());
        assert!(!rises.agents_ready(c));
        rises.cells.get_mut(&c).unwrap().as_mut().unwrap().t = 1.59;
        assert!(!rises.agents_ready(c));
        rises.cells.get_mut(&c).unwrap().as_mut().unwrap().t = 1.6;
        assert!(rises.agents_ready(c));
    }

    #[test]
    fn fast_forward_settles_and_announces_skipped_cues() {
        let mut rises = PlayRises::default();
        let (a, b) = (Axial::new(1, 0), Axial::new(2, 0));
        let mut out = vec![];
        rises.begin(a, "forest", &RiseTimeline::default(), false, &mut out);
        let r = rises.cells.get_mut(&a).unwrap().as_mut().unwrap();
        r.t = 0.5;
        let early = r.due();
        assert_eq!(early.len(), 2, "column and cap");
        rises.fast_forward(&mut out);
        rises.begin(b, "forest", &RiseTimeline::default(), false, &mut out);
        let names: Vec<_> = out.iter().map(|m| (m.cue.name(), m.skipped)).collect();
        assert_eq!(
            names,
            [
                ("edges", true),
                ("props", true),
                ("agents", true),
                ("settled", true)
            ]
        );
        assert!(rises.agents_ready(a) && !rises.agents_ready(b));
        assert!(rises.settled.contains(&a));
    }

    #[test]
    fn reduced_motion_is_an_instant_cut() {
        let mut rises = PlayRises::default();
        let mut out = vec![];
        let c = Axial::new(0, 1);
        rises.begin(c, "meadow", &RiseTimeline::default(), true, &mut out);
        assert!(rises.agents_ready(c));
        assert_eq!(out.len(), 6);
        assert!(out.iter().all(|m| m.skipped));
    }

    #[test]
    fn water_fills_from_the_tile_base() {
        let part = RisePart::new(Axial::new(0, 0), RiseRole::Water, Transform::IDENTITY);
        let mut r = rise();
        r.t = 0.7;
        assert!(!part.at(&r.pose()).1, "hidden before the edges stage");
        r.t = 0.8 + 0.2;
        let (t, shown) = part.at(&r.pose());
        assert!(shown);
        // A vertex at the water level (-1.4 m) sits between the base and the level mid-fill.
        let y = t.transform_point(Vec3::new(0.0, -1.4, 0.0)).y;
        assert!(y > FLOOR_Y && y < -1.4, "{y}");
        let (t, _) = part.at(&RisePose::REST);
        assert_eq!(t, Transform::IDENTITY);
    }

    #[test]
    fn props_stagger_and_buildings_rise_by_floor() {
        assert_eq!(prop_progress(0.0, 0, 3), None);
        let first = prop_progress(0.3, 0, 3).unwrap();
        assert!(prop_progress(0.3, 2, 3).is_none() && first > 0.0);
        assert_eq!(prop_progress(1.0, 2, 3), Some(1.0));
        assert!((floors_up(0.5, 2) - 0.5).abs() < 1e-5, "first floor up");
        assert!((floors_up(1.0, 3) - 1.0).abs() < 1e-5);
        assert!(floors_up(0.2, 3) < 1.0 / 3.0 + 0.05);
    }
}
