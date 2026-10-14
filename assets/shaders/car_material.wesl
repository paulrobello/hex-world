// Car kit pieces (crates/hex-render/src/car_material.rs). StandardMaterial + CarExt (bindings 100-102).
// Runtime recolour (PRD §6): COLOR_0.a marks the paintable surfaces (1 on paint, 0 on glass, tyres,
// lights and trim) and COLOR_0.rgb is each surface's own colour, which StandardMaterial has already
// multiplied into the base colour. Where the mask is set the instance's paint colour is multiplied
// in: it arrives as the per-entity MeshTag (sRGB8 packed 0xBBGGRR), which lives in the mesh uniform,
// so every car of one type and LOD stays in one instanced batch whatever its colour. `masks` (raw
// UV0) carries the §9.4 hooks as in kit_material.wesl: R snow, G wetness, B night emissive (head and
// tail lights), A baked AO. The extension adds one sampled texture, the same as the kit material.
import bevy_pbr::render::{
    mesh_functions,
    pbr_bindings,
    pbr_types,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};

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

struct CarParams {
    snow: f32,
    wetness: f32,
    night: f32,
    car_flags: u32,
}

const CAR_FLAG_MASKS: u32 = 1u;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> car: CarParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var car_masks_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var car_masks_sampler: sampler;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    @if(VERTEX_COLORS) {
        let paint = srgb_to_linear(unpack4x8unorm(mesh_functions::get_tag(in.instance_index)).rgb);
        let base = pbr_input.material.base_color.rgb;
        pbr_input.material.base_color = vec4(mix(base, base * paint, in.color.a), 1.0);
    }

    @if(VERTEX_UVS_A) {
    if ((car.car_flags & CAR_FLAG_MASKS) != 0u) {
        let m = textureSample(car_masks_texture, car_masks_sampler, in.uv);
        pbr_input.diffuse_occlusion *= m.a;
        let cover = m.r * car.snow * smoothstep(0.55, 0.85, pbr_input.N.y);
        let c0 = mix(pbr_input.material.base_color.rgb, vec3(0.90, 0.92, 0.95), cover);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.55, cover);
        let wet = m.g * car.wetness * (1.0 - cover);
        pbr_input.material.base_color = vec4(c0 * mix(1.0, 0.55, wet), pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.12, wet);
        pbr_input.material.emissive = vec4(pbr_input.material.emissive.rgb + c0 * m.b * car.night * 4.0, pbr_input.material.emissive.a);
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
