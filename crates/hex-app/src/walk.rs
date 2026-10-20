//! First-person walk mode (PRD P8, D14): the walker stands on the kit ground at eye height, walks
//! with WASD (Shift sprints), looks with the mouse and hops with Space, and is stopped by buildings,
//! trunks, rocks, deep sea water and the map edge, and fords rivers (`hex_play::walk`). The world
//! keeps running.
//!
//! Enter and exit follow `GameState::walk_mode`, so the F key, Esc, the HUD button and the `walk`
//! command share one snapshot: entering takes the orbit camera's transform and FOV, leaving writes
//! them back exactly. Saves never see the walker: they take the snapshot's orbit camera.
use std::collections::VecDeque;

use bevy::{
    light::{EnvironmentMapLight, Skybox},
    prelude::*,
};
use hex_play::{
    play::PlaySystems,
    tilt_shift::TiltOverride,
    walk::{Blocked, WalkGround},
};
use serde_json::{Value, json};

use crate::actions::Action;
use crate::camera::{focus, free_rotation, yaw_pitch};
use crate::game::GameState;

/// Eye height above the feet (m).
pub const EYE_HEIGHT: f32 = 1.7;
/// The walk speed setting's lowest value and its highest (m/s); owner 2026-10-14.
pub const WALK_SPEED: f32 = 2.8;
pub const WALK_SPEED_MAX: f32 = 8.0;
/// The walk speed setting's default (m/s); owner 2026-10-20.
pub const WALK_SPEED_DEFAULT: f32 = 5.0;
/// Shift sprints at this many times the walk speed, whatever the setting.
pub const SPRINT_FACTOR: f32 = 2.0;
/// Vertical field of view while walking (degrees); leaving restores the orbit camera's.
pub const WALK_FOV_DEG: f32 = 60.0;
/// Mouse look rate (radians per pixel) and the pitch limit short of straight up or down.
pub const LOOK_RAD_PER_PX: f32 = 0.0025;
const MAX_PITCH: f32 = 1.4;
/// The eye eases toward its height over the ground at this rate (1/s), so kerbs and slopes glide.
const EYE_EASE: f32 = 12.0;
/// A hop's take-off speed (m/s) and gravity (m/s²): about half a metre up.
const HOP_SPEED: f32 = 3.2;
const GRAVITY: f32 = 9.81;
/// `walk_to` moves a target that is blocked (a trunk, deep water) to standable ground this close (m).
pub const WALK_TO_SNAP: f32 = 4.0;
/// The pitch the walker enters at (radians, slightly down).
const ENTRY_PITCH: f32 = -0.08;

/// The walk speed setting (m/s), always within [`WALK_SPEED`]..=[`WALK_SPEED_MAX`]. It is per
/// viewer (the overlay keeps it in localStorage), not part of the world save.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkSpeed(f32);

impl Default for WalkSpeed {
    fn default() -> Self {
        Self(WALK_SPEED_DEFAULT)
    }
}

impl WalkSpeed {
    /// `v` clamped to the setting's range; `None` for a value that is not finite.
    pub fn new(v: f32) -> Option<Self> {
        v.is_finite()
            .then(|| Self(v.clamp(WALK_SPEED, WALK_SPEED_MAX)))
    }

    pub fn walk(self) -> f32 {
        self.0
    }

    pub fn sprint(self) -> f32 {
        self.0 * SPRINT_FACTOR
    }

    /// The speed for a stride: sprinting or walking.
    pub fn of(self, sprint: bool) -> f32 {
        if sprint { self.sprint() } else { self.walk() }
    }
}

/// The orbit camera as walk mode found it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitSnapshot {
    pub transform: Transform,
    /// Vertical FOV in radians.
    pub fov: f32,
}

/// A scripted request from the `walk_to`, `walk_look` and `walk_step` commands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WalkRequest {
    /// Stand on the ground at world (x, z), refused where nothing stands; optional heading and pitch.
    To {
        x: f32,
        z: f32,
        heading: Option<f32>,
        pitch: Option<f32>,
    },
    /// Look at heading and pitch (degrees); either may be omitted.
    Look {
        heading: Option<f32>,
        pitch: Option<f32>,
    },
    /// Walk as if the keys were held for `seconds` of game time: `forward` and `right` in -1..1.
    Step {
        forward: f32,
        right: f32,
        seconds: f32,
        sprint: bool,
    },
}

/// The walker: feet on the ground, heading and pitch, the eased eye height and a hop in flight.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Walker {
    pub feet: Vec3,
    /// Yaw about +Y (0 looks toward -Z, positive turns toward -X), as `camera::yaw_pitch`.
    pub yaw: f32,
    pub pitch: f32,
    pub eye_y: f32,
    pub vy: f32,
    pub airborne: bool,
}

/// The open walk session: `saved` is `Some` while the game is in walk mode.
#[derive(Resource, Default)]
pub struct WalkSession {
    pub saved: Option<OrbitSnapshot>,
    pub walker: Walker,
    /// The page holds the pointer lock (the HUD tells the engine): mouse motion looks with no button.
    pub pointer_lock: bool,
    pub requests: VecDeque<WalkRequest>,
    /// A scripted walk in progress: direction (right, forward), sprint, game seconds left.
    pub script: Option<(Vec2, bool, f32)>,
    /// The walk speed setting; the keys and scripted walks read it every frame.
    pub speed: WalkSpeed,
    /// This frame's input: ground offset (right, forward) in metres, look in pixels, a hop.
    pub stride: Vec2,
    pub look: Vec2,
    pub hop: bool,
    /// The latest move's stop reason, kept until the walker next moves freely.
    pub blocked: Option<Blocked>,
    /// Metres moved in the latest frame.
    pub moved: f32,
    /// Metres walked since entering.
    pub distance: f32,
}

impl WalkSession {
    /// The orbit camera a save should store while walking.
    pub fn orbit(&self) -> Option<Transform> {
        self.saved.map(|s| s.transform)
    }

    /// Takes a walk action from the input layer.
    pub fn intent(&mut self, a: &Action) {
        match *a {
            Action::Stride(d) => self.stride += d,
            Action::WalkLook(d) => self.look += d,
            Action::Hop => self.hop = true,
            _ => {}
        }
    }
}

/// Game actions refused while walking (placement, bulldoze, history and photo mode), by the name
/// `last_result` and `actionRefused` report; `None` for the rest.
pub fn refused_while_walking(a: &Action) -> Option<&'static str> {
    Some(match a {
        Action::Primary => "primary",
        Action::Place(_) => "place",
        Action::Pin(_) => "pin",
        Action::Bulldoze(_) => "bulldoze",
        Action::Rotate(_) => "rotate",
        Action::Undo => "undo",
        Action::Redo => "redo",
        Action::ToggleBulldoze | Action::SetTool(_) => "set_tool",
        Action::TogglePhoto => "toggle_photo",
        _ => return None,
    })
}

/// Unit ground directions of a yaw: forward (the view's) and right.
pub fn axes(yaw: f32) -> (Vec2, Vec2) {
    (
        Vec2::new(-yaw.sin(), -yaw.cos()),
        Vec2::new(yaw.cos(), -yaw.sin()),
    )
}

/// Compass heading in degrees, clockwise viewed from above: 0 looks toward -Z, 90 toward +X.
pub fn heading_deg(yaw: f32) -> f32 {
    (-yaw.to_degrees()).rem_euclid(360.0)
}

pub fn yaw_of_heading(deg: f32) -> f32 {
    -deg.to_radians()
}

/// The eye transform of a walker.
pub fn eye(w: &Walker) -> Transform {
    Transform::from_xyz(w.feet.x, w.eye_y, w.feet.z)
        .with_rotation(free_rotation(w.yaw, w.pitch, 0.0))
}

/// Puts the walker on the standable ground nearest `p` within [`WALK_TO_SNAP`] metres, or says
/// why nothing stands at `p`.
fn stand_at(ground: &WalkGround, w: &mut Walker, p: Vec2) -> Result<(), Blocked> {
    let at = match ground.stand(p) {
        Ok(h) => Vec3::new(p.x, h, p.y),
        Err(b) => ground
            .nearest_stand(p)
            .filter(|q| q.xz().distance(p) <= WALK_TO_SNAP)
            .ok_or(b)?,
    };
    let h = at.y;
    w.feet = at;
    w.eye_y = h + EYE_HEIGHT;
    w.vy = 0.0;
    w.airborne = false;
    Ok(())
}

/// One frame of a scripted walk toward `dir` (right, forward) at `speed` m/s with `left` game
/// seconds to go: the ground offset for this frame and the seconds left after it.
pub fn scripted_stride(dir: Vec2, speed: f32, left: f32, dt: f32) -> (Vec2, f32) {
    let t = dt.min(left);
    (dir.normalize_or_zero() * speed * t, left - t)
}

/// One frame of walking: look, the scripted or keyed stride through the ground, the hop and the
/// eased eye. Returns the metres moved.
pub fn advance(s: &mut WalkSession, ground: &WalkGround, dt: f32) -> f32 {
    let w = &mut s.walker;
    w.yaw -= s.look.x * LOOK_RAD_PER_PX;
    w.pitch = (w.pitch - s.look.y * LOOK_RAD_PER_PX).clamp(-MAX_PITCH, MAX_PITCH);
    let mut stride = s.stride;
    if let Some((dir, sprint, left)) = s.script.as_mut() {
        let (d, rest) = scripted_stride(*dir, s.speed.of(*sprint), *left, dt);
        stride += d;
        *left = rest;
        if rest <= 1e-6 {
            s.script = None;
        }
    }
    let (fwd, right) = axes(w.yaw);
    let delta = right * stride.x + fwd * stride.y;
    if s.hop && !w.airborne {
        w.vy = HOP_SPEED;
        w.airborne = true;
    }
    let before = w.feet;
    let ground_y = if delta != Vec2::ZERO {
        let step = ground.step(w.feet, delta);
        w.feet.x = step.pos.x;
        w.feet.z = step.pos.z;
        s.blocked = step.blocked;
        step.pos.y
    } else {
        ground.ground(w.feet.xz()).unwrap_or(w.feet.y)
    };
    if w.airborne {
        w.feet.y += w.vy * dt;
        w.vy -= GRAVITY * dt;
        if w.feet.y <= ground_y {
            w.feet.y = ground_y;
            w.vy = 0.0;
            w.airborne = false;
        }
    } else {
        w.feet.y = ground_y;
    }
    let target = w.feet.y + EYE_HEIGHT;
    w.eye_y += (target - w.eye_y) * (1.0 - (-EYE_EASE * dt).exp());
    s.stride = Vec2::ZERO;
    s.look = Vec2::ZERO;
    s.hop = false;
    let moved = before.xz().distance(w.feet.xz());
    s.moved = moved;
    s.distance += moved;
    moved
}

/// The sky behind the walker: the cubemap that lights the scene (the IBL's specular map, whose
/// sharpest mip is the HDRI) at the IBL's intensity and rotation, so its sun stands where the
/// light comes from and it dims with the time of day. The orbit camera keeps the grade's dark
/// backdrop (owner Q4); from eye height that backdrop filled everything above the horizon.
pub fn walk_sky(env: &EnvironmentMapLight) -> Skybox {
    Skybox {
        image: Some(env.specular_map.clone()),
        brightness: env.intensity,
        rotation: env.rotation,
    }
}

/// Enters and leaves walk mode on `GameState::walk_mode` edges and walks while it is on (after
/// `game::apply_actions` and `photo::sync`). Walking puts [`walk_sky`] on the camera; leaving
/// takes it off.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync(
    mut commands: Commands,
    game: Res<GameState>,
    mut session: ResMut<WalkSession>,
    ground: Res<WalkGround>,
    time: Res<Time>,
    tilt: Option<Res<TiltOverride>>,
    mut cams: Query<
        (
            Entity,
            &mut Transform,
            &mut Projection,
            Option<&EnvironmentMapLight>,
            Option<&mut Skybox>,
        ),
        With<Camera3d>,
    >,
) {
    let Some((cam, mut t, mut proj, env, sky)) = cams.iter_mut().next() else {
        return;
    };
    match (game.walk_mode, session.saved) {
        (true, None) => {
            let fov = match &*proj {
                Projection::Perspective(p) => p.fov,
                _ => return,
            };
            session.saved = Some(OrbitSnapshot { transform: *t, fov });
            let f = focus(&t);
            let (yaw, _) = yaw_pitch(*t.forward());
            let start = ground
                .nearest_stand(f.xz())
                .unwrap_or(Vec3::new(f.x, 0.0, f.z));
            session.walker = Walker {
                feet: start,
                yaw,
                pitch: ENTRY_PITCH,
                eye_y: start.y + EYE_HEIGHT,
                vy: 0.0,
                airborne: false,
            };
            session.blocked = None;
            session.distance = 0.0;
            if let Projection::Perspective(p) = &mut *proj {
                p.fov = WALK_FOV_DEG.to_radians();
            }
            if let Some(env) = env {
                commands.entity(cam).insert(walk_sky(env));
            }
            info!("walk mode: on at {start}");
        }
        (false, Some(snap)) => {
            *t = snap.transform;
            if let Projection::Perspective(p) = &mut *proj {
                p.fov = snap.fov;
            }
            if sky.is_some() {
                commands.entity(cam).remove::<Skybox>();
            }
            if tilt.is_some() {
                commands.remove_resource::<TiltOverride>();
            }
            let s = &mut *session;
            s.saved = None;
            s.script = None;
            s.requests.clear();
            s.pointer_lock = false;
            (s.stride, s.look, s.hop) = (Vec2::ZERO, Vec2::ZERO, false);
            info!("walk mode: off, orbit camera restored");
            return;
        }
        (false, None) => {
            let s = &mut *session;
            s.requests.clear();
            (s.stride, s.look, s.hop) = (Vec2::ZERO, Vec2::ZERO, false);
            return;
        }
        (true, Some(_)) => match (env, sky) {
            (Some(env), Some(mut sky)) if sky.brightness != env.intensity => {
                sky.brightness = env.intensity;
            }
            (Some(env), None) => {
                commands.entity(cam).insert(walk_sky(env));
            }
            _ => {}
        },
    }
    let s = &mut *session;
    while let Some(r) = s.requests.pop_front() {
        match r {
            WalkRequest::To {
                x,
                z,
                heading,
                pitch,
            } => match stand_at(&ground, &mut s.walker, Vec2::new(x, z)) {
                Ok(()) => {
                    s.blocked = None;
                    s.script = None;
                    look_at(&mut s.walker, heading, pitch);
                }
                Err(b) => {
                    s.blocked = Some(b.clone());
                    warn!("walk_to ({x}, {z}) refused: {}", b.reason());
                }
            },
            WalkRequest::Look { heading, pitch } => look_at(&mut s.walker, heading, pitch),
            WalkRequest::Step {
                forward,
                right,
                seconds,
                sprint,
            } => {
                s.script = (seconds > 0.0 && seconds.is_finite()).then_some((
                    Vec2::new(right, forward),
                    sprint,
                    seconds,
                ));
            }
        }
    }
    advance(s, &ground, time.delta_secs());
    let e = eye(&s.walker);
    if *t != e {
        *t = e;
    }
    // Tilt-shift is off in first person whatever the setting: a zero-strength override.
    let off = TiltOverride {
        focus_m: 1.0,
        focus_uv_y: 0.5,
        strength: 0.0,
    };
    if tilt.as_deref() != Some(&off) {
        commands.insert_resource(off);
    }
}

fn look_at(w: &mut Walker, heading: Option<f32>, pitch: Option<f32>) {
    if let Some(h) = heading.filter(|h| h.is_finite()) {
        w.yaw = yaw_of_heading(h);
    }
    if let Some(p) = pitch.filter(|p| p.is_finite()) {
        w.pitch = p.to_radians().clamp(-MAX_PITCH, MAX_PITCH);
    }
}

fn round(v: f32) -> f64 {
    (f64::from(v) * 1e3).round() / 1e3
}

/// `state().walk`: whether walk mode is on, the feet, eye, heading and pitch, the ground under
/// the feet, the tile there, the water depth, what stopped the latest move, the scripted walk, and
/// the walk and sprint speeds (reported whether walk mode is on or not).
pub fn state_json(s: &WalkSession, ground: &WalkGround) -> Value {
    let on = s.saved.is_some();
    let w = &s.walker;
    let v3 = |v: Vec3| json!([round(v.x), round(v.y), round(v.z)]);
    let tile = ground.tile_at(w.feet.xz());
    json!({
        "on": on,
        "feet": on.then(|| v3(w.feet)),
        "eye": on.then(|| v3(Vec3::new(w.feet.x, w.eye_y, w.feet.z))),
        "heading": on.then(|| round(heading_deg(w.yaw))),
        "pitch": on.then(|| round(w.pitch.to_degrees())),
        "ground": on.then(|| ground.ground(w.feet.xz()).ok().map(round)).flatten(),
        "water_depth": on.then(|| round(ground.water_depth(w.feet.xz()))),
        "cell": tile.filter(|_| on).map(|(c, _)| json!({ "q": c.q, "r": c.r })),
        "tile": tile.filter(|_| on).map(|(_, t)| t),
        "blocked": s.blocked.as_ref().filter(|_| on).map(Blocked::reason),
        "blocked_by": s.blocked.as_ref().filter(|_| on).and_then(|b| b.piece()),
        "moving": on && s.moved > 0.0,
        "airborne": on && w.airborne,
        "distance": round(s.distance),
        "step_left": s.script.filter(|_| on).map(|(_, _, left)| round(left)),
        "pointer_lock": on && s.pointer_lock,
        "walk_speed": round(s.speed.walk()),
        "sprint_speed": round(s.speed.sprint()),
    })
}

/// While walking, the audio focus mix centres on the walker's feet with a one-hex spread, and the
/// listener (the camera) is the walker's head.
pub fn extend_play_view(play: &mut Value, s: &WalkSession) {
    let (Some(_), Value::Object(map)) = (s.saved, play) else {
        return;
    };
    let f = s.walker.feet;
    map.insert("focus".into(), json!([round(f.x), round(f.y), round(f.z)]));
    map.insert("zoom".into(), json!(round(EYE_HEIGHT)));
}

/// The walk resources and the ground rebuild (after the play systems, so the scatter is current).
pub struct WalkPlugin;

impl Plugin for WalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WalkSession>()
            .init_resource::<WalkGround>()
            .add_systems(Update, hex_play::walk::sync.after(PlaySystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_match_the_camera_forward() {
        for yaw in [0.0f32, 0.7, -2.1, 3.0] {
            let t = Transform::default().with_rotation(free_rotation(yaw, 0.0, 0.0));
            let (fwd, right) = axes(yaw);
            assert!((t.forward().xz() - fwd).length() < 1e-5, "{yaw}");
            assert!((t.right().xz() - right).length() < 1e-5, "{yaw}");
            let back = yaw_of_heading(heading_deg(yaw));
            assert!(
                ((back - yaw).rem_euclid(std::f32::consts::TAU))
                    .min((yaw - back).rem_euclid(std::f32::consts::TAU))
                    < 1e-4
            );
        }
        assert!((heading_deg(yaw_of_heading(90.0)) - 90.0).abs() < 1e-4);
        assert_eq!(
            axes(yaw_of_heading(90.0)).0.round(),
            Vec2::X,
            "heading 90 is +X"
        );
    }

    #[test]
    fn a_scripted_step_walks_for_exactly_its_seconds_on_open_ground() {
        // An empty world is all edge: the walker stays put and reports it.
        let ground = WalkGround::default();
        let mut s = WalkSession {
            script: Some((Vec2::Y, false, 0.5)),
            ..Default::default()
        };
        let moved = advance(&mut s, &ground, 1.0 / 60.0);
        assert_eq!(moved, 0.0);
        assert_eq!(s.blocked, Some(Blocked::Edge));
        for _ in 0..40 {
            advance(&mut s, &ground, 1.0 / 60.0);
        }
        assert!(s.script.is_none(), "the step ends after its seconds");
    }

    /// Walks a scripted step frame by frame at 60 Hz and returns the metres covered.
    fn scripted_metres(speed: WalkSpeed, sprint: bool, seconds: f32) -> f32 {
        let (mut left, mut metres) = (seconds, 0.0);
        while left > 1e-6 {
            let (d, rest) = scripted_stride(Vec2::Y, speed.of(sprint), left, 1.0 / 60.0);
            metres += d.length();
            left = rest;
        }
        metres
    }

    #[test]
    fn a_scripted_step_covers_speed_times_seconds_and_sprint_is_twice_walk() {
        let default = WalkSpeed::default();
        assert_eq!(default.walk(), 5.0);
        assert_eq!(default.sprint(), 10.0);
        assert!((scripted_metres(default, false, 1.0) - 5.0).abs() < 1e-4);
        assert!((scripted_metres(default, true, 1.0) - 10.0).abs() < 1e-3);
        let set = WalkSpeed::new(5.0).unwrap();
        assert!((scripted_metres(set, false, 2.0) - 10.0).abs() < 1e-3);
        assert!((scripted_metres(set, true, 0.5) - 5.0).abs() < 1e-4);
        for v in [2.8, 4.3, 8.0] {
            let s = WalkSpeed::new(v).unwrap();
            assert_eq!(
                s.sprint(),
                2.0 * s.walk(),
                "sprint is always twice walk at {v}"
            );
        }
    }

    #[test]
    fn the_walk_speed_setting_clamps_to_its_range() {
        assert_eq!(WalkSpeed::new(1.0).unwrap().walk(), WALK_SPEED);
        assert_eq!(WalkSpeed::new(20.0).unwrap().walk(), WALK_SPEED_MAX);
        assert_eq!(WalkSpeed::new(6.5).unwrap().walk(), 6.5);
        assert_eq!(WalkSpeed::new(f32::NAN), None);
        assert_eq!(WalkSpeed::new(f32::INFINITY), None);
    }

    #[test]
    fn the_state_reports_the_walk_and_sprint_speeds_while_off() {
        let ground = WalkGround::default();
        let mut s = WalkSession::default();
        let v = state_json(&s, &ground);
        assert_eq!(
            (v["on"].clone(), v["walk_speed"].clone()),
            (json!(false), json!(5.0))
        );
        assert_eq!(v["sprint_speed"], json!(10.0));
        s.speed = WalkSpeed::new(7.25).unwrap();
        let v = state_json(&s, &ground);
        assert_eq!(
            (v["walk_speed"].clone(), v["sprint_speed"].clone()),
            (json!(7.25), json!(14.5))
        );
    }

    /// Walking puts the IBL's cubemap on the camera as the sky at the IBL's intensity, follows the
    /// time of day's dimming, and leaving restores the orbit camera's backdrop (no sky).
    #[test]
    fn walking_shows_the_ibl_sky_and_leaving_takes_it_off() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(GameState::new(1));
        world.init_resource::<WalkSession>();
        world.init_resource::<WalkGround>();
        world.init_resource::<Time>();
        let specular: Handle<Image> = Handle::default();
        let cam = world
            .spawn((
                Camera3d::default(),
                Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y),
                Projection::Perspective(PerspectiveProjection::default()),
                EnvironmentMapLight {
                    specular_map: specular.clone(),
                    intensity: 1500.0,
                    ..default()
                },
            ))
            .id();
        let run = |world: &mut World, walk: bool| {
            world.resource_mut::<GameState>().walk_mode = walk;
            world.run_system_once(sync).unwrap();
        };
        run(&mut world, false);
        assert!(world.get::<Skybox>(cam).is_none(), "orbit: the backdrop");
        run(&mut world, true);
        let sky = world.get::<Skybox>(cam).expect("a sky while walking");
        assert_eq!(sky.image.as_ref(), Some(&specular));
        assert_eq!(sky.brightness, 1500.0);
        assert_eq!(sky.rotation, Quat::IDENTITY);
        // Dusk dims the IBL (`hooks::apply_time_of_day`); the sky follows.
        world.get_mut::<EnvironmentMapLight>(cam).unwrap().intensity = 300.0;
        run(&mut world, true);
        assert_eq!(world.get::<Skybox>(cam).unwrap().brightness, 300.0);
        run(&mut world, false);
        assert!(world.get::<Skybox>(cam).is_none(), "orbit again: no sky");
    }
}
