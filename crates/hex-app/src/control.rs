//! Agent command model: scripted camera moves, screenshots, scenario switches, state dumps.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;
use hex_render::{ActiveScenario, Scenario, SceneEntity, setup_scene};
use serde::Deserialize;

use crate::{FrameCount, RunFlags, bridge, capture};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Camera { pos: [f32; 3], look_at: [f32; 3] },
    Screenshot { name: String },
    Wait { frames: u32 },
    Scenario { name: String },
    DumpState { name: String },
    Exit,
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
        Command::DumpState { name } => dump_state(&name),
        Command::Exit => exit.0 = true,
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

/// Publishes the state read by `hexWorld.state()`/`dumpState()` and `--dump-state`.
pub fn publish_state(
    flags: Res<RunFlags>,
    frames: Res<FrameCount>,
    active: Res<ActiveScenario>,
    queue: Res<CommandQueue>,
    cameras: Query<&Transform, With<Camera3d>>,
) {
    let camera = cameras.iter().next().map(|t| {
        serde_json::json!({ "pos": t.translation.to_array(), "forward": t.forward().to_array() })
    });
    let state = serde_json::json!({
        "scenario": format!("{:?}", active.0),
        "tiles": active.0.tile_count(),
        "frames": frames.0,
        "seed": flags.0.seed,
        "no_anim": flags.0.no_anim,
        "camera": camera,
        "queued": queue.items.len(),
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
                    {"cmd":"scenario","name":"gate2000"},{"cmd":"dump_state","name":"s"},{"cmd":"exit"}]"#;
        let c = parse_script(s).unwrap();
        assert_eq!(c.len(), 6);
        assert_eq!(c[1], Command::Wait { frames: 30 });
        assert_eq!(c[5], Command::Exit);
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
    fn queue_runs_in_order() {
        let mut q = CommandQueue::default();
        q.push(Command::Wait { frames: 2 });
        q.push(Command::Exit);
        assert_eq!(q.items.len(), 2);
        assert_eq!(q.items.front(), Some(&Command::Wait { frames: 2 }));
    }
}
