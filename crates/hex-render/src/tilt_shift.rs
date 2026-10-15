//! Tilt-shift post-process (PRD §9.1): a miniature-diorama blur driven by a focal band that
//! follows the camera's look-at point on the ground, plus the view-space depth difference.
//!
//! Bevy 0.20's `FullscreenMaterial` binds only the colour target, so it cannot read depth. This is
//! the same one-triangle pass written out by hand: colour, the prepass depth and one uniform. One
//! gather pass over a golden-angle disc replaces a separable blur, since the radius varies per
//! pixel. The pass runs before bloom, which also keeps it clear of Bevy's depth of field (its
//! system is crate-private, so it cannot be ordered against).
//!
//! Costs 2 sampled textures in the fragment stage, far below WebGPU's default of 16.
use bevy::{
    asset::AssetServer,
    core_pipeline::{
        FullscreenShader,
        prepass::{DepthPrepass, ViewPrepassTextures},
        schedule::{Core3d, Core3dSystems},
    },
    ecs::schedule::IntoScheduleConfigs,
    post_process::bloom::bloom,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, Canonical, ColorTargetState, ColorWrites, FragmentState,
            LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
            SamplerDescriptor, ShaderStages, ShaderType, Specializer, SpecializerKey, StoreOp,
            TextureFormat, TextureSampleType, Variants,
            binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::{ExtractedView, ViewTarget},
    },
};

use crate::{
    post::QualityTier,
    workload::{GateConfig, PostMode, TiltMode},
};

const SHADER: &str = "shaders/tilt_shift.wesl";

/// Focus distances (m) between which the effect fades in: off near the ground, full when zoomed out.
pub const ZOOM_OFF_M: f32 = 12.0;
pub const ZOOM_FULL_M: f32 = 48.0;
/// Largest blur radius as a fraction of screen height at strength 1.
const MAX_RADIUS: f32 = 0.022;
/// Half height of the sharp band, as a fraction of screen height.
const BAND_HALF: f32 = 0.12;
/// Saturation added at full effect.
const SATURATION_LIFT: f32 = 0.12;
/// Focus distance when the view ray misses the ground (m).
const FALLBACK_FOCUS_M: f32 = 100.0;

/// Gather taps per quality tier; `None` is off (the lowest tier).
pub fn tier_taps(tier: QualityTier) -> Option<u32> {
    match tier {
        QualityTier::Low => None,
        QualityTier::Medium => Some(8),
        QualityTier::High => Some(16),
        QualityTier::Ultra => Some(24),
    }
}

/// Player-facing state: the toggle and strength slider (PRD §9.1), plus the tier's tap count.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct TiltShiftSettings {
    pub enabled: bool,
    /// 0..=1.
    pub strength: f32,
    pub taps: u32,
}

impl Default for TiltShiftSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            strength: 1.0,
            taps: 16,
        }
    }
}

/// Set once the player toggles tilt-shift themselves: the Settings switch (a `tilt` command with
/// `on`) or the `T` key. A strength-only `tilt` command does not set it. A loaded save restores the
/// flag it was saved with (format v2 `tilt_shift_chosen`; v1 files load unset), so the autosave
/// resume alone never counts as a choice. While set, a tier change keeps the player's toggle and
/// strength and takes only the new tier's taps; while unset it resets both to the tier's default.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TiltPinned(pub bool);

/// The tilt-shift the last scene (re)build set from the tier: where a loaded world with no
/// player choice puts the toggle and strength back.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct TiltTierDefault(pub TiltShiftSettings);

/// The settings after a tier rebuild: `rebuilt` (the new tier's default), or the player's `before`
/// toggle and strength with the new tier's taps once they chose them.
pub fn kept_tilt(
    before: TiltShiftSettings,
    rebuilt: TiltShiftSettings,
    pinned: bool,
) -> TiltShiftSettings {
    if pinned {
        TiltShiftSettings {
            enabled: before.enabled,
            strength: before.strength,
            taps: rebuilt.taps,
        }
    } else {
        rebuilt
    }
}

/// Camera component and shader uniform. Present on a camera only while the effect should run,
/// so a disabled effect costs nothing.
#[derive(Component, ExtractComponent, Clone, Copy, Debug, Default, ShaderType)]
#[extract_app(RenderApp)]
#[require(DepthPrepass)]
pub struct TiltShift {
    /// Band centre, 0 at the top of the screen.
    pub focus_uv_y: f32,
    pub band_half: f32,
    /// View-space distance of the focus point (m).
    pub focus_depth: f32,
    pub near: f32,
    /// Largest blur radius, as a fraction of screen height.
    pub max_radius: f32,
    pub depth_weight: f32,
    pub saturation: f32,
    pub taps: u32,
}

/// 0 at or below `ZOOM_OFF_M`, 1 at or beyond `ZOOM_FULL_M`.
pub fn zoom_factor(focus_distance: f32) -> f32 {
    let t = ((focus_distance - ZOOM_OFF_M) / (ZOOM_FULL_M - ZOOM_OFF_M)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance along the view ray to the ground plane `y = 0`, or the fallback when it misses.
pub fn focus_distance(pos: Vec3, forward: Vec3) -> f32 {
    if forward.y < -0.02 {
        (-pos.y / forward.y).max(0.1)
    } else {
        FALLBACK_FOCUS_M
    }
}

/// Uniform for a camera whose focus point is `focus_dist` away and projects to `focus_uv_y`.
/// `None` when the effect is off or fully faded out.
pub fn build_uniform(
    settings: &TiltShiftSettings,
    focus_dist: f32,
    focus_uv_y: f32,
    near: f32,
) -> Option<TiltShift> {
    let amount = settings.strength.clamp(0.0, 1.0) * zoom_factor(focus_dist);
    (settings.enabled && amount > 0.01).then_some(TiltShift {
        focus_uv_y,
        band_half: BAND_HALF,
        focus_depth: focus_dist,
        near,
        max_radius: MAX_RADIUS * amount,
        depth_weight: 0.6,
        saturation: SATURATION_LIFT * amount,
        taps: settings.taps.clamp(1, 32),
    })
}

/// Photo mode's tilt-shift preset (PRD §4.6): the same kernel and pass, with the focus distance
/// and strength set by the photo panel instead of the ground ray and zoom. While present it wins
/// over [`TiltShiftSettings`], which keeps the player's game-camera choice for when photo mode ends.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct TiltOverride {
    /// View-space distance of the sharp band (m).
    pub focus_m: f32,
    /// The band's screen row, 0 at the top: 0.5 on the optical axis, or the row a click focused on.
    pub focus_uv_y: f32,
    /// 0..=1, applied without the zoom fade so close-ups blur too.
    pub strength: f32,
}

/// Uniform for the photo preset: the band sits on the row the panel chose (the screen centre
/// unless a click focused elsewhere).
pub fn build_override_uniform(o: &TiltOverride, taps: u32, near: f32) -> Option<TiltShift> {
    let amount = o.strength.clamp(0.0, 1.0);
    (amount > 0.01).then_some(TiltShift {
        focus_uv_y: if o.focus_uv_y.is_finite() {
            o.focus_uv_y.clamp(0.0, 1.0)
        } else {
            0.5
        },
        band_half: BAND_HALF,
        focus_depth: o.focus_m.max(0.5),
        near,
        max_radius: MAX_RADIUS * amount,
        depth_weight: 0.6,
        saturation: SATURATION_LIFT * amount,
        taps: taps.clamp(1, 32),
    })
}

/// Initial settings for a scene (re)build. `Auto` is on from Medium up and only in the kit
/// scenario with the post stack; `On` forces it at any tier (Low uses the Medium tap count).
pub fn settings_for(config: &GateConfig, kit: bool, tier: QualityTier) -> TiltShiftSettings {
    let enabled = match config.tilt {
        TiltMode::On => true,
        TiltMode::Off => false,
        TiltMode::Auto => kit && config.post == PostMode::Full && tier_taps(tier).is_some(),
    };
    TiltShiftSettings {
        enabled,
        strength: config.tilt_strength.get(),
        taps: tier_taps(tier).unwrap_or(8),
    }
}

pub struct TiltShiftPlugin;

impl Plugin for TiltShiftPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TiltShiftSettings>()
            .init_resource::<TiltPinned>()
            .init_resource::<TiltTierDefault>()
            .add_plugins((
                ExtractComponentPlugin::<TiltShift>::default(),
                UniformComponentPlugin::<TiltShift>::default(),
            ))
            .add_systems(Update, (toggle_key, update_cameras).chain());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(Render, prepare_pipelines.in_set(RenderSystems::Prepare))
            .add_systems(
                Core3d,
                tilt_shift
                    .before(bloom)
                    .in_set(Core3dSystems::PostProcess)
                    .in_set(TiltShiftPass),
            );
    }
}

/// The tilt-shift render system, for ordering against it (`gpu_timing` markers).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TiltShiftPass;

/// `T` toggles the effect at runtime.
fn toggle_key(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut settings: ResMut<TiltShiftSettings>,
    mut pinned: ResMut<TiltPinned>,
) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::KeyT)) {
        settings.enabled = !settings.enabled;
        pinned.0 = true;
        info!(
            "tilt-shift: {}",
            if settings.enabled { "on" } else { "off" }
        );
    }
}

fn update_cameras(
    mut commands: Commands,
    settings: Res<TiltShiftSettings>,
    photo: Option<Res<TiltOverride>>,
    cameras: Query<(Entity, &GlobalTransform, &Projection, Has<TiltShift>), With<Camera3d>>,
) {
    for (entity, transform, projection, has) in &cameras {
        let Projection::Perspective(p) = projection else {
            continue;
        };
        let pos = transform.translation();
        let forward = transform.forward().as_vec3();
        let dist = focus_distance(pos, forward);
        let view_pos = transform
            .affine()
            .inverse()
            .transform_point3(pos + forward * dist);
        let clip = projection.get_clip_from_view() * view_pos.extend(1.0);
        let uv_y = if clip.w.abs() > 1e-6 {
            0.5 - 0.5 * clip.y / clip.w
        } else {
            0.5
        };
        let uniform = match photo.as_deref() {
            Some(o) => build_override_uniform(o, settings.taps, p.near),
            None => build_uniform(&settings, dist, uv_y, p.near),
        };
        match uniform {
            Some(u) => {
                commands.entity(entity).insert(u);
            }
            None if has => {
                commands.entity(entity).remove::<TiltShift>();
            }
            None => {}
        }
    }
}

#[derive(Resource)]
struct TiltShiftPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    variants: Variants<RenderPipeline, TiltSpecializer>,
}

struct TiltSpecializer;

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct TiltKey {
    target_format: TextureFormat,
}

impl Specializer<RenderPipeline> for TiltSpecializer {
    type Key = TiltKey;

    fn specialize(
        &self,
        key: Self::Key,
        descriptor: &mut RenderPipelineDescriptor,
    ) -> Result<Canonical<Self::Key>, BevyError> {
        descriptor.fragment_mut()?.set_target(
            0,
            ColorTargetState {
                format: key.target_format,
                blend: None,
                write_mask: ColorWrites::ALL,
            },
        );
        Ok(key)
    }
}

fn init_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "tilt_shift_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_depth_2d(),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<TiltShift>(true),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let desc = RenderPipelineDescriptor {
        label: Some("tilt_shift_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: asset_server.load(SHADER),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };
    commands.insert_resource(TiltShiftPipeline {
        layout,
        sampler,
        variants: Variants::new(TiltSpecializer, desc),
    });
}

#[derive(Component)]
struct TiltShiftPipelineId(CachedRenderPipelineId);

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<TiltShiftPipeline>,
    views: Query<(Entity, &ExtractedView), With<TiltShift>>,
    stale: Query<Entity, (With<TiltShiftPipelineId>, Without<TiltShift>)>,
) -> Result<(), BevyError> {
    // The render entity outlives the main-world component: drop the pipeline id so the pass stops.
    for entity in &stale {
        commands.entity(entity).remove::<TiltShiftPipelineId>();
    }
    for (entity, view) in &views {
        let id = pipeline.variants.specialize(
            &pipeline_cache,
            TiltKey {
                target_format: view.target_format,
            },
        )?;
        commands.entity(entity).insert(TiltShiftPipelineId(id));
    }
    Ok(())
}

fn tilt_shift(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<TiltShift>,
        &TiltShiftPipelineId,
        &ViewPrepassTextures,
    )>,
    pipeline: Res<TiltShiftPipeline>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<TiltShift>>,
    mut ctx: RenderContext,
) {
    let (target, index, id, prepass) = view.into_inner();
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(id.0) else {
        return;
    };
    let (Some(depth), Some(binding)) = (prepass.depth_only_view(), uniforms.uniforms().binding())
    else {
        return;
    };
    let post = target.post_process_write();
    let bind_group = ctx.render_device().create_bind_group(
        Some("tilt_shift_bind_group"),
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((post.source, depth, &pipeline.sampler, binding)),
    );
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("tilt_shift"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Default::default()),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_render_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind_group, &[index.index()]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> TiltShiftSettings {
        TiltShiftSettings {
            enabled: true,
            ..default()
        }
    }

    #[test]
    fn tiers_scale_taps_and_lowest_is_off() {
        assert_eq!(tier_taps(QualityTier::Low), None);
        assert!(tier_taps(QualityTier::Medium) < tier_taps(QualityTier::High));
        assert!(tier_taps(QualityTier::High) <= tier_taps(QualityTier::Ultra));
    }

    #[test]
    fn config_resolves_per_tier_scenario_and_force() {
        let c = |tilt, post| GateConfig {
            tilt,
            post,
            ..default()
        };
        let auto = c(TiltMode::Auto, PostMode::Full);
        assert!(settings_for(&auto, true, QualityTier::High).enabled);
        assert!(settings_for(&auto, true, QualityTier::Medium).enabled);
        assert!(
            !settings_for(&auto, true, QualityTier::Low).enabled,
            "off on Low"
        );
        assert!(
            !settings_for(&auto, false, QualityTier::High).enabled,
            "kit scenario only"
        );
        assert!(!settings_for(&c(TiltMode::Auto, PostMode::None), true, QualityTier::High).enabled);
        assert!(settings_for(&c(TiltMode::On, PostMode::Full), false, QualityTier::Low).enabled);
        assert!(!settings_for(&c(TiltMode::Off, PostMode::Full), true, QualityTier::High).enabled);
        assert!(
            settings_for(&auto, true, QualityTier::Medium).taps
                < settings_for(&auto, true, QualityTier::High).taps
        );
    }

    #[test]
    fn zoom_fades_from_off_near_ground_to_full_zoomed_out() {
        assert_eq!(zoom_factor(2.0), 0.0);
        assert_eq!(zoom_factor(ZOOM_OFF_M), 0.0);
        assert_eq!(zoom_factor(ZOOM_FULL_M), 1.0);
        assert_eq!(zoom_factor(500.0), 1.0);
        let mid = zoom_factor((ZOOM_OFF_M + ZOOM_FULL_M) / 2.0);
        assert!((mid - 0.5).abs() < 1e-5);
    }

    #[test]
    fn focus_is_where_the_view_ray_meets_the_ground() {
        let pos = Vec3::new(14.0, 38.0, 62.0);
        let fwd = (Vec3::new(14.0, 0.0, 0.0) - pos).normalize();
        let d = focus_distance(pos, fwd);
        assert!((d - (38.0f32 * 38.0 + 62.0 * 62.0).sqrt()).abs() < 1e-3);
        assert_eq!(
            focus_distance(Vec3::new(0.0, 2.0, 0.0), Vec3::Z),
            FALLBACK_FOCUS_M
        );
    }

    #[test]
    fn photo_override_ignores_zoom_and_the_player_toggle() {
        let o = TiltOverride {
            focus_m: 6.0,
            focus_uv_y: 0.5,
            strength: 0.5,
        };
        // A 6 m close-up is below ZOOM_OFF_M, where the game camera's effect is off.
        let u = build_override_uniform(&o, 16, 0.1).unwrap();
        assert_eq!((u.focus_uv_y, u.focus_depth, u.taps), (0.5, 6.0, 16));
        assert!((u.max_radius - MAX_RADIUS * 0.5).abs() < 1e-6);
        let off = TiltOverride { strength: 0.0, ..o };
        assert!(build_override_uniform(&off, 16, 0.1).is_none());
    }

    #[test]
    fn photo_override_puts_the_band_on_the_clicked_row() {
        let o = TiltOverride {
            focus_m: 30.0,
            focus_uv_y: 0.8,
            strength: 1.0,
        };
        let u = build_override_uniform(&o, 16, 0.1).unwrap();
        assert_eq!((u.focus_uv_y, u.focus_depth), (0.8, 30.0));
        let below = TiltOverride {
            focus_uv_y: 1.4,
            ..o
        };
        assert_eq!(
            build_override_uniform(&below, 16, 0.1).unwrap().focus_uv_y,
            1.0
        );
        let bad = TiltOverride {
            focus_uv_y: f32::NAN,
            ..o
        };
        assert_eq!(
            build_override_uniform(&bad, 16, 0.1).unwrap().focus_uv_y,
            0.5
        );
    }

    #[test]
    fn uniform_follows_settings() {
        assert!(build_uniform(&TiltShiftSettings::default(), 72.0, 0.5, 0.1).is_none());
        assert!(
            build_uniform(&on(), 5.0, 0.5, 0.1).is_none(),
            "off near the ground"
        );
        let full = build_uniform(&on(), 72.0, 0.5, 0.1).unwrap();
        assert_eq!((full.max_radius, full.taps), (MAX_RADIUS, 16));
        let half = TiltShiftSettings {
            strength: 0.5,
            ..on()
        };
        let u = build_uniform(&half, 72.0, 0.5, 0.1).unwrap();
        assert!((u.max_radius - MAX_RADIUS * 0.5).abs() < 1e-6);
        let zero = TiltShiftSettings {
            strength: 0.0,
            ..on()
        };
        assert!(build_uniform(&zero, 72.0, 0.5, 0.1).is_none());
    }

    #[test]
    fn a_tier_rebuild_keeps_the_players_choice_and_takes_the_new_taps() {
        let player = TiltShiftSettings {
            enabled: true,
            strength: 0.3,
            taps: 16,
        };
        let low_default = TiltShiftSettings {
            enabled: false,
            strength: 1.0,
            taps: 8,
        };
        assert_eq!(
            kept_tilt(player, low_default, true),
            TiltShiftSettings {
                enabled: true,
                strength: 0.3,
                taps: 8,
            }
        );
        assert_eq!(
            kept_tilt(player, low_default, false),
            low_default,
            "untouched, the tier's default applies"
        );
    }
}
