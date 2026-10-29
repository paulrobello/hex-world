// `hex_render::kit_instances`: per-instance state for the kit instanced path
// (crates/hex-render/src/kit_instances.rs), bound at the per-mesh group (group 2): one 48-byte
// record per settled kit part, the per-view id list that maps draw instance indices to records,
// and the mesh metadata (decompress AABBs, UV ranges and the LOD band) of the slot's mesh, bound as a
// dynamic-offset uniform so each (slot, LOD) draw selects its own entry. The kit shaders read
// these under `KIT_INSTANCED`, replacing the per-mesh uniform that the batched path reads.
struct GpuKitInstance {
    rotation: vec4<f32>,
    translation: vec3<f32>,
    tag: u32,
    scale: vec3<f32>,
    _pad: u32,
}

struct KitMeshMeta {
    center: vec3<f32>,
    _pad0: u32,
    half: vec3<f32>,
    _pad1: u32,
    uv_a: vec4<f32>,
    uv_b: vec4<f32>,
    // The draw's LOD band edges (x, y, z, w) in metres, as bevy's `visibility_ranges` entries.
    lod_range: vec4<f32>,
}

@group(2) @binding(0) var<storage, read> kit_instances: array<GpuKitInstance>;
@group(2) @binding(1) var<uniform> kit_meta: KitMeshMeta;
// Each record's foliage wind field this frame (hex_render::foliage_wind::pack), written by
// assets/shaders/kit_wind.wesl before any pass draws; read for tree slots only.
@group(2) @binding(2) var<storage, read> kit_fields: array<vec4<f32>>;

/// The record this draw's instance refers to. The record id arrives as an instanced
/// vertex attribute (one u32 per instance, sliced per draw), because WebGPU's
/// `instance_index` is 0-based per draw while Metal's `[[instance_id]]` includes the
/// draw's instance start — an ids-storage lookup keyed on it reads the first slot's
/// records on WebGPU.
fn kit_instance(vid: u32) -> GpuKitInstance {
    return kit_instances[vid];
}

/// The packed foliage wind field of record `vid` (see `kit_fields`).
fn kit_field(vid: u32) -> vec4<f32> {
    return kit_fields[vid];
}

/// `world = translation · rotation · scale` from a record.
fn kit_world_from_local(i: GpuKitInstance) -> mat4x4<f32> {
    let x = i.rotation.x;
    let y = i.rotation.y;
    let z = i.rotation.z;
    let w = i.rotation.w;
    // Column-major basis = R * diag(scale).
    let c0 = vec3(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)) * i.scale.x;
    let c1 = vec3(2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)) * i.scale.y;
    let c2 = vec3(2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)) * i.scale.z;
    return mat4x4<f32>(
        vec4(c0, 0.0),
        vec4(c1, 0.0),
        vec4(c2, 0.0),
        vec4(i.translation, 1.0),
    );
}

/// The LOD cross-fade level of a band draw's part with origin `origin`, seen from `lod_eye`
/// (`view.lod_view_world_position`: the camera, or for a shadow cascade its camera): bevy_pbr's
/// `get_visibility_range_dither_level` (render/mesh_functions.wesl) on the draw's band instead of
/// the per-mesh visibility range, so `pbr_functions::visibility_range_dither` splits the band
/// between two LODs pixel for pixel exactly as for per-entity parts. A band draw also carries the
/// (slot, LOD)'s whole parts, which come out at level 0. LOD0 has no fade-in (x = y = 0): there
/// bevy divides by zero and leans on the saturating conversion, here the level is 0 outright.
fn kit_lod_dither_level(origin: vec3<f32>, lod_eye: vec3<f32>) -> i32 {
    let lod_range = kit_meta.lod_range;
    let camera_distance = length(lod_eye - origin);
    let offset = select(-16, 0, camera_distance >= lod_range.z);
    let bounds = select(lod_range.xy, lod_range.zw, camera_distance >= lod_range.z);
    if (bounds.y <= bounds.x) {
        return offset + 16;
    }
    let level = i32(round((camera_distance - bounds.x) / (bounds.y - bounds.x) * 16.0));
    return offset + clamp(level, 0, 16);
}
