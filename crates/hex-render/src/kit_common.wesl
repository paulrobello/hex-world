// `hex_render::kit_common`: shared by assets/shaders/kit_material.wesl (forward) and kit_prepass.wesl
// (depth/normal prepass), so both passes produce the same tiled world-space normal
// (crates/hex-render/src/kit_material.rs). Embedded with `load_shader_library!` rather than loaded
// as an asset: Bevy probes every asset-path WESL import as both a module and an item file
// (`shaders.wesl`, `shaders/kit_common/<item>.wesl`), which 404s on the web; a package import is
// resolved from registered shaders only.
import bevy_pbr::render::pbr_bindings;

struct KitParams {
    tile_inv_m: f32,
    flags: u32,
    snow: f32,
    wetness: f32,
    night: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

const FLAG_TILEABLE: u32 = 1u;
const FLAG_MASKS: u32 = 2u;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> kit: KitParams;

fn tileable() -> bool {
    return (kit.flags & FLAG_TILEABLE) != 0u;
}

// World-space tiling frame. A tileable set is bound in StandardMaterial's base_color / normal_map /
// metallic_roughness slots and sampled at `uv`. Texture "up" is -Z on up-facing surfaces (the
// Blender bake plane's +Y) and +Y on walls. Image rows run top-down, so v is negated. T, B, N match
// Blender's tangent space for the bake plane, so the OpenGL-convention normal map applies directly.
// Constraints: AO comes from masks.A, never the tile ORM's R; there is no back-face normal flip
// (tileable slots must be single-sided and opaque, enforced by `slot_requests`); the basis switches
// hard at |N.y| = 0.5, so slopes near 60 degrees show a UV/normal seam. The planned caps and sides
// avoid all three.
struct TileFrame {
    T: vec3<f32>,
    B: vec3<f32>,
    N: vec3<f32>,
    uv: vec2<f32>,
}

fn tile_frame(world_normal: vec3<f32>, world_position: vec3<f32>) -> TileFrame {
    let N = normalize(world_normal);
    var up = vec3(0.0, 1.0, 0.0);
    if (abs(N.y) >= 0.5) {
        up = vec3(0.0, 0.0, -1.0);
    }
    let B = normalize(up - N * dot(N, up));
    let T = cross(B, N);
    let uv = vec2(dot(world_position, T), -dot(world_position, B)) * kit.tile_inv_m;
    return TileFrame(T, B, N, uv);
}

// The tile normal map applied in the frame. Call only under `tileable()` (uniform control flow).
fn tiled_normal(f: TileFrame) -> vec3<f32> {
    let tn = textureSample(pbr_bindings::normal_map_texture, pbr_bindings::normal_map_sampler, f.uv).xyz * 2.0 - 1.0;
    return normalize(f.T * tn.x + f.B * tn.y + f.N * tn.z);
}
