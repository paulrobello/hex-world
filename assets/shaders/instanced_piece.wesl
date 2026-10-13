// Instanced static kit pieces (`layout=instanced`, crates/hex-render/src/instanced.rs).
// Group 2 holds every piece, the 40-entry material palette and this frame's per-view id list;
// a draw's instance range indexes the id list, which indexes the pieces.
// One file serves the main pass (vertex_main/fragment_main, Bevy mesh-view bindings, PBR
// lighting with shadows; SSAO sampled here as pbr_input_from_standard_material does, since
// pbr_input_new() leaves occlusion at 1) and the prepass and shadow passes (vertex_prepass/
// fragment_prepass, prepass view bindings).

import bevy_pbr::render::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
};

@if(!INSTANCED_PREPASS)
import bevy_pbr::render::{
    pbr_types,
    pbr_functions,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};

@if(SCREEN_SPACE_AMBIENT_OCCLUSION && !INSTANCED_PREPASS)
import bevy_pbr::render::{
    mesh_view_bindings::screen_space_ambient_occlusion_texture,
    pbr_lighting,
};
@if(SCREEN_SPACE_AMBIENT_OCCLUSION && !INSTANCED_PREPASS)
import bevy_pbr::ssao::utils::ssao_multibounce;

@if(MOTION_VECTOR_PREPASS)
import bevy_pbr::prepass::bindings::previous_view_uniforms;

struct Piece {
    rotation: vec4<f32>,
    translation: vec3<f32>,
    material: u32,
    scale: vec3<f32>,
    pad: u32,
};

struct PaletteEntry {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    pad: f32,
};

@group(2) @binding(0) var<storage, read> pieces: array<Piece>;
@group(2) @binding(1) var<storage, read> palette: array<PaletteEntry>;
@group(2) @binding(2) var<storage, read> ids: array<u32>;

fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn world_position(p: Piece, local: vec3<f32>) -> vec3<f32> {
    return p.translation + rotate(p.rotation, local * p.scale);
}

// Inverse-transpose of rotation · scale, as the merged-chunk layout transforms normals.
fn world_normal(p: Piece, n: vec3<f32>) -> vec3<f32> {
    return normalize(rotate(p.rotation, n / p.scale));
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

// ---- Main pass ----------------------------------------------------------------------------

@if(!INSTANCED_PREPASS)
struct MainOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) @interpolate(flat) material: u32,
};

@if(!INSTANCED_PREPASS)
@vertex
fn vertex_main(v: Vertex) -> MainOut {
    let p = pieces[ids[v.instance_index]];
    let world = world_position(p, v.position);
    var out: MainOut;
    out.position = position_world_to_clip(world);
    out.world_position = vec4<f32>(world, 1.0);
    out.world_normal = world_normal(p, v.normal);
    out.material = p.material;
    return out;
}

@if(!INSTANCED_PREPASS)
@fragment
fn fragment_main(in: MainOut, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let m = palette[in.material];
    var pbr_input = pbr_types::pbr_input_new();
    pbr_input.material.base_color = m.base_color;
    pbr_input.material.emissive = m.emissive;
    pbr_input.material.perceptual_roughness = m.roughness;
    pbr_input.material.metallic = m.metallic;
    pbr_input.material.reflectance = vec3<f32>(m.reflectance);
    pbr_input.flags = MESH_FLAGS_SHADOW_RECEIVER_BIT;
    pbr_input.is_orthographic = view.clip_from_view[3].w == 1.0;
    pbr_input.V = pbr_functions::calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;
    pbr_input.world_normal = pbr_functions::prepare_world_normal(in.world_normal, false, is_front);
    pbr_input.N = normalize(pbr_input.world_normal);
@if(SCREEN_SPACE_AMBIENT_OCCLUSION) {
    // Ported from bevy_pbr pbr_fragment.wesl (pbr_input_from_standard_material).
    let NdotV = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    let ssao = textureLoad(
        screen_space_ambient_occlusion_texture,
        vec2<i32>(in.position.xy - view.viewport.xy),
        0i,
    ).r;
    pbr_input.diffuse_occlusion = min(vec3(1.0), ssao_multibounce(ssao, m.base_color.rgb));
    let roughness = pbr_lighting::perceptualRoughnessToRoughness(m.roughness);
    pbr_input.specular_occlusion =
        saturate(pow(NdotV + ssao, exp2(-16.0 * roughness - 1.0)) - 1.0 + ssao);
}
    var color = pbr_functions::apply_pbr_lighting(pbr_input);
    color = pbr_functions::main_pass_post_lighting_processing(pbr_input, color);
    return color;
}

// ---- Prepass and shadow passes --------------------------------------------------------------

@if(INSTANCED_PREPASS)
struct PrepassOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @location(2) unclipped_depth: f32,
};

@if(PREPASS_FRAGMENT)
struct PrepassFragment {
    @if(NORMAL_PREPASS)
    @location(0) normal: vec4<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(1) motion_vector: vec2<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @builtin(frag_depth) frag_depth: f32,
};

@if(INSTANCED_PREPASS)
@vertex
fn vertex_prepass(v: Vertex) -> PrepassOut {
    let p = pieces[ids[v.instance_index]];
    let world = world_position(p, v.position);
    var out: PrepassOut;
    out.position = position_world_to_clip(world);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
    out.world_position = vec4<f32>(world, 1.0);
    out.world_normal = world_normal(p, v.normal);
    return out;
}

@if(PREPASS_FRAGMENT)
@fragment
fn fragment_prepass(in: PrepassOut) -> PrepassFragment {
    var out: PrepassFragment;
@if(NORMAL_PREPASS)
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;
@if(MOTION_VECTOR_PREPASS) {
    // Static pieces: the previous world position is the current one.
    let clip_t = view.unjittered_clip_from_world * in.world_position;
    let prev_t = previous_view_uniforms.unjittered_clip_from_world * in.world_position;
    out.motion_vector = (clip_t.xy / clip_t.w - prev_t.xy / prev_t.w) * vec2(0.5, -0.5);
}
    return out;
}
