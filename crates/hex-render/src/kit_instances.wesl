// `hex_render::kit_instances`: per-instance records for the kit instanced path
// (crates/hex-render/src/kit_instances.rs), bound at the per-mesh group (group 2): one 48-byte
// record per settled kit part, plus the per-view id list that maps draw instance indices to
// records. The kit shaders read it under `KIT_INSTANCED`.
struct GpuKitInstance {
    rotation: vec4<f32>,
    translation: vec3<f32>,
    tag: u32,
    scale: vec3<f32>,
    _pad: u32,
}

@group(2) @binding(0) var<storage, read> kit_instances: array<GpuKitInstance>;
@group(2) @binding(1) var<storage, read> kit_ids: array<u32>;

/// The record this draw's instance index refers to.
fn kit_instance(i: u32) -> GpuKitInstance {
    return kit_instances[kit_ids[i]];
}

/// `world = translation · rotation · scale` from a record.
fn kit_world_from_local(i: GpuKitInstance) -> mat4x4<f32> {
    let (x, y, z, w) = (i.rotation.x, i.rotation.y, i.rotation.z, i.rotation.w);
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
