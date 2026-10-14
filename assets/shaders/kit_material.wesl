// Kit pieces (crates/hex-render/src/kit_material.rs). StandardMaterial + KitExt (bindings 100-102).
// A tileable set is bound in StandardMaterial's base_color / normal_map / metallic_roughness slots
// and sampled at world-space UVs (crates/hex-render/src/kit_common.wesl), so the extension adds only one sampled texture
// (masks). kit_prepass.wesl writes the same tiled normal into the normal prepass.
import bevy_pbr::render::{
    pbr_bindings,
    pbr_types,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};
import hex_render::kit_common;

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

    @if(VERTEX_UVS_A) {
    if ((kit_common::kit.flags & kit_common::FLAG_MASKS) != 0u) {
        let m = textureSample(masks_texture, masks_sampler, in.uv);
        pbr_input.diffuse_occlusion *= m.a;
        // §9.4 hooks: zero at the fixed noon; `kit_hooks=snow|wet|night` drives them for verification.
        let cover = m.r * kit_common::kit.snow * smoothstep(0.55, 0.85, pbr_input.N.y);
        let c0 = mix(pbr_input.material.base_color.rgb, vec3(0.90, 0.92, 0.95), cover);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.55, cover);
        let wet = m.g * kit_common::kit.wetness * (1.0 - cover);
        pbr_input.material.base_color = vec4(c0 * mix(1.0, 0.55, wet), pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.12, wet);
        pbr_input.material.emissive = vec4(pbr_input.material.emissive.rgb + c0 * m.b * kit_common::kit.night * 4.0, pbr_input.material.emissive.a);
    }
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
