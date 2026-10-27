use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::window::WindowResolution;
use hex_render::{HexRenderPlugin, MeshoptGlbPlugin};

mod actions;
mod app_plugins;
mod args;
mod bridge;
mod camera;
mod capture;
mod control;
#[cfg(not(target_arch = "wasm32"))]
mod exit_guard;
mod frame_cap;
mod game;
#[cfg(not(target_arch = "wasm32"))]
mod offscreen;
mod persist;
mod photo;
mod photo_fx;
mod sim;
#[cfg(not(target_arch = "wasm32"))]
mod thumbs;
mod walk;

#[derive(Resource)]
struct RunFlags(args::Flags);

#[derive(Resource, Default)]
struct FrameCount(u32);

/// READY flips here; diagnostics start their frame-time window on the next frame.
const SHOT_FRAME: u32 = hex_play::diag::READY_FRAME;

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
    #[cfg(not(target_arch = "wasm32"))]
    exit_guard::init(flags.dump_state.as_deref(), flags.save.as_deref());

    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut queue = control::CommandQueue::default();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &flags.script {
        let script = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read script {path}: {e}"))
            .and_then(|s| control::parse_script(&s).map_err(|e| format!("bad script {path}: {e}")));
        match script {
            Ok(cmds) => queue = control::CommandQueue::scripted(cmds),
            Err(e) => {
                eprintln!("hex-world: {e}");
                std::process::exit(2);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    if flags.thumbs {
        queue.hold_input();
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
        // Thumbnails capture the window, and on macOS a window fully behind another app's window
        // captures all black: the small thumbnail window opens behind the active app, so it floats.
        if flags.thumbs {
            window.window_level = bevy::window::WindowLevel::AlwaysOnTop;
        }
    }

    let game = game::GameState::new(flags.seed);
    let cars = sim::CarSim::new(&game.world, &game.catalog);
    let mut app = App::new();
    if let Some(look) = flags.water_look {
        app.insert_resource(look);
    }
    if flags.clock_fixed {
        // 1/60 s rounded up to the nanosecond, so three frames always cover one 50 ms sim tick.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_nanos(16_666_667),
        ))
        .init_resource::<hex_play::boids::FishTrack>();
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
        .add_plugins(frame_cap::FrameCapPlugin(flags.frame_cap))
        .add_plugins(HexRenderPlugin {
            scenario: flags.scenario,
            view: flags.view,
            config: flags.gate,
        })
        .add_plugins(hex_play::HexPlayPlugin {
            fps: flags.fps_on,
            tier_explicit: flags.tier_explicit,
            gpu_timing: flags.gpu_timing,
        })
        .add_plugins(sim::SimPlugin)
        .add_plugins(walk::WalkPlugin)
        .insert_resource(game)
        .insert_resource(cars)
        .insert_resource(hex_play::boids::BoidSeed(flags.seed))
        .insert_resource(flags.debug)
        .insert_resource(RunFlags(flags))
        .insert_resource(queue)
        .init_resource::<FrameCount>()
        .init_resource::<hex_play::play::packs::PackRequests>()
        .add_plugins((
            app_plugins::ActionsPlugin,
            app_plugins::PhotoPlugin,
            app_plugins::PhotoFxPlugin,
            app_plugins::PersistPlugin,
            app_plugins::ControlPlugin,
        ))
        .configure_sets(
            Update,
            (
                app_plugins::AppSet::Frame,
                app_plugins::AppSet::Flags,
                app_plugins::AppSet::Input,
                app_plugins::AppSet::Hover,
                app_plugins::AppSet::Commands,
                app_plugins::AppSet::Apply,
                app_plugins::AppSet::Photo,
                app_plugins::AppSet::Focus,
                app_plugins::AppSet::Walk,
                app_plugins::AppSet::Export,
                app_plugins::AppSet::Effects,
                app_plugins::AppSet::Persist,
                app_plugins::AppSet::Latest,
                app_plugins::AppSet::Packs,
                app_plugins::AppSet::View,
                app_plugins::AppSet::Mirror,
                app_plugins::AppSet::Cars,
                app_plugins::AppSet::State,
                app_plugins::AppSet::Diagnostics,
                app_plugins::AppSet::Exit,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                frame_hooks.in_set(app_plugins::AppSet::Frame),
                timed_flags.in_set(app_plugins::AppSet::Flags),
                camera::pick_hover.in_set(app_plugins::AppSet::Hover),
                game::apply_actions.in_set(app_plugins::AppSet::Apply),
                walk::sync.in_set(app_plugins::AppSet::Walk),
                request_selected_packs.in_set(app_plugins::AppSet::Packs),
                game::sync_view.in_set(app_plugins::AppSet::View),
                sim::mirror_world.in_set(app_plugins::AppSet::Mirror),
                sim::publish_cars.in_set(app_plugins::AppSet::Cars),
                sim::publish_trains.in_set(app_plugins::AppSet::Cars),
                sim::publish_boat_cues.in_set(app_plugins::AppSet::Cars),
                publish_diag.in_set(app_plugins::AppSet::Diagnostics),
            ),
        )
        .add_systems(
            Update,
            sim::draw_networks
                .after(sim::publish_cars)
                .run_if(|d: Res<hex_play::debug::DebugOverlays>| d.networks),
        );

    // Web: the engine keeps its continuous update while the canvas is blurred. Opening a HUD sheet
    // focuses its control, and Bevy's default `WinitSettings::game()` would drop the engine to a
    // 60 Hz reactive update for as long as the sheet is open (half rate on a 120 Hz display,
    // docs/perf/frame-pacing/). The browser throttles hidden tabs itself. Native keeps `game()`:
    // it has no overlay to take focus, so an unfocused native window is one the user left.
    #[cfg(target_arch = "wasm32")]
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    // Native with an uncapped present mode (perf runs) keeps it too: `game()` steps an unfocused
    // window at exactly 60 Hz, and a window opened over SSH or behind another app never gets focus.
    #[cfg(not(target_arch = "wasm32"))]
    if app.world().resource::<RunFlags>().0.present != args::Present::Fifo {
        app.insert_resource(bevy::winit::WinitSettings::continuous());
        frame_cap::set_unfocused_continuous(true);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if app.world().resource::<RunFlags>().0.offscreen {
        app.add_plugins(offscreen::OffscreenPlugin);
    }

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
        capture::request_held(&mut commands, name);
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
    mut requests: ResMut<hex_play::play::packs::PackRequests>,
) {
    if requests.selected.as_deref() != Some(game.selected.as_str()) {
        requests.selected = Some(game.selected.clone());
    }
}

/// Copies the diag snapshot read by `hexWorld.diag()` every 30 frames.
fn publish_diag(frames: Res<FrameCount>, snap: Res<hex_play::diag::DiagSnapshot>) {
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
        capture::request_held(&mut commands, name);
    }
    if flags.0.exit_after_secs.is_some_and(|s| t >= s) {
        exit.0 = true;
    }
}
