// The foliage wind field of every settled kit part, once per frame (crates/hex-render/src/kit_instances.rs,
// `dispatch_kit_wind`): a tree samples the field at its trunk base, so every vertex of it would sample
// the same value in every pass (forward, prepass and each shadow cascade). The instanced kit vertex
// stages read it from `kit_fields` (hex_render::kit_instances) for tree slots instead. Plants sample
// at the vertex and keep their per-vertex field. The clock is the Globals time the vertex stages read.
import hex_render::foliage_wind;
import bevy_render::globals::Globals;

// hex_render::kit_instances' GpuKitInstance (48 bytes).
struct KitRecord {
    rotation: vec4<f32>,
    translation: vec3<f32>,
    tag: u32,
    scale: vec3<f32>,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read> records: array<KitRecord>;
@group(0) @binding(1) var<storage, read_write> fields: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> globals: Globals;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= min(arrayLength(&records), arrayLength(&fields))) {
        return;
    }
    let origin = records[id.x].translation;
    fields[id.x] = foliage_wind::pack(foliage_wind::field(origin, origin, true, globals.time));
}
