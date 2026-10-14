// Kit pieces (crates/hex-render/src/kit_material.rs). StandardMaterial + KitExt (bindings 100-102).
// A tileable set is bound in StandardMaterial's base_color / normal_map / metallic_roughness slots
// and sampled at world-space UVs (crates/hex-render/src/kit_common.wesl), so the extension adds only one sampled texture
// (masks). kit_prepass.wesl writes the same tiled normal into the normal prepass.
import bevy_pbr::render::{
    mesh_functions,
    pbr_bindings,
    pbr_types,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};
import hex_render::{kit_common, hooks, water_common};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

@group(constants::MATERIAL_BIND_GROUP) @binding(101) var masks_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var masks_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // Foliage (an alpha-masked, double-sided slot such as leaf cards) shades its back faces with the
    // front normal: the card normals are authored as one rounded canopy (spherical-normal transfer),
    // and Bevy's back-face flip would turn half of them away from the sky (hero oak look pass).
    let flags = pbr_bindings::material.flags;
    let foliage = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS)
        == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK;
    var pbr_input = pbr_input_from_standard_material(in, is_front || foliage);

    if (kit_common::tileable()) {
        // pbr_input sampled the tile textures at UV0; base color, roughness, metallic and N are
        // all replaced here from the world-space samples (the prepass does the same for N).
        let f = kit_common::tile_frame(in.world_normal, in.world_position.xyz);
        let albedo = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, f.uv);
        let orm = textureSample(pbr_bindings::metallic_roughness_texture, pbr_bindings::metallic_roughness_sampler, f.uv);
        let factor = pbr_bindings::material.base_color;
        pbr_input.material.base_color = vec4(factor.rgb * albedo.rgb, factor.a);
        pbr_input.material.perceptual_roughness = orm.g;
        pbr_input.material.metallic = orm.b;
        pbr_input.N = kit_common::tiled_normal(f);
    }

    // Live wet band on coast-bed sand (FLAG_WET_BAND, docs/kit/water-contract.md §3): the look-dev
    // placeholder bed's band (water_terrain.wesl) on the water surface's swash at the same world
    // point, so the soaked line follows the run-up. Damp sand darkens and glosses up to 0.32-0.40 m
    // above the water; sand the swash reaches is wetter still. The slot's own tint is dry sand
    // (owner, 2026-10-14), so the band owns the whole gradient, as the placeholder darkens its sand.
    var shore_wet = 0.0;
    if ((kit_common::kit.flags & kit_common::FLAG_WET_BAND) != 0u) {
        let wp = in.world_position.xyz;
        let y = wp.y - water_common::COAST_WATER_Y;
        let reach = kit_common::kit.wash_amp * water_common::swash(water_common::coast_shore(y), kit_common::kit.water_time);
        let damp = 1.0 - smoothstep(0.0, 0.32 + 0.08 * water_common::value_noise(wp.xz / 7.0), y);
        let soaked = 1.0 - smoothstep(reach - 0.01, reach + 0.05, y);
        shore_wet = clamp(max(damp * 0.75, soaked), 0.0, 1.0) * step(-0.02, y) + step(y, -0.02);
        let c = pbr_input.material.base_color;
        pbr_input.material.base_color = vec4(c.rgb * mix(1.0, 0.55, shore_wet), c.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.18, shore_wet * step(-0.02, y));
    }

    // §9.4 foliage seasonal tint: the instance's MeshTag is its season index. Untagged instances
    // (every MVP instance) read 0 and skip it.
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
    if ((kit_common::kit.flags & kit_common::FLAG_FOLIAGE) != 0u) {
        let season = mesh_functions::get_tag(in.instance_index);
        if (season != 0u) {
            let c = pbr_input.material.base_color;
            pbr_input.material.base_color = vec4(c.rgb * hooks::season_tint(season), c.a);
        }
    }
    }

    // §9.4 weather and night hooks (hex_render::hooks): zero at the fixed noon; `kit_hooks=` drives
    // them for verification. A slot without a masks map takes the uniform default, only while a
    // hook is on. Rain wets only what the wet band left dry, so wet sand never darkens twice.
    @if(VERTEX_UVS_A) {
    if ((kit_common::kit.flags & kit_common::FLAG_MASKS) != 0u) {
        let m = textureSample(masks_texture, masks_sampler, in.uv);
        pbr_input.diffuse_occlusion *= m.a;
        let cover = m.r * kit_common::kit.snow * smoothstep(0.55, 0.85, pbr_input.N.y);
        let c0 = mix(pbr_input.material.base_color.rgb, vec3(0.90, 0.92, 0.95), cover);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.55, cover);
        let wet = m.g * kit_common::kit.wetness * (1.0 - cover) * (1.0 - shore_wet);
        pbr_input.material.base_color = vec4(c0 * mix(1.0, 0.55, wet), pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.12, wet);
        pbr_input.material.emissive = vec4(pbr_input.material.emissive.rgb + c0 * m.b * kit_common::kit.night * 4.0, pbr_input.material.emissive.a);
    }
    }
    if ((kit_common::kit.flags & kit_common::FLAG_MASKS) == 0u && hooks::any_on(kit_common::kit.snow, kit_common::kit.wetness, kit_common::kit.night)) {
        pbr_input = hooks::surface(pbr_input, hooks::NO_MASKS, kit_common::kit.snow, kit_common::kit.wetness * (1.0 - shore_wet), kit_common::kit.night);
    }

    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
    var out: FragmentOutput;
@if(PREPASS_PIPELINE) {
    out = deferred_output(in, pbr_input);
} @else {
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
}
    return out;
}
