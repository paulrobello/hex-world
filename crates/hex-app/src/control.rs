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
    tilt_shift::TiltShiftSettings,
};
use serde::Deserialize;

use crate::actions::{Action, ActionQueue, Tool};
use crate::game::GameState;
use crate::sim::CarSim;
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
            Command::Undo => Action::Undo,
            Command::Redo => Action::Redo,
            Command::Bulldoze { q, r } => Action::Bulldoze(Axial::new(*q, *r)),
            Command::SetTool { mode } => Action::SetTool(*mode),
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
    mut tilt: ResMut<TiltShiftSettings>,
    mut actions: ResMut<ActionQueue>,
    mut reduced: ResMut<ReducedMotion>,
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
        }
        Command::DumpState { name } => dump_state(&name),
        Command::SetReducedMotion { on } => reduced.0 = on,
        Command::Exit => exit.0 = true,
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

/// Publishes the state read by `hexWorld.state()`/`dumpState()` and `--dump-state`.
pub fn publish_state(
    flags: Res<RunFlags>,
    frames: Res<FrameCount>,
    active: Res<ActiveScenario>,
    queue: Res<CommandQueue>,
    cameras: Query<&Transform, With<Camera3d>>,
    config: Option<Res<hex_render::GateConfig>>,
    gpu: Option<Res<hex_render::GpuLimits>>,
    tilt: Res<TiltShiftSettings>,
    game: Res<GameState>,
    sim: Res<CarSim>,
    (rises, reduced, packs): (Res<PlayRises>, Res<ReducedMotion>, Option<Res<PlayPacks>>),
    views: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
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
    let kit_src = config.map(|c| c.kit_src).unwrap_or_default();
    // `null` until the render device exists (the web renderer starts asynchronously).
    let gpu = gpu.map(|g| {
        serde_json::json!({ "maxSampledTexturesPerShaderStage": g.max_sampled_textures_per_shader_stage })
    });
    let play = active.0 == Scenario::Play;
    let play_state = play.then(|| {
        let mut p = game.state_json();
        sim.extend_state(&mut p);
        extend_play_rise(&mut p, rises.report(), reduced.0);
        extend_play_view(&mut p, cameras.iter().next());
        p
    });
    let state = serde_json::json!({
        "scenario": format!("{:?}", active.0),
        "tiles": if play { game.world.tiles().len() } else { active.0.tile_count(kit_src) },
        "play": play_state,
        "events_pending": bridge::EVENTS.lock().unwrap().len(),
        "gpu": gpu,
        "tilt": { "enabled": tilt.enabled, "strength": tilt.strength, "taps": tilt.taps },
        "frames": frames.0,
        "seed": flags.0.seed,
        "no_anim": flags.0.no_anim,
        "camera": camera,
        "queued": queue.items.len(),
        "packs": packs.as_ref().filter(|_| play).map(|p| {
            p.status().map(|(k, s)| (k.to_owned(), serde_json::json!(s.as_str())))
                .collect::<serde_json::Map<_, _>>()
        }),
        "pack_waiting": packs.as_ref().filter(|_| play).map(|p| {
            p.waiting().iter().map(|(c, tile, packs)| {
                serde_json::json!({ "q": c.q, "r": c.r, "tile": tile, "packs": packs })
            }).collect::<Vec<_>>()
        }),
        "diag": serde_json::from_str::<serde_json::Value>(&bridge::DIAG_JSON.lock().unwrap())
            .unwrap_or(serde_json::Value::Null),
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
                    {"cmd":"tilt","on":false,"strength":0.5}]"#;
        let c = parse_script(s).unwrap();
        assert_eq!(c.len(), 7);
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
    fn parses_play_commands_into_actions() {
        let s = r#"[{"cmd":"select_tile","id":"forest"},{"cmd":"hover","q":1,"r":-2},
                    {"cmd":"rotate","dir":"cw"},{"cmd":"rotate","dir":"ccw"},
                    {"cmd":"place","q":1,"r":0},{"cmd":"undo"},{"cmd":"redo"},
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
