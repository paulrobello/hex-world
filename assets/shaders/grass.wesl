// Grass blades, forward pass (crates/hex-render/src/play/grass.rs). Custom Material: one uniform
// (binding 100, hex_render::grass_common) and no texture, so the pipeline samples only the view's
// textures. The pose is grass_common::pose, shared with grass_prepass.wesl; the position output is
// @invariant so this pass meets the prepass depth exactly. NORMAL carries the blade's base colour
// (the ground slot's tint times its grass set's mean albedo, linear), which the fragment darkens at
// the root and lightens toward the tip.
import bevy_pbr::render::{
    mesh_functions,
    mesh_bindings::mesh,
    mesh_view_bindings as vb,
    view_transformations::position_world_to_clip,
    forward_io::FragmentOutput,
    pbr_types,
    pbr_functions,
    pbr_lighting as lighting,
};
import hex_render::{grass_common as gc, hooks};

@if(SCREEN_SPACE_AMBIENT_OCCLUSION)
import bevy_pbr::ssao::utils::ssao_multibounce;

// Root shade (grass shading its own base), the tip's lift, and how much a gust lightens the bent
// blades so gusts read as rolling bands.
const ROOT_SHADE: f32 = 0.78;
const TIP_LIFT: vec3<f32> = vec3(1.4, 1.36, 1.0);
const GUST_LIFT: f32 = 0.6;
const ROUGHNESS: f32 = 0.78;
const REFLECTANCE: f32 = 0.3;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) blade: vec2<f32>,
    @location(3) shape: vec2<f32>,
}

struct GrassOut {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // Height fraction along the blade, gust level.
    @location(3) along_gust: vec2<f32>,
    @location(4) @interpolate(flat) instance_index: u32,
}

@vertex
fn vertex(v: Vertex) -> GrassOut {
    var out: GrassOut;
    let model = mesh_functions::get_world_from_local(v.instance_index);
    let p = gc::pose(model, v.position, v.blade, v.shape, vb::view.world_position, gc::grass.time);
    out.position = position_world_to_clip(p.world);
    out.world_position = vec4(p.world, 1.0);
    out.world_normal = p.normal;
    out.color = v.color;
    out.along_gust = vec2(v.blade.y, p.gust);
    out.instance_index = v.instance_index;
    return out;
}

@fragment
fn fragment(in: GrassOut) -> FragmentOutput {
    let a = in.along_gust.x;
    // Shade like the ground the blade stands on, tilted by the blade's bend.
    let n = normalize(mix(vec3(0.0, 1.0, 0.0), normalize(in.world_normal), 0.35));
    var c = in.color * mix(ROOT_SHADE, 1.0, smoothstep(0.0, 0.6, a));
    c = mix(c, c * TIP_LIFT, a * a) * (1.0 + GUST_LIFT * in.along_gust.y * (0.35 + 0.65 * a));
    // §9.4 foliage seasonal tint by the entity's MeshTag; 0 (every MVP tile) skips it.
    let season = mesh_functions::get_tag(in.instance_index);
    if (season != 0u) {
        c *= hooks::season_tint(season);
    }

    var pbr = pbr_types::pbr_input_new();
    pbr.flags = mesh[in.instance_index].flags;
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
