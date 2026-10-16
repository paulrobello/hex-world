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
//! placement fast-forwards every running rise to its end state, and a removed tile sinks: for
//! [`SINK_SECONDS`] with a small puff after an undo, for [`BULLDOZE_SINK_SECONDS`] inside a dust
//! cloud after a bulldoze. Under [`ReducedMotion`] a rise is an instant cut with no dust or shake.
use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS, LEVEL_STEP, RiseStage, RiseTimeline};

use crate::{ActiveScenario, Scenario, SceneEntity};

/// Metres below rest at which a rising column starts: the cap starts under the ground plane.
pub const RISE_DEPTH: f32 = LEVEL_STEP + 0.6;
/// Peak height of the cap's overshoot above rest (m).
pub const OVERSHOOT: f32 = 0.35;
/// Undo and redo sink a tile this long (s), ease-in (`design/ui/legend/in-world.md`, Place).
pub const SINK_SECONDS: f32 = 0.4;
/// A bulldozed tile sinks this long (s), ease-in, inside its dust cloud (`in-world.md`, Bulldoze).
pub const BULLDOZE_SINK_SECONDS: f32 = 0.7;
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
    /// The tile left the world and sinks. Both kinds are `sink` on the event wire.
    Sink(SinkKind),
}

/// Why a tile left the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkKind {
    /// An undo or redo stepped the history: a short sink with a small puff.
    Undo,
    /// The bulldoze tool: a longer sink inside a dust cloud.
    Bulldoze,
    /// A replace swapped the tile out (or its undo or redo swapped it back): a short sink with no
    /// dust, and the new tile rises once it is down.
    Replace,
}

impl SinkKind {
    /// How long the tile takes to sink (s).
    pub fn seconds(self) -> f32 {
        match self {
            SinkKind::Undo | SinkKind::Replace => SINK_SECONDS,
            SinkKind::Bulldoze => BULLDOZE_SINK_SECONDS,
        }
    }
}

impl RiseCue {
    pub fn name(self) -> &'static str {
        match self {
            RiseCue::Stage(s) => stage_name(s),
            RiseCue::Settled => "settled",
            RiseCue::Sink(_) => "sink",
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

pub use hex_play_state::{PropKind, RisePart, RiseRole};

pub trait RisePartPose {
    fn at(&self, pose: &RisePose) -> (Transform, bool);
}

impl RisePartPose for RisePart {
    /// The transform and visibility of this part at `pose`.
    fn at(&self, pose: &RisePose) -> (Transform, bool) {
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
    seconds: f32,
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
    /// Whether a drawn tile has completed every rise stage.
    pub(crate) fn settled_cell(&self, cell: Axial) -> bool {
        matches!(self.cells.get(&cell), Some(None))
    }

    /// Whether a drawn tile has reached the agents stage, including settled tiles.
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

    /// Holds the rise just begun at `cell` below ground for `secs` before its column starts, so a
    /// tile swapped in by a replace rises once the old one has sunk. No cue falls due meanwhile.
    pub(crate) fn delay(&mut self, cell: Axial, secs: f32) {
        if let Some(Some(r)) = self.cells.get_mut(&cell) {
            r.t -= secs;
        }
    }

    /// The tile at `cell` left the view: its root and water entities sink, then despawn.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn remove(
        &mut self,
        cell: Axial,
        tile: &str,
        kind: SinkKind,
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
            cue: RiseCue::Sink(kind),
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
                seconds: kind.seconds(),
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

/// Sinks removed tiles over their [`SinkKind::seconds`] (ease-in), then despawns them.
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
        let u = (s.t / s.seconds).min(1.0);
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

/// One dust puff or cloud: shared meshes, its own fading layer materials, CPU-moved particles.
struct Puff {
    entities: Vec<Entity>,
    layers: Vec<(Handle<StandardMaterial>, Layer)>,
    particles: Vec<Particle>,
    t: f32,
}

/// A puff's material layer: one colour whose alpha holds, then fades out quadratically.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    rgb: [f32; 3],
    alpha: f32,
    /// Seconds at full alpha.
    hold: f32,
    /// Seconds by which the alpha reaches 0.
    end: f32,
    shape: Shape,
}

/// The shared mesh a layer's particles draw with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// The rise and undo puffs' small spheres.
    Puff,
    /// The bulldoze cloud's large spheres, subdivided once more so their outline stays round.
    Cloud,
    /// Debris bits.
    Chunk,
}

impl Shape {
    fn mesh(self) -> Mesh {
        match self {
            Shape::Puff => Sphere::new(1.0).mesh().ico(1).expect("ico 1"),
            Shape::Cloud => Sphere::new(1.0).mesh().ico(2).expect("ico 2"),
            Shape::Chunk => Cuboid::new(1.0, 0.7, 1.2).into(),
        }
    }
}

impl Layer {
    fn alpha_at(&self, t: f32) -> f32 {
        let v = ((t - self.hold) / (self.end - self.hold)).clamp(0.0, 1.0);
        self.alpha * (1.0 - v) * (1.0 - v)
    }

    /// The same fade `time` times as long: at `time * t` it is where this one is at `t`.
    fn slowed(self, time: f32) -> Layer {
        Layer {
            hold: self.hold * time,
            end: self.end * time,
            ..self
        }
    }
}

/// The rise and undo puffs' colour.
const PUFF_RGB: [f32; 3] = [0.66, 0.58, 0.46];

/// The bulldoze cloud's size and duration against the first look's, which [`CLOUD_BASE_LAYERS`]
/// and [`cloud_base`] spell out: 80% of each (owner, 2026-10-14). Every phase scales with the
/// duration, so the envelope keeps its shape.
const CLOUD_SIZE: f32 = 0.8;
const CLOUD_TIME: f32 = 0.8;
/// Each puff's full-grown radius against the scaled first look's: the circles were still too big
/// at 80% (owner, 2026-10-14: "reduce the largest size by 20%").
const CLOUD_PEAK: f32 = 0.8;

/// The bulldoze cloud's layers at the first look's timing: a darker low swell over the hex, a
/// lighter billow from the rim and the centre that thins sooner as it rises, and dark debris bits.
const CLOUD_BASE_LAYERS: [Layer; 3] = [
    Layer {
        rgb: [0.54, 0.45, 0.34],
        alpha: 0.42,
        hold: 0.9,
        end: 2.7,
        shape: Shape::Cloud,
    },
    Layer {
        rgb: PUFF_RGB,
        alpha: 0.34,
        hold: 0.6,
        end: 2.4,
        shape: Shape::Cloud,
    },
    Layer {
        rgb: [0.30, 0.24, 0.17],
        alpha: 0.95,
        hold: 1.2,
        end: 2.2,
        shape: Shape::Chunk,
    },
];
/// Bulldoze cloud particles per layer: swell, billow (rim, then centre), debris.
const CLOUD_SWELL: u32 = 40;
const CLOUD_RIM: u32 = 24;
const CLOUD_COLUMN: u32 = 38;
const CLOUD_DEBRIS: u32 = 18;
/// Particles in one bulldoze cloud.
pub const CLOUD_PARTICLES: u32 = CLOUD_SWELL + CLOUD_RIM + CLOUD_COLUMN + CLOUD_DEBRIS;

/// The bulldoze cloud's layers as drawn: [`CLOUD_BASE_LAYERS`] at [`CLOUD_TIME`] of their duration.
fn cloud_layers() -> [Layer; 3] {
    CLOUD_BASE_LAYERS.map(|l| l.slowed(CLOUD_TIME))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Particle {
    origin: Vec3,
    /// Initial throw (m/s), slowed by `drag`.
    vel: Vec3,
    /// Drag on the throw (1/s).
    drag: f32,
    /// Steady rise as the dust spreads (m/s).
    lift: f32,
    /// Fall (m/s²), debris only.
    gravity: f32,
    /// Radius at spawn and when grown (m).
    size: (f32, f32),
    /// Seconds to grow to full size.
    grow: f32,
    /// Life fraction after which the particle shrinks away (1: never).
    shrink: f32,
    life: f32,
    /// Index into the puff's layers.
    layer: usize,
    /// Tumble rate (rad/s), debris only.
    spin: f32,
}

impl Particle {
    /// The same motion `size` times as large about `centre` and `time` times as long: at `time * t`
    /// it stands at `centre + size * (self.at(t) - centre)` (the ground clamp aside) with `size`
    /// times the radius.
    fn scaled(self, centre: Vec3, size: f32, time: f32) -> Particle {
        let speed = size / time;
        Particle {
            origin: centre + (self.origin - centre) * size,
            vel: self.vel * speed,
            drag: self.drag / time,
            lift: self.lift * speed,
            gravity: self.gravity * speed / time,
            size: (self.size.0 * size, self.size.1 * size),
            grow: self.grow * time,
            shrink: self.shrink,
            life: self.life * time,
            layer: self.layer,
            spin: self.spin / time,
        }
    }

    fn at(&self, t: f32) -> Vec3 {
        let throw = self.vel * (1.0 - (-self.drag * t).exp()) / self.drag;
        let mut p = self.origin + throw + Vec3::Y * (self.lift * t - 0.5 * self.gravity * t * t);
        p.y = p.y.max(0.05);
        p
    }

    /// The same particle with its full-grown radius `k` times as large (never below its start).
    fn peaked(mut self, k: f32) -> Particle {
        self.size.1 = (self.size.1 * k).max(self.size.0);
        self
    }

    fn radius(&self, t: f32) -> f32 {
        let u = (t / self.life).min(1.0);
        let grown =
            self.size.0 + (self.size.1 - self.size.0) * ease_out_cubic((t / self.grow).min(1.0));
        let tail = if self.shrink < 1.0 {
            1.0 - smoothstep01(((u - self.shrink) / (1.0 - self.shrink)).clamp(0.0, 1.0))
        } else {
            1.0
        };
        (grown * tail).max(1e-3)
    }
}

/// What a rise cue raises.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dust {
    /// `n` particles thrown from the hex rim (a column start or an undo sink).
    Puff { n: u32, strength: f32, alpha: f32 },
    /// The bulldoze cloud over the whole hex ([`CLOUD_PARTICLES`]).
    Cloud,
}

/// The dust a cue raises: none for cues a fast-forward skipped or under reduced motion.
pub fn dust_for(cue: RiseCue, skipped: bool, reduced: bool) -> Option<Dust> {
    if skipped || reduced {
        return None;
    }
    match cue {
        RiseCue::Stage(RiseStage::Column) => Some(Dust::Puff {
            n: 48,
            strength: 1.0,
            alpha: 0.32,
        }),
        RiseCue::Sink(SinkKind::Undo) => Some(Dust::Puff {
            n: 24,
            strength: 0.6,
            alpha: 0.26,
        }),
        RiseCue::Sink(SinkKind::Bulldoze) => Some(Dust::Cloud),
        _ => None,
    }
}

/// Live dust puffs and the shared particle meshes.
#[derive(Resource, Default)]
pub struct PlayDust {
    puffs: Vec<Puff>,
    /// One shared mesh per [`Shape`], made on first use.
    meshes: [Option<Handle<Mesh>>; 3],
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
            let life = 0.9 + 0.5 * hash01(cell, i, 7);
            Particle {
                origin: Vec3::new(cx, 0.15, cz) + dir * r,
                vel: (dir * (2.5 + 3.0 * hash01(cell, i, 3))
                    + Vec3::Y * (1.2 + 2.2 * hash01(cell, i, 4)))
                    * strength,
                drag: 3.0,
                lift: 0.4,
                gravity: 0.0,
                size: (
                    0.3 + 0.3 * hash01(cell, i, 5),
                    1.2 + 1.2 * hash01(cell, i, 6),
                ),
                grow: life,
                shrink: 1.0,
                life,
                layer: 0,
                spin: 0.0,
            }
        })
        .collect()
}

/// The bulldoze cloud at `cell` as drawn: [`cloud_base`] at [`CLOUD_SIZE`] the size about the
/// cell's centre on the ground and [`CLOUD_TIME`] the duration. Deterministic per cell.
fn cloud(cell: Axial) -> Vec<Particle> {
    let [cx, cz] = cell.to_world(HEX_RADIUS);
    let centre = Vec3::new(cx, 0.0, cz);
    cloud_base(cell)
        .into_iter()
        .map(|p| p.scaled(centre, CLOUD_SIZE, CLOUD_TIME).peaked(CLOUD_PEAK))
        .collect()
}

/// The bulldoze cloud's first look at `cell`: a low swell across the whole hex, a billow thrown
/// out past the rim and up from the centre, and debris bits tossed up that fall back.
fn cloud_base(cell: Axial) -> Vec<Particle> {
    use std::f32::consts::TAU;
    let [cx, cz] = cell.to_world(HEX_RADIUS);
    let centre = Vec3::new(cx, 0.0, cz);
    let h = |i: u32, salt: u32| hash01(cell, i, salt);
    // A point in the disc of radius `r`, uniform by area.
    let disc = |i: u32, r: f32| {
        let a = TAU * h(i, 21);
        let d = r * h(i, 22).sqrt();
        (Vec3::new(a.cos(), 0.0, a.sin()), d)
    };
    let mut out = Vec::with_capacity(CLOUD_PARTICLES as usize);
    for i in 0..CLOUD_SWELL {
        let (dir, d) = disc(i, HEX_RADIUS * 0.8);
        out.push(Particle {
            origin: centre + dir * d + Vec3::Y * (0.6 + 1.0 * h(i, 24)),
            vel: dir * (0.8 + 1.6 * h(i, 25)) + Vec3::Y * (2.0 + 2.5 * h(i, 26)),
            drag: 3.0,
            lift: 0.8 + 0.7 * h(i, 27),
            gravity: 0.0,
            size: (2.6 + 1.4 * h(i, 28), 5.0 + 2.0 * h(i, 29)),
            grow: 0.6,
            shrink: 0.7,
            life: 2.3 + 0.5 * h(i, 23),
            layer: 0,
            spin: 0.0,
        });
    }
    for j in 0..CLOUD_RIM {
        let i = CLOUD_SWELL + j;
        let a = (j as f32 + h(i, 31)) / CLOUD_RIM as f32 * TAU;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        out.push(Particle {
            origin: centre + dir * HEX_RADIUS * (0.82 + 0.14 * h(i, 32)) + Vec3::Y * 0.3,
            vel: dir * (4.0 + 5.0 * h(i, 33)) + Vec3::Y * (2.5 + 3.5 * h(i, 34)),
            drag: 3.0,
            lift: 1.2 + 1.0 * h(i, 35),
            gravity: 0.0,
            size: (1.2 + 0.6 * h(i, 36), 3.5 + 2.0 * h(i, 37)),
            grow: 0.8,
            shrink: 0.6,
            life: 2.0 + 0.6 * h(i, 38),
            layer: 1,
            spin: 0.0,
        });
    }
    for j in 0..CLOUD_COLUMN {
        let i = CLOUD_SWELL + CLOUD_RIM + j;
        let (dir, d) = disc(i, HEX_RADIUS * 0.65);
        out.push(Particle {
            origin: centre + dir * d + Vec3::Y * 1.0,
            vel: dir * (0.6 + 1.4 * h(i, 41)) + Vec3::Y * (6.0 + 5.0 * h(i, 42)),
            drag: 2.0,
            lift: 2.0 + 1.4 * h(i, 43),
            gravity: 0.0,
            size: (2.2 + 1.2 * h(i, 44), 5.0 + 2.0 * h(i, 45)),
            grow: 0.9,
            shrink: 0.55,
            life: 2.1 + 0.5 * h(i, 46),
            layer: 1,
            spin: 0.0,
        });
    }
    for j in 0..CLOUD_DEBRIS {
        let i = CLOUD_SWELL + CLOUD_RIM + CLOUD_COLUMN + j;
        let (dir, d) = disc(i, HEX_RADIUS * 0.7);
        let s = 0.7 + 0.6 * h(i, 51);
        let (vy, g) = (7.0 + 6.0 * h(i, 53), 14.0);
        // Drag only shortens the flight, so the bit lies on the ground for its last 0.3 s or more.
        let landed = (vy + (vy * vy + 2.0 * g * 0.6).sqrt()) / g + 0.3;
        out.push(Particle {
            origin: centre + dir * d + Vec3::Y * 0.6,
            vel: dir * (3.0 + 5.0 * h(i, 52)) + Vec3::Y * vy,
            drag: 0.4,
            lift: 0.0,
            gravity: g,
            size: (s, s),
            grow: 0.1,
            shrink: 0.75,
            life: landed,
            layer: 2,
            spin: 4.0 + 6.0 * h(i, 55),
        });
    }
    out
}

/// Spawns a puff on each column start and undo sink and a cloud on each bulldoze sink, and moves
/// and fades the live ones.
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
        let Some(kind) = dust_for(m.cue, m.skipped, reduced.0) else {
            continue;
        };
        let (layers, ps) = match kind {
            Dust::Puff { n, strength, alpha } => (
                vec![Layer {
                    rgb: PUFF_RGB,
                    alpha,
                    hold: 0.0,
                    end: 1.2,
                    shape: Shape::Puff,
                }],
                particles(m.cell, n, strength),
            ),
            Dust::Cloud => (cloud_layers().to_vec(), cloud(m.cell)),
        };
        let layers: Vec<(Handle<StandardMaterial>, Layer)> = layers
            .into_iter()
            .map(|l| {
                let [r, g, b] = l.rgb;
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgba(r, g, b, l.alpha),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                });
                (material, l)
            })
            .collect();
        let entities = ps
            .iter()
            .map(|p| {
                let (material, layer) = &layers[p.layer];
                let mesh = dust.meshes[layer.shape as usize]
                    .get_or_insert_with(|| meshes.add(layer.shape.mesh()))
                    .clone();
                commands
                    .spawn((
                        Mesh3d(mesh),
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
            layers,
            particles: ps,
            t: 0.0,
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
            alive |= t < p.life;
            if let Ok(mut tr) = transforms.get_mut(*e) {
                tr.translation = p.at(t);
                tr.scale = Vec3::splat(p.radius(t));
                if p.spin != 0.0 {
                    tr.rotation =
                        Quat::from_euler(EulerRot::YXZ, p.spin * t, 0.7 * p.spin * t, 0.0);
                }
            }
        }
        for (handle, layer) in &puff.layers {
            if let Some(mut m) = materials.get_mut(handle) {
                m.base_color.set_alpha(layer.alpha_at(t));
            }
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

    /// Removes a tile of `kind` at (1, 0): the rises, the cues and whether its root still exists.
    fn removed(kind: SinkKind, reduced: bool) -> (PlayRises, Vec<RiseMessage>, bool) {
        let mut world = World::new();
        let root = world.spawn(Transform::IDENTITY).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut rises = PlayRises::default();
        let mut out = vec![];
        let mut commands = Commands::new(&mut queue, &world);
        rises.remove(
            Axial::new(1, 0),
            "forest",
            kind,
            vec![root],
            reduced,
            &mut commands,
            &mut out,
        );
        queue.apply(&mut world);
        let alive = world.get_entity(root).is_ok();
        (rises, out, alive)
    }

    #[test]
    fn bulldoze_sinks_longer_inside_the_cloud_and_undo_with_the_puff() {
        let (rises, out, alive) = removed(SinkKind::Bulldoze, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cue, RiseCue::Sink(SinkKind::Bulldoze));
        assert_eq!(
            out[0].cue.name(),
            "sink",
            "the event wire keeps one sink stage"
        );
        assert!(alive && rises.sinks.len() == 1);
        assert_eq!(rises.sinks[0].seconds, BULLDOZE_SINK_SECONDS);
        assert_eq!(
            dust_for(out[0].cue, out[0].skipped, false),
            Some(Dust::Cloud)
        );

        let (rises, out, _) = removed(SinkKind::Replace, false);
        assert_eq!(out[0].cue.name(), "sink");
        assert_eq!(
            rises.sinks[0].seconds, SINK_SECONDS,
            "a replace sinks short"
        );
        assert_eq!(dust_for(out[0].cue, out[0].skipped, false), None);

        let (rises, out, _) = removed(SinkKind::Undo, false);
        assert_eq!(out[0].cue, RiseCue::Sink(SinkKind::Undo));
        assert_eq!(rises.sinks[0].seconds, SINK_SECONDS);
        assert!(matches!(
            dust_for(out[0].cue, out[0].skipped, false),
            Some(Dust::Puff { n: 24, .. })
        ));
    }

    #[test]
    fn reduced_motion_skips_the_bulldoze_cloud_and_sink() {
        let (rises, out, alive) = removed(SinkKind::Bulldoze, true);
        assert!(out[0].skipped && rises.sinks.is_empty() && !alive);
        assert_eq!(dust_for(out[0].cue, out[0].skipped, true), None);
        let bulldoze = RiseCue::Sink(SinkKind::Bulldoze);
        assert_eq!(dust_for(bulldoze, false, true), None);
        assert_eq!(dust_for(bulldoze, true, false), None);
    }

    #[test]
    fn the_cloud_covers_the_hex_billows_up_and_spreads_past_the_rim() {
        let cell = Axial::new(2, -1);
        let ps = cloud(cell);
        assert_eq!(ps.len(), CLOUD_PARTICLES as usize);
        assert!(ps.len() <= 120);
        assert_eq!(ps, cloud(cell), "deterministic per cell");
        assert_ne!(ps, cloud(Axial::new(0, 1)));
        let [cx, cz] = cell.to_world(HEX_RADIUS);
        let flat = |p: Vec3| Vec2::new(p.x - cx, p.z - cz).length();
        let dust: Vec<&Particle> = ps.iter().filter(|p| p.layer < 2).collect();
        let inner = dust
            .iter()
            .filter(|p| flat(p.origin) < 0.5 * HEX_RADIUS)
            .count();
        assert!(inner >= 15, "the hex interior, not just the rim: {inner}");
        let top = dust
            .iter()
            .map(|p| p.at(p.life).y + p.size.1)
            .fold(0.0, f32::max);
        assert!(top > 6.0, "billows several metres up: {top}");
        let reach = dust
            .iter()
            .map(|p| flat(p.at(0.8)) + p.radius(0.8))
            .fold(0.0, f32::max);
        // About 1.1 m past the rim since the owner's 20% cut of the full-grown puffs (was 1.5 m).
        assert!(reach > HEX_RADIUS + 1.0, "spreads past the rim: {reach}");
        let life = ps.iter().map(|p| p.life).fold(0.0, f32::max);
        assert!(life <= 2.24 + 1e-4, "clear by 2.24 s: {life}");
        assert!(
            (BULLDOZE_SINK_SECONDS + 1.2..BULLDOZE_SINK_SECONDS + 1.6).contains(&life),
            "lingers about 1.5 s after the tile is gone: {life}"
        );
        let layers = cloud_layers();
        let debris: Vec<&Particle> = ps.iter().filter(|p| p.layer == 2).collect();
        assert!(!debris.is_empty() && debris.iter().all(|p| layers[p.layer].shape == Shape::Chunk));
        assert!(
            debris.iter().all(|p| p.at(p.life).y < 0.1),
            "debris falls back"
        );
        let thin = layers[1].alpha_at(1.6) / layers[1].alpha;
        assert!(thin < layers[0].alpha_at(1.6) / layers[0].alpha);
        assert!(layers.iter().all(|l| l.alpha_at(l.end) == 0.0));
    }

    /// Owner 2026-10-14: "reduce dust size and duration by 20%". The drawn cloud is the first look
    /// at 80% of its size about the cell's centre and 80% of its duration, every phase included, with
    /// each puff's full-grown radius cut by [`CLOUD_PEAK`] on top ("reduce the largest size by 20%").
    #[test]
    fn the_cloud_is_the_first_look_at_80_percent_size_and_duration() {
        let cell = Axial::new(2, -1);
        let [cx, cz] = cell.to_world(HEX_RADIUS);
        let centre = Vec3::new(cx, 0.0, cz);
        let base: Vec<Particle> = cloud_base(cell)
            .into_iter()
            .map(|p| {
                let s = p.scaled(centre, CLOUD_SIZE, 1.0).peaked(CLOUD_PEAK);
                Particle {
                    size: (p.size.0, s.size.1 / CLOUD_SIZE),
                    ..p
                }
            })
            .collect();
        let drawn = cloud(cell);
        assert_eq!(base.len(), drawn.len());
        for (b, d) in base.iter().zip(&drawn) {
            assert!(
                (d.life - 0.8 * b.life).abs() < 1e-5,
                "{} {}",
                d.life,
                b.life
            );
            for tau in [0.0, 0.1, 0.4, 0.9, 1.5, b.life] {
                let t = 0.8 * tau;
                let want = centre + (b.at(tau) - centre) * 0.8;
                let got = d.at(t);
                assert!(
                    (got.xz() - want.xz()).length() < 1e-3
                        && (got.y - want.y.max(0.05)).abs() < 1e-3,
                    "at {t} s: {got} against {want}"
                );
                assert!(
                    (d.radius(t) - (0.8 * b.radius(tau)).max(1e-3)).abs() < 1e-4,
                    "radius at {t} s"
                );
            }
        }
        for (b, d) in CLOUD_BASE_LAYERS.iter().zip(cloud_layers()) {
            assert_eq!((d.rgb, d.alpha, d.shape), (b.rgb, b.alpha, b.shape));
            for tau in [0.0, 0.5, 1.0, 1.7, 2.3, 2.7] {
                assert!((d.alpha_at(0.8 * tau) - b.alpha_at(tau)).abs() < 1e-5);
            }
        }
        let life = |ps: &[Particle]| ps.iter().map(|p| p.life).fold(0.0, f32::max);
        assert!((life(&drawn) - 0.8 * life(&base)).abs() < 1e-5);
    }

    /// Entities the dust system spawns for one cue.
    fn spawned_for(cue: RiseCue, reduced: bool) -> usize {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<PlayDust>()
            .insert_resource(ReducedMotion(reduced))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_message::<RiseMessage>()
            .add_systems(Update, dust);
        app.world_mut().write_message(RiseMessage {
            cell: Axial::new(1, 0),
            tile: "forest".into(),
            cue,
            skipped: false,
        });
        app.update();
        let mut q = app.world_mut().query::<&Mesh3d>();
        q.iter(app.world()).count()
    }

    #[test]
    fn the_dust_system_raises_the_cloud_only_for_a_bulldoze() {
        let bulldoze = RiseCue::Sink(SinkKind::Bulldoze);
        assert_eq!(spawned_for(bulldoze, false), CLOUD_PARTICLES as usize);
        assert_eq!(spawned_for(RiseCue::Sink(SinkKind::Undo), false), 24);
        assert_eq!(
            spawned_for(RiseCue::Sink(SinkKind::Replace), false),
            0,
            "a tile swapped out by a replace sinks without dust"
        );
        assert_eq!(spawned_for(bulldoze, true), 0, "reduced motion: no cloud");
    }
}
