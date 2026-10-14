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
// Skirt walls: the kit `soil` set's O4 cutaway (scripts/blender/kit/mat_soil.py), linear from its sRGB stops.
const WALL_BODY_LO: vec3<f32> = vec3(0.063, 0.036, 0.023);
const WALL_BODY_HI: vec3<f32> = vec3(0.102, 0.058, 0.033);
const WALL_GRAIN: vec3<f32> = vec3(0.023, 0.014, 0.0086);
const WALL_TOPSOIL: vec3<f32> = vec3(0.023, 0.014, 0.0091);
const WALL_FOOT: vec3<f32> = vec3(0.0091, 0.007, 0.0056);
const WALL_ROOTS: vec3<f32> = vec3(0.0116, 0.0075, 0.0052);
// Grass rim above sea level (water.rs LAND: world y = 0 in play) and the kit skirt height (LEVEL_STEP).
const WALL_RIM: f32 = 1.4;
const WALL_DEPTH: f32 = 4.0;

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

    // Skirt walls: the O4 cutaway on the kit skirts' world-height profile (depth below the grass rim over
    // the 4 m skirt), so a cut wall matches a neighbouring meadow wall at the same height. A grass lip
    // under a grass-level rim, root-dark topsoil, a warm brown body darkening softly into a near-black foot.
    // Wall-plane noise (along the wall, down it): the xz samples above draw vertical streaks on a wall.
    // 0.866x + 0.5z stretches no hex edge by more than 2x. Sampled here, as implicit-LOD sampling
    // needs uniform control flow.
    let wn = textureSample(terrain_noise_texture, terrain_noise_sampler, vec2(0.866 * wp.x + 0.5 * wp.z, wp.y) / 3.0);
    if (cap < 0.5) {
        let d = WALL_RIM - y;
        let tw = clamp(d / WALL_DEPTH, 0.0, 1.0);
        var wall = mix(WALL_BODY_LO, WALL_BODY_HI, wn.b);
        wall = mix(wall, WALL_GRAIN, (wn.a - 0.5) * 0.5);
        wall = mix(wall, WALL_TOPSOIL, 0.55 * (1.0 - smoothstep(0.04, 0.16, tw)));
        wall = mix(wall, WALL_FOOT, 0.85 * smoothstep(0.3, 1.0, tw + (nz.b - 0.5) * 0.15));
        let edge = 0.12 + (fine.g - 0.5) * 0.12;
        // Only a rim within the lip depth of the grass level gets a lip, and the cap is grass there.
        let lip = 1.0 - smoothstep(edge - 0.008, edge + 0.008, d);
        wall = mix(wall, WALL_ROOTS, 0.7 * (1.0 - smoothstep(0.0, 0.1, d - edge)));
        albedo = mix(wall, GRASS * (0.8 + 0.4 * nz.a), lip);
        rough = mix(0.95, 0.85, lip);
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
