//! Agent command model: scripted camera moves, screenshots, scenario switches, state dumps.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;
use hex_core::{Axial, RotateDir};
use hex_render::{
    ActiveScenario, Scenario, SceneEntity,
    play::{
        packs::PlayPacks,
        rise::{PlayRises, ReducedMotion},
    },
    setup_scene,
    tilt_shift::{TiltPinned, TiltShiftSettings},
};
use serde::Deserialize;

use crate::actions::{Action, ActionQueue, Tool};
use crate::game::GameState;
use crate::persist::{PersistQueue, SavedSettings};
use crate::photo::{Dof, PhotoExports, PhotoPatch, PhotoSession, PhotoSettings};
use crate::sim::CarSim;
use crate::walk::{WalkRequest, WalkSession, WalkSpeed};
use crate::{FrameCount, RunFlags, bridge, capture};

/// `rotate` direction: `cw` is R (clockwise viewed from above), `ccw` is Shift+R.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spin {
    Cw,
    Ccw,
}

/// `toggle` target: the B, Tab and P bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Toggle {
    Bulldoze,
    Inventory,
    Photo,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Camera {
        pos: [f32; 3],
        look_at: [f32; 3],
    },
    Screenshot {
        name: String,
    },
    Wait {
        frames: u32,
    },
    Scenario {
        name: String,
    },
    /// Runtime tilt-shift toggle and strength (the `T` key and the strength slider's script form).
    Tilt {
        on: Option<bool>,
        strength: Option<f32>,
    },
    DumpState {
        name: String,
    },
    /// Quality tier at runtime (`auto`, `low`, `medium`, `high`, `ultra`): rebuilds the scene at the
    /// new tier with the camera kept (`hex_render::tier`); `auto` returns to the detected tier. The
    /// choice is the save's quality from then on. The Settings sheet sends this.
    SetTier {
        tier: String,
    },
    /// Look preset at runtime (`aces`, `tony`, `rich-1`, `rich-2`, `rich-3`; `hex_render::grade`):
    /// the cameras' tone mapper, exposure and grading and the backdrop switch with no rebuild. The
    /// Settings sheet sends this; the overlay keeps the choice per viewer, not in the world save.
    SetGrade {
        grade: String,
    },
    /// Grass density at runtime as a share of the Normal density (0 off, 1 Normal, 1.5 the most;
    /// clamped, rounded to whole percent; `hex_render::play::grass`): each tile's blade mesh swaps
    /// from its seeded set with no scene rebuild. The Settings sheet's slider sends this; the
    /// overlay keeps the choice per viewer, not in the world save.
    SetGrass {
        density: f32,
    },
    /// Reduced motion (`prefers-reduced-motion` or the Settings override, sent by the overlay):
    /// the staged rise and every affordance transition become instant cuts, with no breathing,
    /// dust or camera shake.
    SetReducedMotion {
        on: bool,
    },
    Exit,
    // Play-scenario commands: each enqueues the same `Action` its key or mouse binding does.
    SelectTile {
        id: String,
    },
    /// Hover a cell, as the pointer does (scripts have no mouse).
    Hover {
        q: i32,
        r: i32,
    },
    Rotate {
        dir: Spin,
    },
    /// Place the selected tile at the current ghost rotation; a cell other than the hovered one
    /// is hovered first, which auto-rotates.
    Place {
        q: i32,
        r: i32,
    },
    /// Pin a free slot (U4), as the first click on it does: the ghost, its rotation and the
    /// inventory stay there while the pointer moves. Refused for a cell that is not a free slot.
    Pin {
        q: i32,
        r: i32,
    },
    /// Clear the pinned slot, as Esc does.
    Unpin,
    /// Swap the placed tile at a cell for the selected tile in one undoable step, as Ctrl or
    /// Cmd+click does: the ghost's rotation where it fits, else the first rotation that fits.
    Replace {
        q: i32,
        r: i32,
    },
    Undo,
    Redo,
    Bulldoze {
        q: i32,
        r: i32,
    },
    SetTool {
        mode: Tool,
    },
    Toggle {
        what: Toggle,
    },
    /// Photo-mode panel values (PRD §4.6), each clamped to its slider; omitted fields keep theirs.
    /// `dof` is `off`, `tilt` (the tilt-shift kernel) or `manual`; `focus` is metres, `aperture`
    /// an f-number, `exposure` stops brighter, `fov` and `roll` degrees.
    Photo {
        dof: Option<Dof>,
        focus: Option<f32>,
        aperture: Option<f32>,
        exposure: Option<f32>,
        fov: Option<f32>,
        roll: Option<f32>,
        pause_agents: Option<bool>,
    },
    /// Renders the game camera offscreen at `scale` (1, 2 or 4) times the window's logical size and
    /// saves a PNG: `target/shots/<name>.png` natively, `hexWorld.takeShot(name)` on the web. Ends
    /// with a `photoExported` or `photoExportFailed` event.
    PhotoExport {
        scale: u32,
        name: String,
    },
    /// Encodes the world as `.hexworld` bytes after every command queued before it (PRD §12.2).
    /// `tag` is echoed in the `saved` event and keys `hexWorld.takeSave(tag)` on the web; natively
    /// `path` writes the file. `volumes` are the web audio layer's, which the engine does not own.
    Save {
        tag: Option<String>,
        path: Option<String>,
        volumes: Option<crate::persist::Volumes>,
    },
    /// Replaces the world with a save: natively the file at `path`; on the web the bytes passed to
    /// `hexWorld.load(bytes)`, which queues this command with no path.
    Load {
        path: Option<String>,
    },
    /// Resets to the start world with an empty history, keeping the settings.
    NewWorld,
    /// First-person walk mode (PRD P8): `on` enters or leaves it (omitted keeps it); the HUD sends
    /// `pointer_lock` when the page gains or loses the pointer lock, so mouse motion looks with no
    /// button held.
    Walk {
        on: Option<bool>,
        pointer_lock: Option<bool>,
    },
    /// Walk mode: stand on the ground at world `(x, z)` metres, facing `heading` (degrees clockwise
    /// from -Z viewed from above) and `pitch` (degrees, up positive) when given. Refused, with
    /// `walk.blocked` saying why, where nothing stands.
    WalkTo {
        x: f32,
        z: f32,
        heading: Option<f32>,
        pitch: Option<f32>,
    },
    /// Walk mode: walk as if the keys were held for `seconds` of game time, `forward` and `right`
    /// in -1..1 (`sprint` at twice the walk speed), through the same collision as the keys. `jog`
    /// is the field's old name, still accepted.
    WalkStep {
        forward: f32,
        right: f32,
        seconds: f32,
        #[serde(alias = "jog")]
        sprint: Option<bool>,
    },
    /// The walk speed setting in m/s, clamped to 2.8..=8.0 (sprint is always twice it). Applies at
    /// once, also mid-walk. The Settings sheet's World tab sends it and keeps it per viewer.
    SetWalkSpeed {
        speed: f32,
    },
    /// Walk mode: look toward `heading` and `pitch` (degrees); either may be omitted.
    WalkLook {
        heading: Option<f32>,
        pitch: Option<f32>,
    },
}

impl Command {
    /// The input action a play command enqueues; `None` for the other commands.
    pub fn action(&self) -> Option<Action> {
        Some(match self {
            Command::SelectTile { id } => Action::SelectTile(id.clone()),
            Command::Hover { q, r } => Action::Hover(Some(Axial::new(*q, *r))),
            Command::Rotate { dir } => Action::Rotate(match dir {
                Spin::Cw => RotateDir::Forward,
                Spin::Ccw => RotateDir::Back,
            }),
            Command::Place { q, r } => Action::Place(Axial::new(*q, *r)),
            Command::Pin { q, r } => Action::Pin(Axial::new(*q, *r)),
            Command::Unpin => Action::Unpin,
            Command::Replace { q, r } => Action::Replace(Axial::new(*q, *r)),
            Command::Undo => Action::Undo,
            Command::Redo => Action::Redo,
            Command::Bulldoze { q, r } => Action::Bulldoze(Axial::new(*q, *r)),
            Command::SetTool { mode } => Action::SetTool(*mode),
            Command::Walk { on: Some(on), .. } => Action::SetWalk(*on),
            Command::Toggle { what } => match what {
                Toggle::Bulldoze => Action::ToggleBulldoze,
                Toggle::Inventory => Action::ToggleInventory,
                Toggle::Photo => Action::TogglePhoto,
            },
            _ => return None,
        })
    }
}

/// Parses a JSON array of commands; errors name the offending index.
pub fn parse_script(json: &str) -> Result<Vec<Command>, String> {
    let items: Vec<serde_json::Value> = serde_json::from_str(json)
        .map_err(|e| format!("script must be a JSON array of commands: {e}"))?;
    items
        .into_iter()
        .enumerate()
        .map(|(i, v)| serde_json::from_value(v).map_err(|e| format!("command {i}: {e}")))
        .collect()
}

/// Commands pushed from outside the ECS (the wasm `hex_world_cmd` export), drained each frame.
pub static INBOX: Mutex<Vec<Command>> = Mutex::new(Vec::new());

#[derive(Resource, Default)]
pub struct CommandQueue {
    items: VecDeque<Command>,
    wait: u32,
}

impl CommandQueue {
    // The web feeds commands through `INBOX` instead.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn push(&mut self, c: Command) {
        self.items.push_back(c);
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn push_front(&mut self, c: Command) {
        self.items.push_front(c);
    }
}

/// Set by any exit path; `exit_when_idle` sends `AppExit` once pending screenshots have landed.
#[derive(Resource, Default)]
pub struct ExitRequest(pub bool);

/// Runs one command per frame once the scene is ready. Bad commands are logged and skipped.
pub fn run_commands(
    mut commands: Commands,
    mut queue: ResMut<CommandQueue>,
    mut exit: ResMut<ExitRequest>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
    scene: Query<Entity, With<SceneEntity>>,
    (mut tilt, mut pinned): (ResMut<TiltShiftSettings>, ResMut<TiltPinned>),
    mut actions: ResMut<ActionQueue>,
    mut reduced: ResMut<ReducedMotion>,
    (mut photo, mut exports): (ResMut<PhotoSettings>, ResMut<PhotoExports>),
    (mut persist, mut saved): (ResMut<PersistQueue>, ResMut<SavedSettings>),
    mut walk: ResMut<WalkSession>,
) {
    queue.items.extend(INBOX.lock().unwrap().drain(..));
    if !*bridge::READY.lock().unwrap() {
        return;
    }
    if queue.wait > 0 {
        queue.wait -= 1;
        return;
    }
    let Some(cmd) = queue.items.pop_front() else {
        return;
    };
    info!("command: {cmd:?}");
    if let Command::Walk {
        pointer_lock: Some(lock),
        ..
    } = cmd
    {
        walk.pointer_lock = lock;
    }
    if let Some(a) = cmd.action() {
        actions.0.push_back(a);
        return;
    }
    match cmd {
        Command::Camera { pos, look_at } => {
            let t = Transform::from_translation(pos.into()).looking_at(look_at.into(), Vec3::Y);
            if cameras.is_empty() {
                warn!("camera command: no camera");
            }
            for mut c in &mut cameras {
                *c = t;
            }
        }
        Command::Screenshot { name } => capture::request(&mut commands, &name),
        Command::Wait { frames } => queue.wait = frames,
        Command::Scenario { name } => match Scenario::parse(&name) {
            Some(s) => {
                // Children also carry `SceneEntity` and go with their parent; skip the repeats.
                for e in &scene {
                    commands.entity(e).try_despawn();
                }
                commands.insert_resource(ActiveScenario(s));
                commands.run_system_cached(setup_scene);
            }
            None => warn!("scenario command: unknown scenario '{name}', skipped"),
        },
        Command::Tilt { on, strength } => {
            tilt.enabled = on.unwrap_or(tilt.enabled);
            if let Some(v) = strength.filter(|v| (0.0..=1.0).contains(v)) {
                tilt.strength = v;
            }
            if on.is_some() {
                pinned.0 = true;
            }
        }
        Command::DumpState { name } => dump_state(&name),
        Command::SetTier { tier } => match crate::persist::quality_setting(&tier) {
            Some(q) => {
                saved.quality = q;
                hex_render::tier::request(
                    &mut commands,
                    crate::persist::quality_tier(q),
                    "set_tier command".into(),
                    false,
                );
            }
            None => warn!("set_tier command: unknown tier '{tier}', skipped"),
        },
        Command::SetGrade { grade } => match hex_render::Grade::parse(&grade) {
            // Photo mode re-applies its entry exposure every frame, so it shifts with the grade.
            Some(g) => commands.queue(move |w: &mut World| {
                let shift = hex_render::grade::apply(w, g);
                if let Some(mut s) = w.get_resource_mut::<PhotoSession>()
                    && let Some(e) = s.saved.as_mut().and_then(|snap| snap.ev100.as_mut())
                {
                    *e += shift;
                }
            }),
            None => warn!("set_grade command: unknown grade '{grade}', skipped"),
        },
        Command::SetGrass { density } => {
            match hex_render::play::grass::Density::from_share(density) {
                Some(d) => commands.queue(move |w: &mut World| {
                    w.resource_mut::<hex_render::GateConfig>().grass = d;
                }),
                None => warn!("set_grass command: density {density} is not a number, skipped"),
            }
        }
        Command::SetReducedMotion { on } => reduced.0 = on,
        Command::Photo {
            dof,
            focus,
            aperture,
            exposure,
            fov,
            roll,
            pause_agents,
        } => photo.apply(&PhotoPatch {
            dof,
            focus,
            aperture,
            exposure,
            fov,
            roll,
            pause_agents,
        }),
        Command::PhotoExport { scale, name } => exports.push(scale, name),
        Command::Exit => exit.0 = true,
        c @ (Command::Save { .. } | Command::Load { .. } | Command::NewWorld) => {
            persist.0.push_back(c)
        }
        Command::Walk { .. } => {}
        Command::WalkTo {
            x,
            z,
            heading,
            pitch,
        } => walk.requests.push_back(WalkRequest::To {
            x,
            z,
            heading,
            pitch,
        }),
        Command::WalkStep {
            forward,
            right,
            seconds,
            sprint,
        } => walk.requests.push_back(WalkRequest::Step {
            forward,
            right,
            seconds,
            sprint: sprint.unwrap_or(false),
        }),
        Command::SetWalkSpeed { speed } => match WalkSpeed::new(speed) {
            Some(s) => walk.speed = s,
            None => warn!("set_walk_speed command: speed {speed} is not a number, skipped"),
        },
        Command::WalkLook { heading, pitch } => walk
            .requests
            .push_back(WalkRequest::Look { heading, pitch }),
        _ => unreachable!("play commands return through `action`"),
    }
}

fn dump_state(name: &str) {
    let json = bridge::STATE_JSON.lock().unwrap().clone();
    info!("dump_state {name}: {json}");
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = format!("{}/{name}.json", capture::SHOT_DIR);
        match std::fs::create_dir_all(capture::SHOT_DIR).and_then(|_| std::fs::write(&path, json)) {
            Ok(()) => info!("wrote {path}"),
            Err(e) => warn!("cannot write {path}: {e}"),
        }
    }
}

/// Adds the staged rise to the `play` state: `rising` (tiles mid-rise, by cell) and `reduced_motion`.
pub fn extend_play_rise(
    play: &mut serde_json::Value,
    rising: Vec<(Axial, String, &'static str, f32)>,
    reduced_motion: bool,
) {
    let serde_json::Value::Object(map) = play else {
        return;
    };
    let rising: Vec<serde_json::Value> = rising
        .into_iter()
        .map(|(c, tile, stage, t)| {
            serde_json::json!({ "q": c.q, "r": c.r, "tile": tile, "stage": stage,
                                "t": (f64::from(t) * 1e3).round() / 1e3 })
        })
        .collect();
    map.insert("rising".into(), serde_json::json!(rising));
    map.insert("reduced_motion".into(), serde_json::json!(reduced_motion));
}

/// Adds the audio layer's view to the `play` state: `focus`, the ground point the camera orbits
/// (`camera::focus`), and `zoom`, the camera's distance from it in metres. Both are `null` without a camera.
pub fn extend_play_view(play: &mut serde_json::Value, camera: Option<&Transform>) {
    let serde_json::Value::Object(map) = play else {
        return;
    };
    let round = |v: f32| (f64::from(v) * 1e3).round() / 1e3;
    let (focus, zoom) = match camera {
        Some(t) => {
            let f = crate::camera::focus(t);
            (
                serde_json::json!([round(f.x), round(f.y), round(f.z)]),
                serde_json::json!(round(t.translation.distance(f))),
            )
        }
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    map.insert("focus".into(), focus);
    map.insert("zoom".into(), zoom);
}

/// `state().grade`: the active look preset, the default one, and every preset with its one-line
/// description.
pub fn grade_json(active: hex_render::Grade) -> serde_json::Value {
    let presets: Vec<serde_json::Value> = hex_render::Grade::ALL
        .iter()
        .map(|g| serde_json::json!({ "name": g.name(), "detail": g.detail() }))
        .collect();
    serde_json::json!({
        "name": active.name(),
        "default": hex_render::Grade::default().name(),
        "presets": presets,
    })
}

/// Publishes the state read by `hexWorld.state()`/`dumpState()` and `--dump-state`.
pub fn publish_state(
    flags: Res<RunFlags>,
    frames: Res<FrameCount>,
    active: Res<ActiveScenario>,
    queue: Res<CommandQueue>,
    cameras: Query<&Transform, With<Camera3d>>,
    config: Option<Res<hex_render::GateConfig>>,
    gpu: Option<Res<hex_render::GpuLimits>>,
    (tilt, tilt_chosen): (Res<TiltShiftSettings>, Res<TiltPinned>),
    game: Res<GameState>,
    sim: Res<CarSim>,
    (rises, reduced, packs, ghost_drawn): (
        Res<PlayRises>,
        Res<ReducedMotion>,
        Option<Res<PlayPacks>>,
        Option<Res<hex_render::play::ghost::GhostDrawn>>,
    ),
    views: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    (boids, scatter, tier, ghost_report, grass): (
        Option<Res<hex_render::boids::BoidReport>>,
        Option<Res<hex_render::play::scatter::ScatterReport>>,
        Res<hex_render::tier::TierChoice>,
        Option<Res<hex_render::play::ghost::GhostReport>>,
        Option<Res<hex_render::play::grass::GrassReport>>,
    ),
    (photo, session, exports, paused, kit_failures): (
        Res<PhotoSettings>,
        Res<PhotoSession>,
        Res<PhotoExports>,
        Res<hex_render::AgentsPaused>,
        Option<Res<hex_render::KitLoadFailures>>,
    ),
    (walk, walk_ground): (Res<WalkSession>, Res<hex_render::walk::WalkGround>),
) {
    *bridge::VIEW.lock().unwrap() = views.iter().find(|(c, _)| c.is_active).and_then(|(c, gt)| {
        Some(bridge::View {
            clip_from_world: c.clip_from_view() * gt.to_matrix().inverse(),
            origin: c.logical_viewport_rect()?.min,
            size: c.logical_viewport_size()?,
        })
    });
    let camera = cameras.iter().next().map(|t| {
        serde_json::json!({ "pos": t.translation.to_array(), "forward": t.forward().to_array() })
    });
    let kit_src = config.as_ref().map(|c| c.kit_src).unwrap_or_default();
    let grade = grade_json(config.map(|c| c.grade).unwrap_or_default());
    let gpu_limits = gpu.as_deref().copied();
    // `null` until the render device exists (the web renderer starts asynchronously).
    let gpu = gpu.map(|g| {
        serde_json::json!({ "maxSampledTexturesPerShaderStage": g.max_sampled_textures_per_shader_stage,
                            "maxTextureDimension2D": g.max_texture_dimension_2d })
    });
    let play = active.0 == Scenario::Play;
    let play_state = play.then(|| {
        let mut p = game.state_json();
        sim.extend_state(&mut p);
        extend_play_rise(&mut p, rises.report(), reduced.0);
        extend_play_view(&mut p, cameras.iter().next());
        crate::walk::extend_play_view(&mut p, &walk);
        p
    });
    let state = serde_json::json!({
        "scenario": format!("{:?}", active.0),
        "tiles": if play { game.world.tiles().len() } else { active.0.tile_count(kit_src) },
        "play": play_state,
        "events_pending": bridge::EVENTS.lock().unwrap().len(),
        "gpu": gpu,
        "tilt": { "enabled": tilt.enabled, "strength": tilt.strength, "taps": tilt.taps, "chosen": tilt_chosen.0 },
        "tier": tier.json(),
        "grade": grade,
        "photo": crate::photo::state_json(&photo, &session, &exports, paused.0, sim.ticks(), gpu_limits.as_ref()),
        "walk": crate::walk::state_json(&walk, &walk_ground),
        "frames": frames.0,
        "seed": flags.0.seed,
        "no_anim": flags.0.no_anim,
        "camera": camera,
        "queued": queue.items.len(),
        "packs": packs.as_ref().filter(|_| play).map(|p| {
            p.status().map(|(k, s)| (k.to_owned(), serde_json::json!(s.as_str())))
                .collect::<serde_json::Map<_, _>>()
        }),
        "ghost_meshes": ghost_drawn.as_ref().filter(|_| play).map(|g| g.0),
        "ghost": ghost_report.filter(|_| play)
            .and_then(|g| serde_json::from_str::<serde_json::Value>(&g.0).ok()),
        "pack_waiting": packs.as_ref().filter(|_| play).map(|p| {
            p.waiting().iter().map(|(c, tile, packs)| {
                serde_json::json!({ "q": c.q, "r": c.r, "tile": tile, "packs": packs })
            }).collect::<Vec<_>>()
        }),
        "diag": serde_json::from_str::<serde_json::Value>(&bridge::DIAG_JSON.lock().unwrap())
            .unwrap_or(serde_json::Value::Null),
        "boids": boids.and_then(|b| serde_json::from_str::<serde_json::Value>(&b.0).ok()),
        "scatter": scatter.filter(|_| play)
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s.0).ok()),
        "grass": grass.and_then(|g| serde_json::from_str::<serde_json::Value>(&g.0).ok()),
        "assets": { "failed": kit_failures.map(|f| f.list()).unwrap_or_default() },
    });
    *bridge::STATE_JSON.lock().unwrap() = state.to_string();
}

pub fn exit_when_idle(exit: Res<ExitRequest>, mut out: MessageWriter<AppExit>) {
    if exit.0 && capture::pending() == 0 {
        out.write(AppExit::Success);
    }
}

/// Writes `--dump-state` on every exit path, including closing the window.
pub fn dump_state_on_exit(flags: Res<RunFlags>, mut exits: MessageReader<AppExit>) {
    if exits.read().next().is_none() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &flags.0.dump_state {
        let json = bridge::STATE_JSON.lock().unwrap().clone();
        match std::fs::write(path, json) {
            Ok(()) => info!("wrote {path}"),
            Err(e) => warn!("cannot write {path}: {e}"),
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = &flags;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_command() {
        let s = r#"[{"cmd":"camera","pos":[0,90,90],"look_at":[0,0,0]},
                    {"cmd":"wait","frames":30},{"cmd":"screenshot","name":"a"},
                    {"cmd":"scenario","name":"gate2000"},{"cmd":"dump_state","name":"s"},{"cmd":"exit"},
                    {"cmd":"tilt","on":false,"strength":0.5},{"cmd":"set_tier","tier":"low"},
                    {"cmd":"photo","dof":"manual","focus":12.5},{"cmd":"photo_export","scale":4,"name":"p"}]"#;
        let c = parse_script(s).unwrap();
        assert_eq!(c.len(), 10);
        assert_eq!(c[7], Command::SetTier { tier: "low".into() });
        assert_eq!(c[7].action(), None);
        assert_eq!(
            c[8],
            Command::Photo {
                dof: Some(Dof::Manual),
                focus: Some(12.5),
                aperture: None,
                exposure: None,
                fov: None,
                roll: None,
                pause_agents: None,
            }
        );
        assert_eq!(
            c[9],
            Command::PhotoExport {
                scale: 4,
                name: "p".into()
            }
        );
        assert_eq!(
            c[6],
            Command::Tilt {
                on: Some(false),
                strength: Some(0.5)
            }
        );
        assert_eq!(c[1], Command::Wait { frames: 30 });
        assert_eq!(c[5], Command::Exit);
    }

    #[test]
    fn parses_walk_step_sprint_with_the_old_jog_name_and_the_walk_speed() {
        let c = parse_script(
            r#"[{"cmd":"walk_step","forward":1,"right":0,"seconds":2,"sprint":true},
                {"cmd":"walk_step","forward":1,"right":0,"seconds":2,"jog":true},
                {"cmd":"walk_step","forward":-1,"right":0,"seconds":1},
                {"cmd":"set_walk_speed","speed":4.5}]"#,
        )
        .unwrap();
        let step = |sprint| Command::WalkStep {
            forward: 1.0,
            right: 0.0,
            seconds: 2.0,
            sprint,
        };
        assert_eq!(c[0], step(Some(true)));
        assert_eq!(c[1], step(Some(true)), "the old field name still parses");
        assert!(matches!(c[2], Command::WalkStep { sprint: None, .. }));
        assert_eq!(c[3], Command::SetWalkSpeed { speed: 4.5 });
        assert_eq!(c[3].action(), None);
    }

    #[test]
    fn parses_play_commands_into_actions() {
        let s = r#"[{"cmd":"select_tile","id":"forest"},{"cmd":"hover","q":1,"r":-2},
                    {"cmd":"rotate","dir":"cw"},{"cmd":"rotate","dir":"ccw"},
                    {"cmd":"place","q":1,"r":0},{"cmd":"replace","q":1,"r":0},
                    {"cmd":"undo"},{"cmd":"redo"},
                    {"cmd":"bulldoze","q":0,"r":1},{"cmd":"set_tool","mode":"bulldoze"},
                    {"cmd":"toggle","what":"inventory"},{"cmd":"toggle","what":"photo"},
                    {"cmd":"toggle","what":"bulldoze"}]"#;
        let actions: Vec<_> = parse_script(s)
            .unwrap()
            .iter()
            .map(|c| c.action().expect("play command"))
            .collect();
        assert_eq!(
            actions,
            [
                Action::SelectTile("forest".into()),
                Action::Hover(Some(Axial::new(1, -2))),
                Action::Rotate(RotateDir::Forward),
                Action::Rotate(RotateDir::Back),
                Action::Place(Axial::new(1, 0)),
                Action::Replace(Axial::new(1, 0)),
                Action::Undo,
                Action::Redo,
                Action::Bulldoze(Axial::new(0, 1)),
                Action::SetTool(Tool::Bulldoze),
                Action::ToggleInventory,
                Action::TogglePhoto,
                Action::ToggleBulldoze,
            ]
        );
        assert_eq!(Command::Exit.action(), None);
        assert_eq!(Command::Wait { frames: 1 }.action(), None);
    }

    #[test]
    fn bad_play_commands_fail_at_parse_time() {
        for bad in [
            r#"[{"cmd":"rotate","dir":"left"}]"#,
            r#"[{"cmd":"set_tool","mode":"shovel"}]"#,
            r#"[{"cmd":"place","q":1}]"#,
            r#"[{"cmd":"hover","q":"a","r":0}]"#,
            r#"[{"cmd":"toggle","what":"map"}]"#,
        ] {
            assert!(parse_script(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn unknown_command_reports_index() {
        let e = parse_script(r#"[{"cmd":"wait","frames":1},{"cmd":"explode"}]"#).unwrap_err();
        assert!(e.contains("1"), "{e}");
    }

    #[test]
    fn malformed_json_is_error_not_panic() {
        assert!(parse_script("not json").is_err());
        assert!(
            parse_script(r#"{"cmd":"exit"}"#).is_err(),
            "top level must be an array"
        );
    }

    #[test]
    fn play_view_reports_focus_and_zoom() {
        let t =
            Transform::from_xyz(30.0, 60.0, 80.0).looking_at(Vec3::new(30.0, 0.0, 0.0), Vec3::Y);
        let mut p = serde_json::json!({});
        extend_play_view(&mut p, Some(&t));
        assert_eq!(p["focus"], serde_json::json!([30.0, 0.0, 0.0]));
        assert_eq!(p["zoom"], serde_json::json!(100.0));
        extend_play_view(&mut p, None);
        assert!(p["focus"].is_null() && p["zoom"].is_null());
    }

    #[test]
    fn queue_runs_in_order() {
        let mut q = CommandQueue::default();
        q.push(Command::Wait { frames: 2 });
        q.push(Command::Exit);
        assert_eq!(q.items.len(), 2);
        assert_eq!(q.items.front(), Some(&Command::Wait { frames: 2 }));
    }
}
