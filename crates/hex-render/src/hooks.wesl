// `hex_render::hooks`: the PRD §9.4 surface hooks (D7) for assets/shaders/kit_material.wesl and
// car_material.wesl (crates/hex-render/src/hooks.rs). Embedded like kit_common. Declares no bindings
// and samples nothing, so a hook never adds a sampled texture. `surface` serves slots without a masks
// map; the masked paths keep the owner-approved inline copy of the same math (tests/hooks/mod.rs
// holds all three to it). Every hook is inert at the MVP values (snow, wetness and night 0, season
// index 0), and the callers skip the paths that only exist for the hooks.
import bevy_pbr::render::pbr_types;

// The masks of a slot that binds no masks map: full snow accumulation on up-facing faces, full
// porosity, no night emitters and no AO. Callers use it only while a weather hook is on.
const NO_MASKS: vec4<f32> = vec4(1.0, 1.0, 0.0, 1.0);

fn any_on(snow: f32, wetness: f32, night: f32) -> bool {
    return snow > 0.0 || wetness > 0.0 || night > 0.0;
}

// masks: R snow accumulation, G wetness/porosity, B night emission, A baked AO. Snow lies only on
// up-facing faces; wet surfaces darken and gloss; emitters glow by the night factor.
fn surface(input: pbr_types::PbrInput, m: vec4<f32>, snow: f32, wetness: f32, night: f32) -> pbr_types::PbrInput {
    var pbr_input = input;
    pbr_input.diffuse_occlusion *= m.a;
    let cover = m.r * snow * smoothstep(0.55, 0.85, pbr_input.N.y);
    let c0 = mix(pbr_input.material.base_color.rgb, vec3(0.90, 0.92, 0.95), cover);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.55, cover);
    let wet = m.g * wetness * (1.0 - cover);
    pbr_input.material.base_color = vec4(c0 * mix(1.0, 0.55, wet), pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.12, wet);
    pbr_input.material.emissive = vec4(pbr_input.material.emissive.rgb + c0 * m.b * night * 4.0, pbr_input.material.emissive.a);
    return pbr_input;
}

// Foliage seasonal tint by the instance's index (its MeshTag): 0 is summer, the MVP look, and
// multiplies by exactly 1. 1 and 2 are early and late autumn (crates/hex-render/src/hooks.rs).
fn season_tint(index: u32) -> vec3<f32> {
    switch index {
        case 1u: {
            return vec3(1.30, 0.80, 0.30);
        }
        case 2u: {
            return vec3(1.15, 0.48, 0.20);
        }
        default: {
            return vec3(1.0);
        }
    }
}
