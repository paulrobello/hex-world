//! `--thumbs`: renders the per-rotation tile thumbnails the Legend inventory shows (PRD §5.3).
//!
//! Each tile is drawn alone at the origin through the play scenario's own path: this module only
//! writes [`PlayView`], and `hex_render::play` spawns the tile's kit pieces or placeholders, so the
//! thumbnails follow the game as kit pieces land. Rotations whose world-edge sockets equal an
//! earlier rotation's reuse that image. The run writes raw frames against a key colour plus
//! `plan.json` to [`RAW_DIR`]; `scripts/thumbs/build.py` keys, downsamples and encodes them.
use bevy::prelude::*;
use bevy::render::view::DebandDither;
use hex_core::{Axial, EdgeSocket, TileCatalog, TileDef};
use hex_render::play::{PlayKit, PlayTile, PlayView, rise::ReducedMotion};
use hex_render::{PostMode, Scenario, TiltMode};

use crate::{args::Flags, capture, control::ExitRequest, game};

/// `target/shots/thumbs`: `capture` writes `target/shots/<name>.png`.
pub const RAW_DIR: &str = "target/shots/thumbs";
const SHOT_PREFIX: &str = "thumbs";

/// Window size: 4x the 96 px thumbnail (48 CSS px Legend row glyph at 2x), box-filtered down by
/// `build.py`. Post is off (no MSAA or TAA), so the supersample is the anti-aliasing.
pub const RENDER_SIZE: (u32, u32) = (384, 384);

/// Background, keyed to alpha by `build.py`. No terrain or kit material is magenta.
const KEY: Color = Color::srgb(1.0, 0.0, 1.0);

/// Camera: on the play start camera's side (+Z, edge 0 on the right), pitched near top-down, with
/// a narrow field of view so the far and near edges keep nearly the same scale. One pose for every
/// tile, so all thumbnails share one scale. The hex fills about 84% of the width and the frame
/// keeps a 12% top margin for pieces taller than today's oak and two-storey house.
const PITCH_DEG: f32 = 58.0;
const FOV_DEG: f32 = 20.0;
const DISTANCE: f32 = 96.0;
const LOOK_AT: Vec3 = Vec3::new(0.0, 1.5, 1.5);

/// Frames before the first stage. Starting at a fixed frame keeps every capture on the same frame
/// number from run to run, so `--clock fixed` time-driven materials match.
const START_FRAME: u32 = 240;
/// Each tile is shown once before any capture, so pipelines compile and textures load first.
const WARM_FRAMES: u32 = 30;
/// Frames a captured tile is shown before its screenshot is requested.
const SETTLE_FRAMES: u32 = 20;
/// Frames held after the request, so the capture never sees the next tile.
const HOLD_FRAMES: u32 = 3;

/// The thumbnail mode's fixed render settings: the play scenario with no post stack, tilt-shift
/// or FPS counter, the fixed clock, and the supersampled window.
pub fn force_flags(f: &mut Flags) {
    f.scenario = Scenario::Play;
    f.gate.post = PostMode::None;
    f.gate.tilt = TiltMode::Off;
    f.fps_off = true;
    f.clock_fixed = true;
    f.size = Some(RENDER_SIZE);
    f.shot = None;
    f.script = None;
    f.exit_after_frames = None;
    f.exit_after_secs = None;
    f.screenshot_after_secs = None;
}

/// World-edge sockets of `def` placed at `rotation`.
fn signature(def: &TileDef, rotation: u8) -> [&EdgeSocket; 6] {
    std::array::from_fn(|w| def.edge(w as u8, rotation))
}

/// For each rotation 0..6, the lowest rotation with the same world-edge sockets: the image it uses.
pub fn canonical_rotations(def: &TileDef) -> [u8; 6] {
    std::array::from_fn(|k| {
        let k = k as u8;
        (0..k)
            .find(|&j| signature(def, j) == signature(def, k))
            .unwrap_or(k)
    })
}

/// File stem of the image for `tile` at canonical rotation `k`.
pub fn stem(tile: &str, k: u8) -> String {
    format!("{tile}-r{k}")
}

/// `plan.json`: the catalog's tiles in order, each with its six canonical rotations.
pub fn plan_json(catalog: &TileCatalog) -> serde_json::Value {
    let tiles: Vec<serde_json::Value> = catalog
        .tiles()
        .iter()
        .map(|t| serde_json::json!({ "id": t.id, "rotations": canonical_rotations(t) }))
        .collect();
    serde_json::json!({
        "render_size": [RENDER_SIZE.0, RENDER_SIZE.1],
        "camera": {
            "pitch_deg": PITCH_DEG,
            "fov_deg": FOV_DEG,
            "distance_m": DISTANCE,
            "look_at": [LOOK_AT.x, LOOK_AT.y, LOOK_AT.z],
        },
        "tiles": tiles,
    })
}

pub fn camera_transform() -> Transform {
    let pitch = PITCH_DEG.to_radians();
    let eye = LOOK_AT + Vec3::new(0.0, pitch.sin(), pitch.cos()) * DISTANCE;
    Transform::from_translation(eye).looking_at(LOOK_AT, Vec3::Y)
}

struct Stage {
    tile: String,
    rotation: u8,
    /// Capture name, or `None` for a warm-up stage.
    shot: Option<String>,
}

#[derive(Resource)]
struct ThumbRun {
    stages: Vec<Stage>,
    frame: u32,
    stage: usize,
    stage_frame: u32,
}

impl ThumbRun {
    fn new(catalog: &TileCatalog) -> Self {
        let warm = catalog.tiles().iter().map(|t| Stage {
            tile: t.id.clone(),
            rotation: 0,
            shot: None,
        });
        let shots = catalog.tiles().iter().flat_map(|t| {
            let canon = canonical_rotations(t);
            (0..6u8)
                .filter(move |&k| canon[k as usize] == k)
                .map(|k| Stage {
                    tile: t.id.clone(),
                    rotation: k,
                    shot: Some(format!("{SHOT_PREFIX}/{}", stem(&t.id, k))),
                })
        });
        Self {
            stages: warm.chain(shots).collect(),
            frame: 0,
            stage: 0,
            stage_frame: 0,
        }
    }
}

pub struct ThumbsPlugin;

impl Plugin for ThumbsPlugin {
    fn build(&self, app: &mut App) {
        let catalog =
            TileCatalog::builtin().expect("the embedded catalog is validated by hex-core");
        let plan = serde_json::to_string_pretty(&plan_json(&catalog)).expect("plan serializes");
        let prepared = std::fs::remove_dir_all(RAW_DIR)
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            })
            .and_then(|_| std::fs::create_dir_all(RAW_DIR))
            .and_then(|_| std::fs::write(format!("{RAW_DIR}/plan.json"), plan + "\n"));
        if let Err(e) = prepared {
            eprintln!("hex-world: cannot prepare {RAW_DIR}: {e}");
            std::process::exit(2);
        }
        // Thumbnails show tiles at rest: under reduced motion a rise is an instant cut, so no
        // capture lands mid-rise (dust, a column still climbing past the frame edge).
        app.insert_resource(ClearColor(KEY))
            .insert_resource(ReducedMotion(true))
            .insert_resource(ThumbRun::new(&catalog))
            .add_systems(
                Update,
                (
                    stage_scene,
                    drive.after(game::sync_view).before(hex_render::play::sync),
                ),
            );
    }
}

/// Holds the thumbnail camera and hides the GPU flock every frame.
fn stage_scene(
    mut commands: Commands,
    mut cameras: Query<
        (
            Entity,
            &mut Transform,
            &mut Projection,
            Option<&mut DebandDither>,
        ),
        With<Camera3d>,
    >,
    mut named: Query<(&Name, &mut Visibility)>,
) {
    for (e, mut t, mut p, dither) in &mut cameras {
        *t = camera_transform();
        if let Projection::Perspective(persp) = p.as_mut() {
            persp.fov = FOV_DEG.to_radians();
        }
        // Dither noise would leak into the keyed background.
        match dither {
            Some(mut d) if *d != DebandDither::Disabled => *d = DebandDither::Disabled,
            Some(_) => {}
            None => {
                commands.entity(e).insert(DebandDither::Disabled);
            }
        }
    }
    for (name, mut vis) in &mut named {
        if name.as_str() == "boids" && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}

/// Steps through the stages: one tile alone at the origin, then its screenshot.
fn drive(
    mut commands: Commands,
    mut run: ResMut<ThumbRun>,
    kit: Option<Res<PlayKit>>,
    mut view: ResMut<PlayView>,
    mut exit: ResMut<ExitRequest>,
) {
    run.frame += 1;
    let ready = matches!(kit.as_deref(), Some(PlayKit::Ready(_)));
    if run.frame < START_FRAME || !ready || exit.0 {
        return;
    }
    let ThumbRun {
        stages,
        stage,
        stage_frame,
        ..
    } = &mut *run;
    let Some(s) = stages.get(*stage) else {
        info!(
            "thumbs: {} images in {RAW_DIR}",
            stages.iter().filter(|s| s.shot.is_some()).count()
        );
        exit.0 = true;
        return;
    };
    let want = PlayView {
        tiles: vec![PlayTile {
            cell: Axial::ORIGIN,
            tile: s.tile.clone(),
            rotation: s.rotation,
        }],
        ..default()
    };
    if *view != want {
        *view = want;
    }
    *stage_frame += 1;
    let last = match &s.shot {
        None => WARM_FRAMES,
        Some(name) => {
            if *stage_frame == SETTLE_FRAMES {
                capture::request(&mut commands, name);
            }
            SETTLE_FRAMES + HOLD_FRAMES
        }
    };
    if *stage_frame >= last {
        *stage += 1;
        *stage_frame = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    #[test]
    fn distinct_rotations_follow_socket_symmetry() {
        let c = catalog();
        let distinct = |id: &str| {
            let canon = canonical_rotations(c.get(id).unwrap());
            (0..6u8).filter(|&k| canon[k as usize] == k).count()
        };
        let want = [
            ("meadow", 1),
            ("forest", 1),
            ("house_cluster", 6),
            ("road_straight", 3),
            ("road_bend_120", 6),
            ("road_bend_60", 6),
            ("road_junction", 2),
            ("river_straight", 3),
            ("river_bend", 6),
            ("river_road_bridge", 3),
            ("beach", 6),
            ("beach_outer", 6),
            ("beach_inner", 6),
            ("ocean", 1),
        ];
        for (id, n) in want {
            assert_eq!(distinct(id), n, "{id}");
        }
        assert_eq!(ThumbRun::new(&c).stages.len(), 14 + 56);
    }

    #[test]
    fn a_shared_image_means_equal_world_sockets() {
        for t in catalog().tiles() {
            let canon = canonical_rotations(t);
            for k in 0..6u8 {
                let c = canon[k as usize];
                assert!(c <= k);
                assert_eq!(signature(t, c), signature(t, k), "{} r{k}", t.id);
            }
        }
        let c = catalog();
        let straight = canonical_rotations(c.get("road_straight").unwrap());
        assert_eq!(straight, [0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn camera_looks_down_from_the_play_side() {
        let t = camera_transform();
        let fwd = t.forward();
        assert!(fwd.z < 0.0 && fwd.y < 0.0, "{fwd}");
        assert!(((t.translation - LOOK_AT).length() - DISTANCE).abs() < 1e-3);
    }
}
