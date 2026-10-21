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
    water_time: f32,
    wash_amp: f32,
    wind_amp: f32,
    // Linear leaf - flower colour of a FLAG_BLOSSOM slot (A 1): out of season a texel shifts by
    // its blossom coverage times this.
    blossom: vec4<f32>,
    // A FLAG_ANIMAL slot's rig (hex_render::animal): (hipY, front hip z, back hip z, swing rad) and
    // (neck pivot y, neck pivot z, head rad, bob rad). 0 on other slots.
    animal_rig: vec4<f32>,
    animal_head: vec4<f32>,
}

const FLAG_TILEABLE: u32 = 1u;
const FLAG_MASKS: u32 = 2u;
// A foliage slot: takes the per-instance seasonal tint (hex_render::hooks, crates/hex-render/src/hooks.rs).
const FLAG_FOLIAGE: u32 = 4u;
// Coast-bed sand: the live swash wet band (kit_material.wesl), on the water's clock in `water_time`.
const FLAG_WET_BAND: u32 = 8u;
// Sways in the wind (hex_render::foliage_wind, in the vertex stage of every pass) with amplitude
// `wind_amp`: a tree leans from its base by height and flexes by TEXCOORD_1 U, a plant bends by U².
const FLAG_WIND_TREE: u32 = 16u;
const FLAG_WIND_PLANT: u32 = 32u;
const FLAG_WET_SEABED: u32 = 64u;
// Seasonal blossom: the unique ORM's A is 1 - blossom coverage; a season index that hides blossom
// shifts it to leaf by `blossom` (kit_material.wesl, hooks.wesl `blossom_shown`).
const FLAG_BLOSSOM: u32 = 128u;
// A farm animal: walks and grazes by its MeshTag motion word (hex_render::animal_motion, in the
// vertex stage of every pass). Never a foliage slot, so the tag is not a season index there.
const FLAG_ANIMAL: u32 = 256u;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> kit: KitParams;

fn tileable() -> bool {
    return (kit.flags & FLAG_TILEABLE) != 0u;
}

// The normal transform `transpose(inverse(m))`, via the adjugate (WGSL has no `inverse`):
// for column-major columns a, b, c, the inverse's ROWS are cross(b,c), cross(c,a), cross(a,b),
// all over det = dot(a, cross(b,c)); packed as columns they are already transpose(inverse(m)).
// Transposing again yields inverse(m), which mirrors a yawed piece's normals about Y.
fn kit_normal_basis(m: mat3x3<f32>) -> mat3x3<f32> {
    let a = m[0];
    let b = m[1];
    let c = m[2];
    let r0 = cross(b, c);
    let det = dot(a, r0);
    return mat3x3(r0 / det, cross(c, a) / det, cross(a, b) / det);
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
