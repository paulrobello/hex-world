//! `--thumbs`: renders the per-rotation tile thumbnails the Legend inventory shows (PRD §5.3).
//!
//! Each tile is drawn alone at the origin through the play scenario's own path: this module only
//! writes [`PlayView`], and `hex_play::play` spawns the tile's kit pieces or placeholders, so the
//! thumbnails follow the game as kit pieces land. Rotations whose world-edge sockets equal an
//! earlier rotation's reuse that image. The run writes raw frames against a key colour plus
//! `plan.json` to [`RAW_DIR`]; `scripts/thumbs/build.py` keys, downsamples and encodes them.
use bevy::prelude::*;
use bevy::render::view::DebandDither;
use hex_core::{Axial, EdgeSocket, TileCatalog, TileDef};
use hex_play::play::{
    PlayKit, PlayTile, PlayView,
    packs::{PackRequests, PlayPacks},
    rise::ReducedMotion,
};
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
/// tile, so all thumbnails share one scale. The hex fills about 75% of the width, leaving room for
/// scattered spruces near the border, whose crowns rise and overhang the edge by up to 1 m.
const PITCH_DEG: f32 = 58.0;
const FOV_DEG: f32 = 20.0;
const DISTANCE: f32 = 108.0;
const LOOK_AT: Vec3 = Vec3::new(0.0, 3.5, 1.5);

/// Frames before the first stage. Starting at a fixed frame keeps every capture on the same frame
/// number from run to run, so `--clock fixed` time-driven materials match.
const START_FRAME: u32 = 240;
/// Each tile is shown once before any capture, so pipelines compile and textures load first.
const WARM_FRAMES: u32 = 30;
/// Frames a captured tile is shown before its screenshot is requested.
const SETTLE_FRAMES: u32 = 20;
/// Frames held after the request, so the capture never sees the next tile.
const HOLD_FRAMES: u32 = 3;
/// How long a tile whose frame came back all black is captured again before the run stops with
/// exit 1 (so `make thumbs` fails before the build step). Wall time, not frames: under host load
/// the GPU can return black frames for several seconds, across several shots in a row. Stopping
/// at the first expired shot bounds a run on a GPU that never draws to one wait.
const BLANK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// The thumbnail mode's fixed render settings: the play scenario with no post stack, tilt-shift
/// or FPS counter, the fixed clock, and the supersampled window.
pub fn force_flags(f: &mut Flags) {
    f.scenario = Scenario::Play;
    f.gate.post = PostMode::None;
    f.gate.tilt = TiltMode::Off;
    f.fps_on = false;
    f.clock_fixed = true;
    f.size = Some(RENDER_SIZE);
    f.shot = None;
    f.script = None;
    f.exit_after_frames = None;
    f.exit_after_secs = None;
    f.screenshot_after_secs = None;
    // Thumbnails are committed assets: the tier stays High unless a `--tier` key names one.
    f.tier_explicit = true;
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
    blank: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// When the current shot first came back black, and how many black frames since.
    blank_since: Option<std::time::Instant>,
    blanks: u32,
    /// The pending capture stores a black frame: [`BLANK_WAIT`] ran out.
    keep_black: bool,
}

/// A capture keeps a black frame only once the shot has been black for [`BLANK_WAIT`].
fn keep_black(blank_since: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    blank_since.is_some_and(|t| now.duration_since(t) >= BLANK_WAIT)
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
            blank: Default::default(),
            blank_since: None,
            blanks: 0,
            keep_black: false,
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
            .insert_resource(PackRequests {
                all: true,
                ..default()
            })
            .add_systems(
                Update,
                (
                    stage_scene,
                    drive.after(game::sync_view).before(hex_play::play::sync),
                ),
            );
    }
}

/// Holds the thumbnail camera and the key clear colour and hides the GPU flock every frame.
fn stage_scene(
    mut commands: Commands,
    mut clear: ResMut<ClearColor>,
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
    // Every scene build sets the grade's backdrop as the clear colour.
    if clear.0 != KEY {
        clear.0 = KEY;
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
    (kit, packs): (Option<Res<PlayKit>>, Option<Res<PlayPacks>>),
    mut view: ResMut<PlayView>,
    mut exit: ResMut<ExitRequest>,
) {
    run.frame += 1;
    // Every pack, not just core: the thumbnails show each tile's kit look. No packs resource with a
    // ready kit means the manifest failed to parse and every tile is a placeholder anyway.
    let ready =
        matches!(kit.as_deref(), Some(PlayKit::Ready(_))) && packs.is_none_or(|p| p.all_ready());
    if run.frame < START_FRAME || !ready || exit.0 {
        return;
    }
    let ThumbRun {
        stages,
        stage,
        stage_frame,
        blank,
        blank_since,
        blanks,
        keep_black: keep,
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
                blank.store(false, std::sync::atomic::Ordering::SeqCst);
                *keep = keep_black(*blank_since, std::time::Instant::now());
                capture::request_unless_blank(&mut commands, name, blank.clone(), *keep);
            }
            SETTLE_FRAMES + HOLD_FRAMES
        }
    };
    // A shot stage moves on only once its capture is encoded, so a blank frame can be retaken.
    if *stage_frame >= last && (s.shot.is_none() || capture::pending() == 0) {
        if s.shot.is_some() && blank.swap(false, std::sync::atomic::Ordering::SeqCst) && !*keep {
            blank_since.get_or_insert_with(std::time::Instant::now);
            *blanks += 1;
            *stage_frame = 0;
            return;
        }
        if let (Some(t), Some(name)) = (blank_since.take(), &s.shot) {
            let waited = t.elapsed().as_secs_f32();
            if *keep {
                error!("thumbs: {name} still black after {waited:.1} s ({blanks} blank frames)");
                std::process::exit(1);
            } else {
                info!(
                    "thumbs: {name} rendered after {waited:.1} s waiting through {blanks} blank frame(s)"
                );
            }
        }
        *blanks = 0;
        *keep = false;
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
    fn a_black_frame_is_kept_only_after_the_wait() {
        let now = std::time::Instant::now();
        assert!(
            !keep_black(None, now),
            "the first capture never keeps black"
        );
        assert!(!keep_black(Some(now), now));
        assert!(!keep_black(Some(now), now + BLANK_WAIT / 2));
        assert!(keep_black(Some(now), now + BLANK_WAIT));
    }

    #[test]
    fn only_a_fully_black_frame_counts_as_blank() {
        assert!(capture::all_black(&[0, 0, 0, 255, 0, 0, 0, 255]));
        assert!(!capture::all_black(&[0, 0, 0, 255, 0, 1, 0, 255]));
        assert!(!capture::all_black(&[]));
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
            ("oak", 1),
            ("windmill", 1),
            ("house_cluster", 6),
            ("road_straight", 3),
            ("road_bend_120", 6),
            ("road_bend_60", 6),
            ("road_junction", 2),
            ("river_straight", 3),
            ("river_bend", 6),
            ("river_road_bridge", 3),
            ("river_spring", 6),
            ("beach", 6),
            ("beach_outer", 6),
            ("beach_inner", 6),
            ("river_mouth", 6),
            ("river_mouth_l", 6),
            ("ocean", 1),
        ];
        for (id, n) in want {
            assert_eq!(distinct(id), n, "{id}");
        }
        assert_eq!(ThumbRun::new(&c).stages.len(), 19 + 76);
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
