use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::window::WindowResolution;
use hex_render::{HexRenderPlugin, MeshoptGlbPlugin};

mod actions;
mod args;
mod bridge;
mod camera;
mod capture;
mod control;
mod game;
mod persist;
mod photo;
mod sim;
#[cfg(not(target_arch = "wasm32"))]
mod thumbs;
mod walk;

#[derive(Resource)]
struct RunFlags(args::Flags);

#[derive(Resource, Default)]
struct FrameCount(u32);

/// READY flips here; diagnostics start their frame-time window on the next frame.
const SHOT_FRAME: u32 = hex_render::diag::READY_FRAME;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let pairs = args::native_pairs();
    #[cfg(target_arch = "wasm32")]
    let pairs = args::web_pairs();
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let (mut flags, warnings) = args::flags_from_pairs(&pairs);
    #[cfg(not(target_arch = "wasm32"))]
    if flags.thumbs {
        thumbs::force_flags(&mut flags);
    }

    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut queue = control::CommandQueue::default();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &flags.script {
        let script = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read script {path}: {e}"))
            .and_then(|s| control::parse_script(&s).map_err(|e| format!("bad script {path}: {e}")));
        match script {
            Ok(cmds) => cmds.into_iter().for_each(|c| queue.push(c)),
            Err(e) => {
                eprintln!("hex-world: {e}");
                std::process::exit(2);
            }
        }
    }

    // `--load <path>` runs before any script command, once the scene is ready.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &flags.load {
        queue.push_front(control::Command::Load {
            path: Some(path.clone()),
        });
    }

    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut window = Window {
        title: "Hex World".into(),
        canvas: Some("#hex-world".into()),
        fit_canvas_to_parent: true,
        ..default()
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((w, h)) = flags.size {
        window.resolution = WindowResolution::new(w, h).with_scale_factor_override(1.0);
        window.resizable = false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use bevy::window::PresentMode;
        window.present_mode = match flags.present {
            args::Present::Fifo => PresentMode::Fifo,
            args::Present::Immediate => PresentMode::Immediate,
            args::Present::NoVsync => PresentMode::AutoNoVsync,
        };
    }

    let game = game::GameState::new(flags.seed);
    let cars = sim::CarSim::new(&game.world, &game.catalog);
    let mut app = App::new();
    if flags.clock_fixed {
        // 1/60 s rounded up to the nanosecond, so three frames always cover one 50 ms sim tick.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_nanos(16_666_667),
        ))
        .init_resource::<hex_render::boids::FishTrack>();
    }
    // Asset sources must be registered before `AssetPlugin` (inside DefaultPlugins) builds.
    app.add_plugins(MeshoptGlbPlugin)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(window),
                    ..default()
                })
                .set(asset_plugin()),
        )
        .add_plugins(HexRenderPlugin {
            scenario: flags.scenario,
            view: flags.view,
            config: flags.gate,
            fps: !flags.fps_off,
            tier_explicit: flags.tier_explicit,
            gpu_timing: flags.gpu_timing,
        })
        .add_plugins(sim::SimPlugin)
        .add_plugins(walk::WalkPlugin)
        .insert_resource(game)
        .insert_resource(cars)
        .insert_resource(hex_render::boids::BoidSeed(flags.seed))
        .insert_resource(RunFlags(flags))
        .insert_resource(queue)
        .init_resource::<actions::ActionQueue>()
        .init_resource::<FrameCount>()
        .init_resource::<control::ExitRequest>()
        .init_resource::<photo::PhotoSettings>()
        .init_resource::<photo::PhotoSession>()
        .init_resource::<photo::PhotoExports>()
        .init_resource::<persist::PersistQueue>()
        .init_resource::<persist::SavedSettings>()
        .init_resource::<hex_render::play::packs::PackRequests>()
        .add_systems(
            Update,
            (
                frame_hooks,
                timed_flags,
                actions::read_input,
                camera::pick_hover,
                control::run_commands,
                game::apply_actions,
                photo::sync,
                walk::sync,
                photo::run_exports,
                persist::run,
                persist::keep_latest,
                request_selected_packs,
                game::sync_view,
                sim::mirror_world,
                sim::publish_cars,
                control::publish_state,
                publish_diag,
                control::exit_when_idle,
            )
                .chain(),
        )
        .add_systems(Last, (control::dump_state_on_exit, persist::save_on_exit));

    #[cfg(not(target_arch = "wasm32"))]
    if app.world().resource::<RunFlags>().0.thumbs {
        app.add_plugins(thumbs::ThumbsPlugin);
    }

    for w in warnings {
        warn!("{w}");
    }
    app.run();
}

/// `cargo run` makes Bevy resolve assets against the crate dir (`crates/hex-app`), so native
/// runs from the repo root point at `./assets` explicitly. The web build serves `assets/`
/// beside `index.html`, and there are no `.meta` files: probing for them would 404 into the
/// console, which the smoke test treats as an error.
fn asset_plugin() -> AssetPlugin {
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut plugin = AssetPlugin {
        meta_check: bevy::asset::AssetMetaCheck::Never,
        ..default()
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(dir) = std::env::current_dir().map(|d| d.join("assets"))
        && dir.is_dir()
    {
        plugin.file_path = dir.to_string_lossy().into_owned();
    }
    plugin
}

fn frame_hooks(
    mut commands: Commands,
    flags: Res<RunFlags>,
    mut frames: ResMut<FrameCount>,
    mut exit: ResMut<control::ExitRequest>,
) {
    frames.0 += 1;
    if frames.0 == SHOT_FRAME {
        *bridge::READY.lock().unwrap() = true;
    }
    if frames.0 == SHOT_FRAME
        && let Some(name) = &flags.0.shot
    {
        capture::request(&mut commands, name);
    }
    if frames.0 == SHOT_FRAME
        && let Some(scale) = flags.0.photo_export
    {
        control::INBOX
            .lock()
            .unwrap()
            .push(control::Command::PhotoExport {
                scale,
                name: format!("photo-{scale}x"),
            });
    }
    if flags.0.exit_after_frames.is_some_and(|n| frames.0 >= n) {
        exit.0 = true;
    }
}

/// Hands the selected tile to the kit streamer, which requests its packs at once (selection does
/// not change the render view until a slot is hovered).
fn request_selected_packs(
    game: Res<game::GameState>,
    mut requests: ResMut<hex_render::play::packs::PackRequests>,
) {
    if requests.selected.as_deref() != Some(game.selected.as_str()) {
        requests.selected = Some(game.selected.clone());
    }
}

/// Copies the diag snapshot read by `hexWorld.diag()` every 30 frames.
fn publish_diag(frames: Res<FrameCount>, snap: Res<hex_render::diag::DiagSnapshot>) {
    if frames.0.is_multiple_of(30) && !snap.json.is_empty() {
        *bridge::DIAG_JSON.lock().unwrap() = snap.json.clone();
    }
}

/// Wall-clock flags: `--screenshot-after` and `--exit-after-seconds`.
fn timed_flags(
    mut commands: Commands,
    flags: Res<RunFlags>,
    time: Res<Time<Real>>,
    mut shot_taken: Local<bool>,
    mut exit: ResMut<control::ExitRequest>,
) {
    let t = time.elapsed_secs();
    if !*shot_taken && flags.0.screenshot_after_secs.is_some_and(|s| t >= s) {
        *shot_taken = true;
        let name = flags.0.shot.as_deref().unwrap_or("screenshot-after");
        capture::request(&mut commands, name);
    }
    if flags.0.exit_after_secs.is_some_and(|s| t >= s) {
        exit.0 = true;
    }
}
