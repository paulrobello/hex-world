// Placeholder terrain for the water scenario (crates/hex-render/src/water.rs). Custom Material:
// bindings 100-102 only; default mesh vertex and prepass shaders. Grass plateau, beach sand with
// the wet band and the live swash line (same `swash` as the water surface), river banks and beds.
// Vertex COLOR: (land, river, shore distance, 1 on the cap / 0 on the skirt).
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_bindings::mesh,
    mesh_view_bindings as vb,
    pbr_types,
    pbr_functions,
    pbr_lighting as lighting,
};
import hex_render::water_common as wc;

@if(SCREEN_SPACE_AMBIENT_OCCLUSION)
import bevy_pbr::ssao::utils::ssao_multibounce;

struct TerrainParams {
    terrain_time: f32,
    wash_amp: f32,
    sea_level: f32,
    _pad: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> terrain: TerrainParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var terrain_noise_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var terrain_noise_sampler: sampler;

const GRASS: vec3<f32> = vec3(0.075, 0.16, 0.035);
const SAND: vec3<f32> = vec3(0.50, 0.42, 0.29);
const SAND_UNDER: vec3<f32> = vec3(0.40, 0.35, 0.25);
const SOIL: vec3<f32> = vec3(0.16, 0.115, 0.075);
const GRAVEL: vec3<f32> = vec3(0.21, 0.19, 0.16);
const STRATA: vec3<f32> = vec3(0.20, 0.15, 0.10);

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let wp = in.world_position.xyz;
    let land = in.color.x;
    let river = in.color.y;
    let shore = in.color.z;
    let cap = in.color.w;
    let beach = clamp(1.0 - land - river, 0.0, 1.0);
    let t = terrain.terrain_time;
    let y = wp.y - terrain.sea_level;

    let nz = textureSample(terrain_noise_texture, terrain_noise_sampler, wp.xz / 7.0);
    let fine = textureSample(terrain_noise_texture, terrain_noise_sampler, wp.xz / 1.3);
    var n = normalize(in.world_normal);
    let slope = 1.0 - n.y;

    // Ground cover: grass above ~1 m on gentle slopes, sand on the beach, soil and gravel along the
    // river, shading darker under water.
    let grass_w = smoothstep(0.82, 1.08, y + (nz.b - 0.5) * 0.35) * (1.0 - smoothstep(0.25, 0.45, slope));
    let low = mix(mix(SOIL, GRAVEL, smoothstep(0.2, -0.6, y)), SAND, beach);
    let under = mix(SAND_UNDER, GRAVEL * 0.9, river);
    var albedo = mix(low, under, smoothstep(0.05, -0.35, y));
    albedo = mix(albedo, GRASS * (0.8 + 0.4 * nz.a), grass_w);
    albedo *= 0.85 + 0.3 * fine.b;
    var rough = mix(0.92, 0.85, grass_w);

    // Wet band: sand the swash reached recently stays dark and glossy, decaying over ~0.3 m above
    // the waterline; the live run-up line is wetter still.
    let reach = terrain.wash_amp * wc::swash(shore, t) * (1.0 - river);
    let damp = (1.0 - smoothstep(0.0, 0.32 + 0.08 * nz.b, y)) * (1.0 - grass_w);
    let soaked = 1.0 - smoothstep(reach - 0.01, reach + 0.05, y);
    let wet = clamp(max(damp * 0.75, soaked), 0.0, 1.0) * step(-0.02, y) + step(y, -0.02);
    albedo *= mix(1.0, 0.55, wet);
    rough = mix(rough, 0.18, wet * step(-0.02, y));

    // Skirt walls: banded soil, a diorama cut.
    if (cap < 0.5) {
        albedo = STRATA * (0.75 + 0.35 * fract(wp.y * 0.6 + nz.b * 0.4));
        rough = 0.95;
    }

    let ripple = (fine.xy * 2.0 - 1.0) * mix(0.12, 0.04, grass_w);
    n = normalize(n + vec3(ripple.x, 0.0, ripple.y) * cap);

    var pbr = pbr_types::pbr_input_new();
    pbr.flags = mesh[in.instance_index].flags;
    pbr.is_orthographic = vb::view.clip_from_view[3].w == 1.0;
    pbr.V = pbr_functions::calculate_view(in.world_position, pbr.is_orthographic);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    pbr.world_normal = normalize(in.world_normal);
    pbr.N = n;
    pbr.material.base_color = vec4(albedo, 1.0);
    pbr.material.perceptual_roughness = rough;
    pbr.material.metallic = 0.0;

@if(SCREEN_SPACE_AMBIENT_OCCLUSION) {
    let ssao = textureLoad(vb::screen_space_ambient_occlusion_texture, vec2<i32>(in.position.xy - vb::view.viewport.xy), 0i).r;
    pbr.diffuse_occlusion = min(pbr.diffuse_occlusion, ssao_multibounce(ssao, albedo));
    let n_dot_v = max(dot(n, pbr.V), 1e-4);
    let roughness = lighting::perceptualRoughnessToRoughness(rough);
    pbr.specular_occlusion = saturate(pow(n_dot_v + ssao, exp2(-16.0 * roughness - 1.0)) - 1.0 + ssao);
}

    var out: FragmentOutput;
    out.color = pbr_functions::apply_pbr_lighting(pbr);
    out.color = pbr_functions::main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
