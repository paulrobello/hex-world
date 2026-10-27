//! GPU time per pass (PRD §15 P2, ADR 0001 carry-over), opt-in with `gpu_timing=on`.
//!
//! Bevy's `RenderDiagnosticsPlugin` reports CPU time only on Metal and WebGPU: its spans need
//! timestamps inside encoders or passes, which neither exposes (and its encoder timestamps are a
//! no-op on macOS). What both expose is `timestamp-query` at pass boundaries. So marker systems sit
//! between Bevy's pass systems in `Core3d`, and each records a one-workgroup compute pass whose
//! `end_of_pass` write stamps the GPU clock there. A segment's time is the next marker's stamp
//! minus its own.
//!
//! Three things make the stamps mean "the passes before this marker finished" (each measured
//! natively on an M4 Max before it was added):
//! - Each marker reads one texel of the texture the passes before it wrote (the directional shadow
//!   map, the prepass depth, the SSAO output, or the view's main colour target), so the GPU finishes
//!   them before the marker, and the next pass that writes that texture waits for it. Without this,
//!   a pass's cost landed in the next segment.
//! - The stamp is the marker's end: on Metal a pass's start is sampled before its dependency wait.
//! - A query set is not a tracked resource on Metal, so a resolve in the same frame raced the last
//!   marker (38 to 64% of frames had a stale last stamp). Each frame writes one half of a doubled
//!   query set and resolves the half written a frame earlier. The results go to a ring of readback buffers that map without
//!   blocking, so a sample lands a few frames late, keyed by the frame it rendered (`FrameCount`).
//!   Frames whose stamps still run backwards are dropped and counted.
//!
//! All three are native only (`NATIVE_MARKERS`): in Chrome that build hung the page, so the web
//! keeps plain start-of-pass markers resolved in the same frame. There the frame total is usable,
//! but the split is not: in Chrome on the M4 Max most of the frame landed in `preprocess` and
//! `prepass` and the later segments read 0, so per-pass attribution comes from native runs.
//! Natively on Metal neither the split nor the total held against a Metal System Trace on the base
//! M4 (`UNTRUSTED_METAL`), so `diag().gpu.trusted` is false there and on the web.
//!
//! A frame whose camera had no colour target is refused (`GpuTimer::no_target`): natively an
//! occluded window gets no swap-chain texture, and the passes that draw to it never run.
//!
//! The dependencies remove the overlap between passes, so timing-on frames are slower than real
//! ones: frame-time baselines are measured with timing off. Timing starts at the camera's GPU
//! preprocessing (the boids dispatch before the camera is not measured) and ends after
//! tonemapping (the UI and upscale passes are not measured). Chrome quantizes timestamps to 100 µs
//! unless WebGPU developer features are on. `HEXW_GPU_TIMING_DEBUG=1` logs one frame's raw stamps
//! every 200 frames.
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};

use bevy::{
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        deferred::node::late_deferred_prepass,
        prepass::{ViewPrepassTextures, node::early_prepass},
        schedule::{Core3d, Core3dSystems},
        tonemapping::tonemapping,
    },
    diagnostic::FrameCount,
    ecs::schedule::IntoScheduleConfigs,
    pbr::{
        EARLY_SHADOW_PASS, LATE_SHADOW_PASS, ScreenSpaceAmbientOcclusionResources,
        ViewShadowBindings, allocate_uniforms, early_prepass_build_indirect_parameters,
        main_transmissive_pass_3d, per_view_shadow_pass, shared_shadow_pass,
    },
    post_process::bloom::bloom,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        renderer::{
            CurrentView, PendingCommandBuffers, RenderContext, RenderDevice, RenderGraph,
            RenderGraphSystems, RenderQueue,
        },
        view::ViewTarget,
    },
    ui_render::ui_pass,
};

/// Natively markers read the texture the passes before them wrote, stamp their end, and each frame
/// resolves the query-set half the previous frame wrote (see the module docs). In Chrome that
/// build hung the page a few seconds after READY, and a query resolved in a later submit than the
/// one that wrote it read 0; the web keeps the first design, which passed web-smoke: no texture
/// dependency, start-of-pass stamps, a same-frame resolve.
const NATIVE_MARKERS: bool = cfg!(not(target_arch = "wasm32"));
/// Markers per frame: one per segment start, plus `end`.
const MAX_MARKS: u32 = 32;
/// Readback buffers in flight.
const SLOTS: usize = 4;
/// GPU frames kept for the statistics (the diag window is 600 frames).
const RING: usize = 600;
/// GPU frames over 25 ms kept with their full breakdown.
const TAIL_KEEP: usize = 32;
const TAIL_MS: f32 = 25.0;

/// Segment names in GPU order. A marker named `X` starts segment `X`; `end` closes the last one.
pub const SEGMENTS: [&str; 10] = [
    "preprocess",
    "shadow",
    "prepass",
    "ssao",
    "opaque",
    "transmission",
    "transparent",
    "taa",
    "tilt_shift",
    "post",
];

/// What each segment covers, for the diag output and docs.
pub const SEGMENT_SPANS: [(&str, &str); 10] = [
    (
        "preprocess",
        "GPU mesh preprocessing: uniform allocation, culling, indirect args",
    ),
    ("shadow", "early directional and point shadow passes"),
    (
        "prepass",
        "depth/normal/motion prepass, depth downsample, late (occlusion) passes",
    ),
    (
        "ssao",
        "after the Prepass set until MainPass: SSAO, main indirect args",
    ),
    ("opaque", "main opaque pass (opaque, alpha mask, skybox)"),
    (
        "transmission",
        "main transmissive pass: screen copy plus refracting water (Medium and up)",
    ),
    (
        "transparent",
        "main transparent pass (alpha-blended water on Low) and other MainPass work",
    ),
    ("taa", "EarlyPostProcess: TAA"),
    ("tilt_shift", "tilt-shift pass"),
    (
        "post",
        "bloom, depth of field and tonemapping (ends after tonemapping)",
    ),
];

/// `diag().gpu.note` on Metal. On the base M4 (MiniMac, 2026-10-27, `--offscreen on`) a Metal System
/// Trace put the marker encoders where they belong, after the passes they wait for, yet the stamps
/// they wrote ran early: one frame read shadow 2.1, prepass 2.3, opaque 6.4 and transmission
/// 12.4 ms where the trace's markers bounded 7.0, 6.3, 10.5 and 1.8, so the missing time piled into
/// the later segments, and the split moved between runs. The total moved too: 19.5 ms in one run
/// and 26.2 ms in another of the same 33 ms frame, against about 28.4 ms of in-span GPU-busy time
/// in the trace. Why the end-of-encoder samples run early is not known.
const UNTRUSTED_METAL: &str = "not trusted on Metal: on the base M4 the per-pass stamps disagreed \
     with a Metal System Trace by 2-3x per segment and the total read 8-31% under the trace's \
     GPU-busy time; take GPU numbers from scripts/perf/low-camera-trace.sh";
/// `diag().gpu.note` on the web.
const UNTRUSTED_WEB: &str = "split not trusted on the web: start-of-pass markers cluster the \
     cost early, and the span leaves out upscaling and the browser's own passes";

/// `diag().gpu.reason` when every frame was refused for lack of a colour target.
const NO_TARGET_REASON: &str = "refused: the camera had no colour target (window occluded, minimized or \
     on a locked screen, so no swap-chain texture), so the main pass and post never ran";

/// One rendered frame's GPU time per segment (ms), in `SEGMENTS` order where present.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuFrame {
    /// `FrameCount` of the main-world frame this render frame drew.
    pub frame: u32,
    pub segments: Vec<(&'static str, f32)>,
    pub total_ms: f32,
    /// A stamp earlier than the one before it (overlap or a skipped pass); clamped to 0.
    pub non_monotonic: bool,
}

#[derive(Default)]
struct Published {
    supported: Option<bool>,
    /// The adapter is Metal (see `UNTRUSTED_METAL`).
    metal: bool,
    timestamp_period_ns: f32,
    ring: VecDeque<GpuFrame>,
    tail: VecDeque<GpuFrame>,
    frames: u64,
    /// Frames dropped because a stamp ran ahead of the one before it (see the module docs).
    rejected: u64,
    /// Frames dropped because the camera had no colour target, so its main pass and everything
    /// after it never ran (see `GpuTimer::no_target`).
    rejected_no_target: u64,
}

static PUBLISHED: Mutex<Option<Published>> = Mutex::new(None);

fn publish(f: impl FnOnce(&mut Published)) {
    let mut g = PUBLISHED.lock().unwrap();
    f(g.get_or_insert_with(Published::default));
}

/// GPU frames whose `frame` lies in `range`, newest last (for the stall-tail join in `diag`).
pub fn frames_in(range: std::ops::RangeInclusive<u32>) -> Vec<GpuFrame> {
    PUBLISHED
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| {
            p.ring
                .iter()
                .filter(|f| range.contains(&f.frame))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * sorted.len() as f32).ceil() as usize;
    sorted[idx.clamp(1, sorted.len()) - 1]
}

fn round2(v: f32) -> f64 {
    (f64::from(v) * 100.0).round() / 100.0
}

/// Mean, p50, p95 and max of one series.
pub fn stats(values: &[f32]) -> serde_json::Value {
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(f32::total_cmp);
    let mean = if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    };
    serde_json::json!({
        "mean": round2(mean),
        "p50": round2(percentile(&v, 50.0)),
        "p95": round2(percentile(&v, 95.0)),
        "max": round2(v.last().copied().unwrap_or(0.0)),
    })
}

fn frame_json(f: &GpuFrame) -> serde_json::Value {
    serde_json::json!({
        "frame": f.frame,
        "total_ms": round2(f.total_ms),
        "segments": f.segments.iter().map(|(n, ms)| (n.to_string(), serde_json::json!(round2(*ms))))
            .collect::<serde_json::Map<_, _>>(),
    })
}

/// The `gpu` object of the diag snapshot: `null` fields until timing is on and supported.
pub fn diag_json(enabled: bool) -> serde_json::Value {
    let g = PUBLISHED.lock().unwrap();
    let p = g.as_ref();
    let supported = p.and_then(|p| p.supported);
    let reason = match (enabled, supported) {
        (false, _) => "off (gpu_timing=on enables it)",
        (true, None) => "waiting for the render device",
        (true, Some(false)) => "adapter lacks timestamp-query: CPU frame times only",
        (true, Some(true)) => "pass-boundary timestamps",
    };
    let Some(p) = p.filter(|p| enabled && p.supported == Some(true) && !p.ring.is_empty()) else {
        let no_target = p.map_or(0, |p| p.rejected_no_target);
        let reason = if enabled && supported == Some(true) && no_target > 0 {
            NO_TARGET_REASON
        } else {
            reason
        };
        return serde_json::json!({
            "enabled": enabled, "supported": supported, "reason": reason,
            "frames": 0, "rejected_no_target": no_target,
            "passes": serde_json::Value::Null, "total": serde_json::Value::Null,
            "tail_over_25ms": [],
        });
    };
    let passes: serde_json::Map<String, serde_json::Value> = SEGMENTS
        .iter()
        .map(|name| {
            let v: Vec<f32> = p
                .ring
                .iter()
                .filter_map(|f| f.segments.iter().find(|(n, _)| n == name).map(|s| s.1))
                .collect();
            (name.to_string(), stats(&v))
        })
        .collect();
    let totals: Vec<f32> = p.ring.iter().map(|f| f.total_ms).collect();
    let note = if !NATIVE_MARKERS {
        Some(UNTRUSTED_WEB)
    } else if p.metal {
        Some(UNTRUSTED_METAL)
    } else {
        None
    };
    serde_json::json!({
        "enabled": enabled,
        "supported": true,
        "reason": reason,
        "trusted": note.is_none(),
        "note": note,
        "timestamp_period_ns": p.timestamp_period_ns,
        "frames": p.ring.len(),
        "frames_total": p.frames,
        "rejected_non_monotonic": p.rejected,
        "rejected_no_target": p.rejected_no_target,
        "passes": passes,
        "total": stats(&totals),
        "spans": SEGMENT_SPANS.iter().map(|(n, s)| (n.to_string(), serde_json::json!(s)))
            .collect::<serde_json::Map<_, _>>(),
        "tail_over_25ms": p.tail.iter().map(frame_json).collect::<Vec<_>>(),
    })
}

/// Segments from one frame's stamps: segment `labels[i]` runs from stamp `i` to stamp `i + 1`.
pub fn segments(
    labels: &[&'static str],
    stamps: &[u64],
    period_ns: f32,
) -> (Vec<(&'static str, f32)>, f32, bool) {
    // Stamps after `end` (the sentinel) are not part of the frame.
    let n = labels
        .iter()
        .position(|l| *l == "end")
        .map_or(labels.len(), |i| i + 1)
        .min(stamps.len());
    let mut out = vec![];
    let mut non_monotonic = false;
    for i in 0..n.saturating_sub(1) {
        let (a, b) = (stamps[i], stamps[i + 1]);
        non_monotonic |= b < a;
        let ms = (b.saturating_sub(a) as f64 * f64::from(period_ns) / 1e6) as f32;
        // Several cameras or a marker that fired twice: sum into the first entry of that name.
        match out.iter_mut().find(|(l, _)| *l == labels[i]) {
            Some((_, t)) => *t += ms,
            None => out.push((labels[i], ms)),
        }
    }
    let total = if n >= 2 {
        (stamps[n - 1].saturating_sub(stamps[0]) as f64 * f64::from(period_ns) / 1e6) as f32
    } else {
        0.0
    };
    (out, total, non_monotonic)
}

/// The texture marker `i` reads, so the GPU orders it after the passes that wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dep {
    /// The directional shadow map array (written by the shadow passes).
    Shadow,
    /// The prepass depth (written by the prepass).
    PrepassDepth,
    /// The view's main colour target (everything from the opaque pass on).
    Main,
    /// The SSAO output, falling back to the prepass depth on tiers without SSAO.
    Ssao,
}

/// Marker index to its dependency (see `SEGMENTS`: marker `i` starts segment `i`).
fn dep(i: usize) -> Dep {
    match i {
        2 => Dep::Shadow,
        3 => Dep::PrepassDepth,
        4 => Dep::Ssao,
        _ => Dep::Main,
    }
}

struct Slot {
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// 0 free, 1 written (resolved next frame), 3 resolved (mapped after submit), 4 mapping,
    /// 2 mapped.
    state: Arc<AtomicU8>,
    labels: Vec<&'static str>,
    frame: u32,
    /// First query index of the half this slot's frame wrote.
    base: u32,
    /// The frame's camera had no colour target (`GpuTimer::no_target`).
    no_target: bool,
}

#[derive(Resource)]
struct GpuTimer {
    query_set: wgpu::QuerySet,
    scratch: wgpu::Buffer,
    /// Destination of the scratch copy that orders the resolve after the last marker.
    sink: wgpu::Buffer,
    /// No dependency (fallback when the view lacks the texture).
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    /// Per `Dep`: the pipeline reading one texel of that texture kind.
    dep_pipelines: [wgpu::ComputePipeline; 3],
    slots: Vec<Slot>,
    /// The slot this frame writes, if one was free.
    current: Option<usize>,
    /// The slot the previous frame wrote, resolved this frame.
    written: Option<usize>,
    /// The query-set half this frame writes (0 or 1).
    half: u32,
    labels: Vec<&'static str>,
    /// The view that owns this frame's markers (the first camera to reach one).
    view: Option<Entity>,
    /// A marker that reads the main colour target found none: the camera got no `ViewTarget`
    /// (natively, wgpu hands out no swap-chain texture while the window is occluded, and Bevy then
    /// skips every pass that draws to the target), so the later markers stamp back to back and
    /// the frame's split would read the main pass and post as 0.
    no_target: bool,
    period_ns: f32,
}

/// Whether `gpu_timing=on` was given (read by `diag`).
#[derive(Resource, Clone, Copy, Debug)]
pub struct GpuTiming {
    pub enabled: bool,
}

pub struct GpuTimingPlugin {
    pub enabled: bool,
}

impl Plugin for GpuTimingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GpuTiming {
            enabled: self.enabled,
        });
        if !self.enabled {
            return;
        }
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        use Core3dSystems::*;
        render_app
            .add_systems(RenderStartup, init_timer)
            .add_systems(
                Render,
                begin_frame
                    .after(RenderSystems::ExtractCommands)
                    .before(RenderSystems::PrepareAssets),
            )
            .add_systems(
                RenderGraph,
                (
                    resolve
                        .after(RenderGraphSystems::Render)
                        .before(RenderGraphSystems::Submit),
                    finish.in_set(RenderGraphSystems::Finish),
                ),
            )
            .add_systems(
                Core3d,
                (
                    mark::<0>.before(allocate_uniforms),
                    mark::<1>
                        .after(early_prepass_build_indirect_parameters)
                        .before(per_view_shadow_pass::<EARLY_SHADOW_PASS>)
                        .before(shared_shadow_pass::<EARLY_SHADOW_PASS>)
                        .before(early_prepass),
                    mark::<2>
                        .after(per_view_shadow_pass::<EARLY_SHADOW_PASS>)
                        .after(shared_shadow_pass::<EARLY_SHADOW_PASS>)
                        .before(early_prepass),
                    // Last in the Prepass set, so SSAO (after Prepass, private) follows it.
                    mark::<3>
                        .in_set(Prepass)
                        .after(late_deferred_prepass)
                        .after(per_view_shadow_pass::<LATE_SHADOW_PASS>)
                        .after(shared_shadow_pass::<LATE_SHADOW_PASS>),
                    mark::<4>.in_set(MainPass).before(main_opaque_pass_3d),
                    mark::<5>
                        .in_set(MainPass)
                        .after(main_opaque_pass_3d)
                        .before(main_transmissive_pass_3d),
                    mark::<6>
                        .in_set(MainPass)
                        .after(main_transmissive_pass_3d)
                        .before(main_transparent_pass_3d),
                    mark::<7>.after(MainPass).before(EarlyPostProcess),
                    mark::<8>
                        .in_set(PostProcess)
                        .after(EarlyPostProcess)
                        .before(crate::tilt_shift::TiltShiftPass),
                    mark::<9>
                        .in_set(PostProcess)
                        .after(crate::tilt_shift::TiltShiftPass)
                        .before(bloom),
                    mark::<10>
                        .in_set(PostProcess)
                        .after(tonemapping)
                        .before(ui_pass),
                ),
            );
    }
}

fn init_timer(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    adapter: Res<bevy::render::renderer::RenderAdapterInfo>,
    mut commands: Commands,
) {
    let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let period_ns = queue.get_timestamp_period();
    let metal = adapter.backend == wgpu::Backend::Metal;
    publish(|p| {
        p.supported = Some(supported);
        p.metal = metal;
        p.timestamp_period_ns = period_ns;
    });
    info!("gpu_timing: timestamp-query {supported}, period {period_ns} ns");
    if !supported {
        return;
    }
    let d = device.wgpu_device();
    let query_set = d.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("gpu_timing"),
        ty: wgpu::QueryType::Timestamp,
        count: MAX_MARKS * 2,
    });
    // A one-workgroup write to a scratch buffer: real work, so no backend skips the marker pass.
    let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gpu_timing_marker"),
        source: wgpu::ShaderSource::Wgsl(
            "@group(0) @binding(0) var<storage, read_write> scratch: u32;\n\
             @compute @workgroup_size(1) fn main() { scratch = scratch + 1u; }"
                .into(),
        ),
    });
    let pipeline = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gpu_timing_marker"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let scratch = d.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_timing_scratch"),
        size: 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let bind_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gpu_timing_marker"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: scratch.as_entire_binding(),
        }],
    });
    let float_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gpu_timing_marker_float"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let float_pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gpu_timing_marker_float"),
        bind_group_layouts: &[Some(&float_layout)],
        immediate_size: 0,
    });
    let dep_pipeline = |texture: &str, load: &str, layout: Option<&wgpu::PipelineLayout>| {
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_timing_marker_dep"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "@group(0) @binding(0) var<storage, read_write> scratch: u32;\n\
                     @group(0) @binding(1) var dep: {texture};\n\
                     @compute @workgroup_size(1) fn main() {{ scratch = scratch + u32({load} > 2.0); }}"
                )
                .into(),
            ),
        });
        d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gpu_timing_marker_dep"),
            layout,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let dep_pipelines = [
        dep_pipeline(
            "texture_depth_2d_array",
            "textureLoad(dep, vec2(0i), 0i, 0i)",
            None,
        ),
        dep_pipeline("texture_depth_2d", "textureLoad(dep, vec2(0i), 0i)", None),
        dep_pipeline(
            "texture_2d<f32>",
            "textureLoad(dep, vec2(0i), 0i).x",
            Some(&float_pipeline_layout),
        ),
    ];
    let bytes = u64::from(MAX_MARKS) * 8;
    let slots = (0..SLOTS)
        .map(|_| Slot {
            resolve: d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu_timing_resolve"),
                size: bytes,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu_timing_read"),
                size: bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            state: Arc::new(AtomicU8::new(0)),
            labels: vec![],
            frame: 0,
            base: 0,
            no_target: false,
        })
        .collect();
    commands.insert_resource(GpuTimer {
        query_set,
        scratch,
        sink: d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_timing_sink"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        pipeline,
        bind_group,
        dep_pipelines,
        slots,
        current: None,
        written: None,
        half: 0,
        labels: vec![],
        view: None,
        no_target: false,
        period_ns,
    });
}

/// Collects mapped slots into the published ring, then claims a free slot for this frame.
fn begin_frame(timer: Option<ResMut<GpuTimer>>, frame: Res<FrameCount>) {
    let Some(mut timer) = timer else {
        return;
    };
    let period = timer.period_ns;
    for slot in &mut timer.slots {
        if slot.state.load(Ordering::Acquire) != 2 {
            continue;
        }
        let n = slot.labels.len() as u64;
        let stamps: Option<Vec<u64>> = slot.read.slice(..n * 8).get_mapped_range().ok().map(|v| {
            v.as_chunks::<8>()
                .0
                .iter()
                .map(|&c| u64::from_le_bytes(c))
                .collect()
        });
        slot.read.unmap();
        let Some(stamps) = stamps else {
            slot.state.store(0, Ordering::Release);
            continue;
        };
        if slot.frame % 200 == 0 && std::env::var_os("HEXW_GPU_TIMING_DEBUG").is_some() {
            let base = stamps.first().copied().unwrap_or(0);
            let rel: Vec<String> = slot
                .labels
                .iter()
                .zip(&stamps)
                .map(|(l, s)| format!("{l}:{:.3}", (*s as f64 - base as f64) / 1e6))
                .collect();
            info!("gpu_timing frame {}: {}", slot.frame, rel.join(" "));
        }
        let no_target = slot.no_target;
        let (segments, total_ms, non_monotonic) = segments(&slot.labels, &stamps, period);
        let f = GpuFrame {
            frame: slot.frame,
            segments,
            total_ms,
            non_monotonic,
        };
        publish(|p| {
            p.frames += 1;
            if no_target {
                p.rejected_no_target += 1;
                return;
            }
            if f.non_monotonic {
                p.rejected += 1;
                return;
            }
            if f.total_ms > TAIL_MS {
                if p.tail.len() == TAIL_KEEP {
                    p.tail.pop_front();
                }
                p.tail.push_back(f.clone());
            }
            if p.ring.len() == RING {
                p.ring.pop_front();
            }
            p.ring.push_back(f);
        });
        slot.state.store(0, Ordering::Release);
    }
    timer.labels.clear();
    timer.view = None;
    timer.no_target = false;
    timer.current = timer
        .slots
        .iter()
        .position(|s| s.state.load(Ordering::Acquire) == 0);
    timer.half ^= 1;
    let base = timer.half * MAX_MARKS;
    if let Some(i) = timer.current {
        timer.slots[i].frame = frame.0;
        timer.slots[i].base = base;
    }
}

fn mark<const I: usize>(
    timer: Option<ResMut<GpuTimer>>,
    view: Option<Res<CurrentView>>,
    views: Query<(
        Entity,
        Option<&ViewTarget>,
        Option<&ViewPrepassTextures>,
        Option<&ViewShadowBindings>,
        Option<&ScreenSpaceAmbientOcclusionResources>,
    )>,
    mut ctx: RenderContext,
) {
    let Some(mut timer) = timer else {
        return;
    };
    if timer.current.is_none() || timer.labels.len() >= MAX_MARKS as usize {
        return;
    }
    // The first camera to reach a marker owns the frame's markers.
    let v = view.map(|v| v.0);
    match timer.view {
        None => timer.view = v,
        Some(owner) if Some(owner) != v => return,
        Some(_) => {}
    }
    let index = timer.half * MAX_MARKS + timer.labels.len() as u32;
    let label = match I.cmp(&SEGMENTS.len()) {
        std::cmp::Ordering::Less => SEGMENTS[I],
        std::cmp::Ordering::Equal => "end",
        std::cmp::Ordering::Greater => "sentinel",
    };
    timer.labels.push(label);
    let owner = timer.view;
    let row = views.iter().find(|(e, ..)| Some(*e) == owner);
    let d = dep(I);
    if d == Dep::Main && row.is_some_and(|(_, target, ..)| target.is_none()) {
        timer.no_target = true;
    }
    let timer = &*timer;
    // (pipeline index, texture): 0 depth array, 1 depth, 2 float.
    let texture: Option<(usize, &wgpu::TextureView)> =
        row.and_then(|(_, target, prepass, shadow, ssao)| {
            let depth = || prepass.and_then(|p| p.depth_only_view()).map(|v| (1, &**v));
            match d {
                Dep::Shadow => shadow.map(|s| (0, &*s.directional_light_depth_texture_view)),
                Dep::PrepassDepth => depth(),
                Dep::Ssao => ssao
                    .map(|s| (2, &*s.screen_space_ambient_occlusion_texture.default_view))
                    .or_else(depth),
                Dep::Main => target
                    .filter(|t| t.main_texture().sample_count() == 1)
                    .map(|t| (2, &**t.main_texture_view())),
            }
        });
    let dep_bind_group = texture.filter(|_| NATIVE_MARKERS).map(|(k, view)| {
        let pipeline = &timer.dep_pipelines[k];
        let bg = ctx
            .render_device()
            .wgpu_device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu_timing_marker_dep"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: timer.scratch.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                ],
            });
        (pipeline, bg)
    });
    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gpu_timing_marker"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &timer.query_set,
                // Natively the end of the marker pass: on Metal its start is sampled before the
                // dependency wait, so only the end says when the passes before it finished.
                beginning_of_pass_write_index: (!NATIVE_MARKERS).then_some(index),
                end_of_pass_write_index: NATIVE_MARKERS.then_some(index),
            }),
        });
    match &dep_bind_group {
        Some((pipeline, bg)) => {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bg, &[]);
        }
        None => {
            pass.set_pipeline(&timer.pipeline);
            pass.set_bind_group(0, &timer.bind_group, &[]);
        }
    }
    pass.dispatch_workgroups(1, 1, 1);
}

/// Resolves this frame's stamps into its slot and copies them to the mappable buffer.
fn resolve(
    timer: Option<ResMut<GpuTimer>>,
    device: Res<RenderDevice>,
    mut pending: ResMut<PendingCommandBuffers>,
) {
    let Some(mut timer) = timer else {
        return;
    };
    let previous = timer.written.take();
    if let Some(i) = timer.current {
        let labels = std::mem::take(&mut timer.labels);
        if labels.len() < 2 {
            timer.current = None;
        } else {
            let no_target = timer.no_target;
            let slot = &mut timer.slots[i];
            slot.labels = labels;
            slot.no_target = no_target;
            slot.state.store(1, Ordering::Release);
            timer.written = Some(i);
        }
    }
    let target = if NATIVE_MARKERS {
        previous
    } else {
        timer.written.take()
    };
    let Some(w) = target else {
        return;
    };
    let slot = &timer.slots[w];
    let n = slot.labels.len() as u32;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu_timing_resolve"),
    });
    // Also orders this encoder after every marker that wrote the scratch buffer.
    encoder.copy_buffer_to_buffer(&timer.scratch, 0, &timer.sink, 0, Some(4));
    encoder.resolve_query_set(&timer.query_set, slot.base..slot.base + n, &slot.resolve, 0);
    encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.read, 0, Some(u64::from(n) * 8));
    pending.push_encoder(encoder, "gpu_timing_resolve");
    slot.state.store(3, Ordering::Release);
}

/// After submit: maps the slot without waiting; `begin_frame` reads it once mapped.
fn finish(timer: Option<ResMut<GpuTimer>>) {
    let Some(mut timer) = timer else {
        return;
    };
    timer.current = None;
    for slot in &timer.slots {
        if slot.state.load(Ordering::Acquire) != 3 {
            continue;
        }
        slot.state.store(4, Ordering::Release);
        let state = slot.state.clone();
        let n = slot.labels.len() as u64;
        slot.read
            .slice(..n * 8)
            .map_async(wgpu::MapMode::Read, move |r| {
                state.store(if r.is_ok() { 2 } else { 0 }, Ordering::Release);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_from_stamps() {
        let labels = ["prepass", "preprocess", "shadow", "end"];
        let stamps = [1_000_000, 1_500_000, 3_500_000, 4_000_000];
        let (s, total, bad) = segments(&labels, &stamps, 1.0);
        assert_eq!(
            s,
            vec![("prepass", 0.5), ("preprocess", 2.0), ("shadow", 0.5)]
        );
        assert_eq!(total, 3.0);
        assert!(!bad);
    }

    #[test]
    fn period_scales_and_backwards_stamps_clamp() {
        let (s, total, bad) = segments(&["a", "b", "end"], &[100, 50, 300], 1000.0);
        assert_eq!(s, vec![("a", 0.0), ("b", 0.25)]);
        assert_eq!(total, 0.2);
        assert!(bad);
    }

    #[test]
    fn repeated_labels_sum_and_short_input_is_empty() {
        let (s, _, _) = segments(&["a", "a", "end"], &[0, 1_000_000, 3_000_000], 1.0);
        assert_eq!(s, vec![("a", 3.0)]);
        let (s, total, _) = segments(&["a"], &[5], 1.0);
        assert!(s.is_empty() && total == 0.0);
    }

    #[test]
    fn stamps_after_end_are_ignored() {
        let (s, total, bad) = segments(&["a", "end", "sentinel"], &[0, 2_000_000, 1], 1.0);
        assert_eq!((s, total, bad), (vec![("a", 2.0)], 2.0, false));
    }

    #[test]
    fn segment_tables_agree() {
        assert_eq!(SEGMENTS.len(), SEGMENT_SPANS.len());
        for (a, (b, _)) in SEGMENTS.iter().zip(SEGMENT_SPANS) {
            assert_eq!(*a, b);
        }
    }

    #[test]
    fn stats_and_diag_json_shape() {
        let s = stats(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(s["mean"], 2.5);
        assert_eq!(s["p95"], 4.0);
        let off = diag_json(false);
        assert_eq!(off["enabled"], false);
        assert!(off["passes"].is_null());
        serde_json::to_string(&off).unwrap();
    }
}
