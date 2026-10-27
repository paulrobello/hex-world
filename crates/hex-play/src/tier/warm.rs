//! Pipeline warm-up for a quality change (PRD §9.2). A change in play keeps the scene
//! ([`super::retier_play`]), so what is left of its stall is creating the new tier's render
//! pipelines: the camera's SSAO and normal prepass key every mesh pipeline, and the water's shader
//! variant follows the tier. On the web the browser compiles each new pipeline in its GPU process
//! and presents nothing meanwhile, seconds with a cold Metal shader cache. So once the play scene
//! has been idle for [`STABLE_FRAMES`] after READY with pipelines it has not warmed, a hidden camera
//! with each neighbouring tier's view components draws the same view into a small image for a few
//! frames, beside a copy of a water surface on that tier's variant that only it sees. Bevy then holds
//! those tiers' pipelines, and a later change to them creates none. Runs again whenever the scene
//! has created pipelines since (a pack, a new piece, a tier change). On by default on the web only
//! (`tier_warm`, `GateConfig::tier_warm`).
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::{
    asset::RenderAssetUsages,
    camera::{RenderTarget, visibility::RenderLayers},
    diagnostic::FrameCount,
    light::NotShadowCaster,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{
            Extent3d, PipelineCache, TextureDimension, TextureFormat, TextureUsages,
        },
        sync_world::RenderEntity,
    },
};

use super::TierChoice;
use crate::{
    AgentsPaused, SceneEntity,
    play::PlayKit,
    post::{QualityTier, TierWarmCamera, set_camera_ssao},
    water::{WaterKey, WaterMaterial, WaterSurface, tiles::PlayWaterCamera, wave_count},
    workload::{GateConfig, PostMode},
};

/// No warm-up before READY (`diag::READY_FRAME`), so it never delays it.
pub const WARM_AFTER_FRAME: u32 = 120;
/// Frames the pipeline count must hold, with none being created, before a warm-up starts.
pub const STABLE_FRAMES: u32 = 60;
/// Frames a warm camera draws at least, and at most while pipelines are still being created. A new
/// view's key gains the environment map a frame or two after it first draws (Bevy gathers its light
/// probes late), so a shorter warm-up specializes for a key no game camera has.
pub const MIN_FRAMES: u32 = 10;
pub const MAX_FRAMES: u32 = 120;
/// The layer only warm cameras see: their copy of a water surface on another tier's variant.
pub const WARM_LAYER: usize = 7;
/// Width of the warm target; the height follows the game camera's aspect.
const TARGET_WIDTH: u32 = 256;

/// Render pipelines in the cache, and those not yet created, as the render world last saw them.
static CACHED: AtomicUsize = AtomicUsize::new(0);
static WAITING: AtomicUsize = AtomicUsize::new(0);

/// The tier whose pipelines a warm-up at `tier` creates: High and Medium warm each other, and Low
/// warms Medium (Ultra draws with High's pipelines). Medium does not warm Low: that would put a
/// second compile right after every change to Medium.
pub fn targets(tier: QualityTier) -> &'static [QualityTier] {
    use QualityTier::*;
    match tier {
        High | Ultra => &[Medium],
        Medium => &[High],
        Low => &[Medium],
    }
}

/// `state().tier.warm`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WarmReport {
    pub enabled: bool,
    /// A warm camera is drawing.
    pub active: bool,
    /// Warm-ups finished.
    pub runs: u32,
    /// The tiers the latest warm-up created pipelines for.
    pub tiers: Vec<QualityTier>,
    /// Pipelines the latest warm-up added to the cache, and the frames it drew.
    pub added: usize,
    pub frames: u32,
    /// Render pipelines in the cache.
    pub pipelines: usize,
    /// The latest warm-up ran at the tier in force and no pipeline was created since: a change to
    /// its tiers now creates none.
    pub current: bool,
}

impl WarmReport {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "active": self.active,
            "runs": self.runs,
            "tiers": self.tiers.iter().map(|t| t.name()).collect::<Vec<_>>(),
            "added": self.added,
            "frames": self.frames,
            "pipelines": self.pipelines,
            "current": self.current,
        })
    }
}

/// A warm-up in flight.
struct Live {
    entities: Vec<Entity>,
    frames: u32,
    before: usize,
    from: QualityTier,
    tiers: Vec<QualityTier>,
}

#[derive(Default)]
pub(super) struct WarmState {
    /// The cache size and tier the latest warm-up finished at.
    done: Option<(usize, QualityTier)>,
    last: usize,
    stable: u32,
    live: Option<Live>,
    target: Option<Handle<Image>>,
}

/// A copy of a game camera seen through `tier`: the components a camera built at the tier would
/// key its pipelines on, drawing into `target`, before the game camera and seeing the warm layer.
fn spawn_camera(
    commands: &mut Commands,
    game: Entity,
    camera: &Camera,
    tier: QualityTier,
    config: &GateConfig,
    target: Handle<Image>,
) -> Entity {
    let mut game = commands.entity(game);
    // Denying a component also drops the components that require it (`Camera3d` requires `Camera`)
    // unless `without_required_by_components` says otherwise.
    let mut warm = game.clone_and_spawn_with_opt_out(|b| {
        b.without_required_by_components(|b| {
            b.deny::<(RenderEntity, Camera, RenderTarget, RenderLayers)>();
        });
    });
    warm.insert((
        Camera {
            order: -1 - tier as isize,
            is_active: true,
            ..camera.clone()
        },
        RenderTarget::Image(target.into()),
        RenderLayers::from_layers(&[0, WARM_LAYER]),
        TierWarmCamera,
    ));
    if config.post == PostMode::Full {
        set_camera_ssao(&mut warm, tier);
    }
    crate::water::tiles::set_transmission(&mut warm, tier);
    warm.id()
}

fn target_image(aspect: f32) -> Image {
    let height = ((TARGET_WIDTH as f32 / aspect.max(0.1)).round() as u32).clamp(64, 1024);
    let mut img = Image::new_uninit(
        Extent3d {
            width: TARGET_WIDTH,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;
    img
}

/// Starts a warm-up once the play scene is idle with pipelines it has not warmed, and ends it once
/// the warm cameras' pipelines exist.
#[allow(clippy::too_many_arguments)]
pub(super) fn warm(
    mut commands: Commands,
    mut state: Local<WarmState>,
    mut choice: ResMut<TierChoice>,
    (config, active, frame, kit, paused): (
        Res<GateConfig>,
        Res<crate::ActiveScenario>,
        Res<FrameCount>,
        Option<Res<PlayKit>>,
        Option<Res<AgentsPaused>>,
    ),
    cameras: Query<
        (Entity, &Camera, &GlobalTransform),
        (
            With<Camera3d>,
            With<PlayWaterCamera>,
            With<SceneEntity>,
            Without<TierWarmCamera>,
        ),
    >,
    water: Query<(&Mesh3d, &MeshMaterial3d<WaterMaterial>), With<WaterSurface>>,
    (mut images, mut water_mats): (ResMut<Assets<Image>>, ResMut<Assets<WaterMaterial>>),
) {
    let cached = CACHED.load(Ordering::Acquire);
    let waiting = WAITING.load(Ordering::Acquire);
    let report = |choice: &mut TierChoice, live: bool, current: bool| {
        let w = &mut choice.warm;
        w.enabled = config.tier_warm;
        w.active = live;
        w.pipelines = cached;
        w.current = current;
    };
    let warmed = |done: Option<(usize, QualityTier)>| {
        done.is_some_and(|(n, tier)| n >= cached && tier == config.tier)
    };
    if let Some(live) = &mut state.live {
        live.frames += 1;
        if (live.frames >= MIN_FRAMES && waiting == 0) || live.frames >= MAX_FRAMES {
            let live = state.live.take().unwrap();
            for e in live.entities {
                commands.entity(e).try_despawn();
            }
            let w = &mut choice.warm;
            w.runs += 1;
            w.added = cached.saturating_sub(live.before);
            w.frames = live.frames;
            w.tiers = live.tiers;
            info!(
                "tier: warm-up for {:?} added {} pipelines in {} frames",
                w.tiers, w.added, w.frames
            );
            state.done = Some((cached, live.from));
            state.stable = 0;
            report(&mut choice, false, warmed(state.done));
        } else {
            report(&mut choice, true, false);
        }
        return;
    }
    report(&mut choice, false, warmed(state.done));
    if cached != state.last || waiting > 0 {
        state.last = cached;
        state.stable = 0;
    } else {
        state.stable = state.stable.saturating_add(1);
    }
    let idle = config.tier_warm
        && active.0 == crate::Scenario::Play
        && frame.0 > WARM_AFTER_FRAME
        && matches!(kit.as_deref(), Some(PlayKit::Ready(_)))
        && !choice.rebuilding()
        && !paused.is_some_and(|p| p.0)
        && state.stable >= STABLE_FRAMES;
    if !idle || warmed(state.done) {
        return;
    }
    let Some((game, camera, at)) = cameras.iter().find(|(_, c, _)| c.is_active) else {
        return;
    };
    let aspect = camera
        .logical_viewport_size()
        .map_or(16.0 / 9.0, |s| s.x / s.y.max(1.0));
    let target = state
        .target
        .get_or_insert_with(|| images.add(target_image(aspect)))
        .clone();
    let tiers = targets(config.tier).to_vec();
    let mut entities = vec![];
    // In front of the camera, so the warm view draws it whatever the game view shows.
    let ahead = Transform::from_translation(at.translation() + at.forward() * 40.0);
    let sample = water
        .iter()
        .next()
        .and_then(|(mesh, mat)| Some((mesh.0.clone(), water_mats.get(&mat.0)?.clone())));
    for &tier in &tiers {
        entities.push(spawn_camera(
            &mut commands,
            game,
            camera,
            tier,
            &config,
            target.clone(),
        ));
        if let Some((mesh, mat)) = &sample {
            let key = WaterKey {
                freeze: mat.key.freeze,
                ..WaterKey::for_tier(tier)
            };
            if key == mat.key {
                continue;
            }
            let mut m = mat.clone();
            m.key = key;
            m.params.wave_count = wave_count(tier);
            entities.push(
                commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(water_mats.add(m)),
                        ahead,
                        RenderLayers::layer(WARM_LAYER),
                        NotShadowCaster,
                        TierWarmCamera,
                        SceneEntity,
                    ))
                    .id(),
            );
        }
    }
    info!("tier: warming {tiers:?} ({cached} pipelines cached)");
    state.live = Some(Live {
        entities,
        frames: 0,
        before: cached,
        from: config.tier,
        tiers,
    });
    report(&mut choice, true, false);
}

/// Render world: publishes the pipeline cache's size and what it still has to create.
pub(super) fn count_pipelines(cache: Res<PipelineCache>) {
    CACHED.store(cache.pipelines().count(), Ordering::Release);
    WAITING.store(cache.waiting_pipelines().count(), Ordering::Release);
}

pub(super) fn build(app: &mut App) {
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(Render, count_pipelines.in_set(RenderSystems::Cleanup));
    }
}

#[cfg(all(test, feature = "gate-tools"))]
mod tests {
    #[test]
    fn warm_ups_start_after_ready() {
        assert_eq!(super::WARM_AFTER_FRAME, crate::diag::READY_FRAME);
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    use QualityTier::*;

    #[test]
    fn warm_targets_are_the_neighbours_with_other_pipelines() {
        assert_eq!(targets(High), &[Medium]);
        assert_eq!(targets(Ultra), &[Medium]);
        assert_eq!(targets(Medium), &[High]);
        assert_eq!(targets(Low), &[Medium]);
        for t in [Low, Medium, High, Ultra] {
            assert!(!targets(t).contains(&t));
        }
    }
}
