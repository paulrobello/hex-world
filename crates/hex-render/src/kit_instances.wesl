// `hex_render::kit_instances`: per-instance state for the kit instanced path
// (crates/hex-render/src/kit_instances.rs), bound at the per-mesh group (group 2): one 48-byte
// record per settled kit part, the per-view id list that maps draw instance indices to records,
// and the mesh metadata (decompress AABBs and UV ranges) of the slot's mesh, bound as a
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
}

@group(2) @binding(0) var<storage, read> kit_instances: array<GpuKitInstance>;
@group(2) @binding(1) var<uniform> kit_meta: KitMeshMeta;

/// The record this draw's instance refers to. The record id arrives as an instanced
/// vertex attribute (one u32 per instance, sliced per draw), because WebGPU's
/// `instance_index` is 0-based per draw while Metal's `[[instance_id]]` includes the
/// draw's instance start — an ids-storage lookup keyed on it reads the first slot's
/// records on WebGPU.
fn kit_instance(vid: u32) -> GpuKitInstance {
    return kit_instances[vid];
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
