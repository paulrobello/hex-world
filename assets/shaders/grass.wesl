// Grass blades, forward pass (crates/hex-render/src/play/grass.rs, instanced by
// play/grass_gpu.rs): the shared blade mesh's corner at location 0 and the blade's instance record
// at 1-4. Group 2 holds two uniforms (hex_render::grass_common) and no texture, so the pipeline
// samples only the view's textures. The pose is grass_common::pose, shared with
// grass_prepass.wesl; the position output is @invariant so this pass meets the prepass depth
// exactly. The instance colour is the blade's base colour (the ground slot's tint times its grass
// set's mean albedo, linear), which the fragment darkens at the root and lightens toward the tip.
import bevy_pbr::render::{
    mesh_view_bindings as vb,
    view_transformations::position_world_to_clip,
    forward_io::FragmentOutput,
    pbr_types,
    pbr_functions,
    pbr_lighting as lighting,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};
import hex_render::{grass_common as gc, hooks};

@if(SCREEN_SPACE_AMBIENT_OCCLUSION)
import bevy_pbr::ssao::utils::ssao_multibounce;

// Root shade (grass shading its own base); the tip colour, lighter and less saturated than the
// blade's (TIP_DESAT toward its luminance, then TIP_LIFT), mixed in linearly to TIP_MIX at the tip
// (owner 2026-10-14, after small-world's grass); and how much a gust lightens the bent blades so
// gusts read as rolling bands.
const ROOT_SHADE: f32 = 0.78;
const TIP_DESAT: f32 = 0.3;
const TIP_LIFT: vec3<f32> = vec3(1.8, 1.8, 1.6);
const TIP_MIX: f32 = 0.85;
const GUST_LIFT: f32 = 0.6;
const ROUGHNESS: f32 = 0.78;
const REFLECTANCE: f32 = 0.3;

struct Vertex {
    @location(0) corner: vec3<f32>,
    @location(1) root: vec3<f32>,
    @location(2) yaw_rank: f32,
    @location(3) size: vec2<f32>,
    @location(4) color: vec4<f32>,
}

struct GrassOut {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // Height fraction along the blade, gust level.
    @location(3) along_gust: vec2<f32>,
    @location(4) @interpolate(flat) season: u32,
}

@vertex
fn vertex(v: Vertex) -> GrassOut {
    var out: GrassOut;
    let b = gc::blade(v.corner, v.yaw_rank, v.size);
    let p = gc::pose(gc::tile.world_from_local, v.root, b.blade, b.shape, vb::view.world_position, gc::grass.time);
    out.position = position_world_to_clip(p.world);
    out.world_position = vec4(p.world, 1.0);
    out.world_normal = p.normal;
    out.color = v.color.rgb;
    out.along_gust = vec2(b.blade.y, p.gust);
    out.season = gc::tile.tag;
    return out;
}

@fragment
fn fragment(in: GrassOut) -> FragmentOutput {
    let a = in.along_gust.x;
    // Shade like the ground the blade stands on, tilted by the blade's bend.
    let n = normalize(mix(vec3(0.0, 1.0, 0.0), normalize(in.world_normal), 0.35));
    let tip = mix(in.color, vec3(dot(in.color, vec3(0.2126, 0.7152, 0.0722))), TIP_DESAT) * TIP_LIFT;
    var c = in.color * mix(ROOT_SHADE, 1.0, smoothstep(0.0, 0.6, a));
    c = mix(c, tip, TIP_MIX * a) * (1.0 + GUST_LIFT * in.along_gust.y * (0.35 + 0.65 * a));
    // §9.4 foliage seasonal tint by the group's MeshTag; 0 (every MVP tile) skips it.
    if (in.season != 0u) {
        c *= hooks::season_tint(in.season);
    }

    var pbr = pbr_types::pbr_input_new();
    pbr.flags = MESH_FLAGS_SHADOW_RECEIVER_BIT;
    pbr.is_orthographic = vb::view.clip_from_view[3].w == 1.0;
    pbr.V = pbr_functions::calculate_view(in.world_position, pbr.is_orthographic);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    pbr.world_normal = n;
    pbr.N = n;
    pbr.material.base_color = vec4(c, 1.0);
    pbr.material.perceptual_roughness = ROUGHNESS;
    pbr.material.reflectance = vec3(REFLECTANCE);
    pbr.material.metallic = 0.0;

@if(SCREEN_SPACE_AMBIENT_OCCLUSION) {
    let ssao = textureLoad(vb::screen_space_ambient_occlusion_texture, vec2<i32>(in.position.xy - vb::view.viewport.xy), 0i).r;
    pbr.diffuse_occlusion = min(pbr.diffuse_occlusion, ssao_multibounce(ssao, c));
    let n_dot_v = max(dot(n, pbr.V), 1e-4);
    let roughness = lighting::perceptualRoughnessToRoughness(ROUGHNESS);
    pbr.specular_occlusion = saturate(pow(n_dot_v + ssao, exp2(-16.0 * roughness - 1.0)) - 1.0 + ssao);
}

    var out: FragmentOutput;
    out.color = pbr_functions::apply_pbr_lighting(pbr);
    out.color = pbr_functions::main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
