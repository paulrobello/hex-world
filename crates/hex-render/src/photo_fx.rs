//! Photo-mode effects (owner, 2026-10-14): a vignette, a colour filter (black and white or sepia),
//! film grain and chromatic aberration, in one fullscreen pass on the photo camera.
//!
//! The pass runs after tone mapping (Bevy's FXAA slot), so the filter sees display colour, and the
//! grain and vignette sit on top of everything earlier: the tilt-shift gather, TAA, bloom and the
//! lens DOF never blur or smear them. Tilt-shift stays before bloom because it needs HDR colour and
//! the depth prepass. Chromatic aberration is done here rather than with Bevy's
//! `ChromaticAberration`: that effect runs before tone mapping, binds a LUT texture, and caps its
//! samples per texel, so a 2x or 4x export would fringe differently from the viewfinder.
//!
//! It samples only the view's colour target (1 sampled texture) and adds nothing to any mesh
//! pipeline. [`PhotoFx`] is on a camera only while photo mode has an effect on, so every other
//! frame renders exactly as before. A photo export renders the same camera entity into its
//! offscreen image, so the pass runs there too; every size in the shader is a fraction of the
//! target, so a 2x export looks like the viewfinder.
use bevy::{
    asset::AssetServer,
    core_pipeline::{
        FullscreenShader,
        schedule::{Core3d, Core3dSystems},
        tonemapping::tonemapping,
    },
    ecs::schedule::IntoScheduleConfigs,
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
            binding_types::{sampler, texture_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::{ExtractedView, ViewTarget},
    },
};

const SHADER: &str = "shaders/photo_fx.wesl";

/// Corner darkening at vignette 1 (gamma space): the corners keep 15% of their brightness.
pub const VIGNETTE_MAX: f32 = 0.85;
/// Where the vignette starts (0 centre, 1 corner) at softness 0 and at softness 1.
pub const VIGNETTE_INNER: (f32, f32) = (0.8, 0.0);
/// Grain amplitude at grain 1 (gamma space, peak to peak).
pub const GRAIN_MAX: f32 = 0.22;
/// Grain cells per target height: the same grain at any window size, DPR or export scale.
pub const GRAIN_ROWS: f32 = 720.0;
/// Red and blue shift at the corners at aberration 1, as a fraction of the target size.
pub const ABERRATION_MAX: f32 = 0.006;
/// Values at or below this count as off.
const OFF: f32 = 0.005;

/// The colour filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FxFilter {
    #[default]
    Normal,
    Mono,
    Sepia,
}

/// What photo mode asks for this frame. Present only while photo mode is on; without it no camera
/// carries [`PhotoFx`]. Every value is 0..=1 and clamped again here.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PhotoFxLook {
    pub filter: FxFilter,
    pub filter_strength: f32,
    pub vignette: f32,
    pub vignette_softness: f32,
    pub grain: f32,
    pub aberration: f32,
    /// Grain pattern: the frame in the viewfinder, fixed during an export so the PNG is repeatable.
    pub seed: u32,
}

impl Default for PhotoFxLook {
    fn default() -> Self {
        Self {
            filter: FxFilter::Normal,
            filter_strength: 1.0,
            vignette: 0.0,
            vignette_softness: 0.5,
            grain: 0.0,
            aberration: 0.0,
            seed: 0,
        }
    }
}

/// 0..=1, with NaN as 0.
fn unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// Camera component and shader uniform; on the camera only while an effect is on.
#[derive(Component, ExtractComponent, Clone, Copy, Debug, Default, PartialEq, ShaderType)]
#[extract_app(RenderApp)]
pub struct PhotoFx {
    /// 0 normal, 1 black and white, 2 sepia.
    pub filter_mode: u32,
    pub filter_strength: f32,
    pub vignette: f32,
    pub vignette_inner: f32,
    pub grain: f32,
    pub grain_rows: f32,
    pub aberration: f32,
    pub seed: u32,
}

/// The uniform for a look, or `None` when every effect is off (then the pass does not run).
pub fn build_uniform(look: &PhotoFxLook) -> Option<PhotoFx> {
    let strength = unit(look.filter_strength);
    let filter_mode = match look.filter {
        FxFilter::Normal => 0,
        _ if strength <= OFF => 0,
        FxFilter::Mono => 1,
        FxFilter::Sepia => 2,
    };
    let on = |v: f32| if v > OFF { v } else { 0.0 };
    let (vignette, grain, aberration) = (
        on(unit(look.vignette)),
        on(unit(look.grain)),
        on(unit(look.aberration)),
    );
    if filter_mode == 0 && vignette == 0.0 && grain == 0.0 && aberration == 0.0 {
        return None;
    }
    let soft = unit(look.vignette_softness);
    Some(PhotoFx {
        filter_mode,
        filter_strength: if filter_mode == 0 { 0.0 } else { strength },
        vignette: vignette * VIGNETTE_MAX,
        vignette_inner: VIGNETTE_INNER.0 + (VIGNETTE_INNER.1 - VIGNETTE_INNER.0) * soft,
        grain: grain * GRAIN_MAX,
        grain_rows: GRAIN_ROWS,
        aberration: aberration * ABERRATION_MAX,
        seed: look.seed,
    })
}

pub struct PhotoFxPlugin;

impl Plugin for PhotoFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<PhotoFx>::default(),
            UniformComponentPlugin::<PhotoFx>::default(),
        ))
        // After every Update system, so the look photo mode set this frame is the one drawn.
        .add_systems(PostUpdate, update_cameras);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(Render, prepare_pipelines.in_set(RenderSystems::Prepare))
            .add_systems(
                Core3d,
                photo_fx
                    .after(tonemapping)
                    .in_set(Core3dSystems::PostProcess)
                    .in_set(PhotoFxPass),
            );
    }
}

/// The photo-effects render system, for ordering against it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhotoFxPass;

/// Puts [`PhotoFx`] on every 3D camera while [`PhotoFxLook`] asks for an effect, and takes it off
/// otherwise.
pub fn update_cameras(
    mut commands: Commands,
    look: Option<Res<PhotoFxLook>>,
    cameras: Query<(Entity, Option<&PhotoFx>), With<Camera3d>>,
) {
    let want = look.as_deref().and_then(build_uniform);
    for (entity, have) in &cameras {
        match (want, have) {
            (Some(w), Some(h)) if w == *h => {}
            (Some(w), _) => {
                commands.entity(entity).insert(w);
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<PhotoFx>();
            }
            (None, None) => {}
        }
    }
}

#[derive(Resource)]
struct PhotoFxPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    variants: Variants<RenderPipeline, FxSpecializer>,
}

struct FxSpecializer;

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct FxKey {
    target_format: TextureFormat,
}

impl Specializer<RenderPipeline> for FxSpecializer {
    type Key = FxKey;

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
        "photo_fx_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<PhotoFx>(true),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: bevy::render::render_resource::FilterMode::Linear,
        min_filter: bevy::render::render_resource::FilterMode::Linear,
        ..default()
    });
    let desc = RenderPipelineDescriptor {
        label: Some("photo_fx_pipeline".into()),
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
    commands.insert_resource(PhotoFxPipeline {
        layout,
        sampler,
        variants: Variants::new(FxSpecializer, desc),
    });
}

#[derive(Component)]
struct PhotoFxPipelineId(CachedRenderPipelineId);

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<PhotoFxPipeline>,
    views: Query<(Entity, &ExtractedView), With<PhotoFx>>,
    stale: Query<Entity, (With<PhotoFxPipelineId>, Without<PhotoFx>)>,
) -> Result<(), BevyError> {
    // The render entity outlives the main-world component: drop the pipeline id so the pass stops.
    for entity in &stale {
        commands.entity(entity).remove::<PhotoFxPipelineId>();
    }
    for (entity, view) in &views {
        let id = pipeline.variants.specialize(
            &pipeline_cache,
            FxKey {
                target_format: view.target_format,
            },
        )?;
        commands.entity(entity).insert(PhotoFxPipelineId(id));
    }
    Ok(())
}

fn photo_fx(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<PhotoFx>,
        &PhotoFxPipelineId,
    )>,
    pipeline: Res<PhotoFxPipeline>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<PhotoFx>>,
    mut ctx: RenderContext,
) {
    let (target, index, id) = view.into_inner();
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(id.0) else {
        return;
    };
    let Some(binding) = uniforms.uniforms().binding() else {
        return;
    };
    let post = target.post_process_write();
    let bind_group = ctx.render_device().create_bind_group(
        Some("photo_fx_bind_group"),
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((post.source, &pipeline.sampler, binding)),
    );
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("photo_fx"),
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
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    #[test]
    fn defaults_and_zeroes_build_no_pass() {
        assert!(build_uniform(&PhotoFxLook::default()).is_none());
        let zero_filter = PhotoFxLook {
            filter: FxFilter::Mono,
            filter_strength: 0.0,
            vignette_softness: 1.0,
            ..default()
        };
        assert!(
            build_uniform(&zero_filter).is_none(),
            "a filter at strength 0 and softness alone do nothing"
        );
        let nan = PhotoFxLook {
            vignette: f32::NAN,
            grain: f32::NAN,
            aberration: f32::NAN,
            ..default()
        };
        assert!(build_uniform(&nan).is_none(), "NaN counts as off");
    }

    #[test]
    fn each_effect_alone_builds_the_pass_and_values_clamp() {
        let one = |look: PhotoFxLook| build_uniform(&look).expect("an effect is on");
        let mono = one(PhotoFxLook {
            filter: FxFilter::Mono,
            filter_strength: 3.0,
            ..default()
        });
        assert_eq!((mono.filter_mode, mono.filter_strength), (1, 1.0));
        let sepia = one(PhotoFxLook {
            filter: FxFilter::Sepia,
            filter_strength: 0.5,
            ..default()
        });
        assert_eq!((sepia.filter_mode, sepia.filter_strength), (2, 0.5));
        let v = one(PhotoFxLook {
            vignette: 2.0,
            vignette_softness: -1.0,
            ..default()
        });
        assert_eq!(
            (v.vignette, v.vignette_inner, v.filter_mode),
            (VIGNETTE_MAX, VIGNETTE_INNER.0, 0)
        );
        let soft = one(PhotoFxLook {
            vignette: 0.5,
            vignette_softness: 1.0,
            ..default()
        });
        assert_eq!(soft.vignette_inner, VIGNETTE_INNER.1);
        let g = one(PhotoFxLook {
            grain: 1.0,
            seed: 7,
            ..default()
        });
        assert_eq!((g.grain, g.grain_rows, g.seed), (GRAIN_MAX, GRAIN_ROWS, 7));
        let ca = one(PhotoFxLook {
            aberration: 0.5,
            ..default()
        });
        assert!((ca.aberration - ABERRATION_MAX * 0.5).abs() < 1e-7);
        assert_eq!((ca.vignette, ca.grain), (0.0, 0.0));
    }

    #[test]
    fn the_camera_carries_the_pass_only_while_the_look_asks_for_it() {
        let mut app = App::new();
        let cam = app.world_mut().spawn(Camera3d::default()).id();
        let has = |app: &App| app.world().get::<PhotoFx>(cam).copied();
        app.world_mut().run_system_once(update_cameras).unwrap();
        assert!(has(&app).is_none(), "no look, no pass");
        app.insert_resource(PhotoFxLook::default());
        app.world_mut().run_system_once(update_cameras).unwrap();
        assert!(
            has(&app).is_none(),
            "a look with every effect off adds no pass"
        );
        app.insert_resource(PhotoFxLook {
            vignette: 0.6,
            ..default()
        });
        app.world_mut().run_system_once(update_cameras).unwrap();
        assert_eq!(has(&app).unwrap().vignette, 0.6 * VIGNETTE_MAX);
        app.world_mut().remove_resource::<PhotoFxLook>();
        app.world_mut().run_system_once(update_cameras).unwrap();
        assert!(has(&app).is_none(), "the look gone, the pass is off");
    }
}
