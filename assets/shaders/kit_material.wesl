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
@if(!KIT_INSTANCED)
import bevy_pbr::render::mesh_functions;
@if(KIT_INSTANCED)
import bevy_pbr::render::{mesh_types, pbr_functions};
@if(KIT_INSTANCED)
import hex_render::kit_instances;
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

// Dry sand's mean linear albedo (plaster set times the beach_sand tint) over the coast seabed's
// (gravel set times the beach_seabed tint), per channel; kit_material.rs holds it to the PNGs and
// the manifest.
const DRY_SAND_OVER_SEABED: vec3<f32> = vec3(1.8285, 1.8762, 1.953);

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
    // The instanced path binds kit records where bevy's per-mesh uniform sits, so the mesh flags
    // pbr_fragment read from `mesh[instance_index]` are record bytes. Restore what a kit entity
    // gets (a shadow receiver with a positive-determinant transform: kit records never mirror)
    // and redo the winding-dependent normal with them.
    @if(KIT_INSTANCED) {
    pbr_input.flags = mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT
        | mesh_types::MESH_FLAGS_SIGN_DETERMINANT_MODEL_3X3_BIT;
    let double_sided = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
    pbr_input.world_normal = pbr_functions::prepare_world_normal(
        in.world_normal,
        double_sided,
        is_front || foliage,
    );
    // Only a double-sided slot's N depends on the winding; single-sided pieces keep bevy's
    // normal-mapped N.
    @if(!LOAD_PREPASS_NORMALS) {
    if (double_sided) {
        pbr_input.N = normalize(pbr_input.world_normal);
    }
    }
    }

    if (kit_common::tileable()) {
        // pbr_input sampled the tile textures at UV0; base color, roughness, metallic and N are
        // all replaced here from the world-space samples (the prepass does the same for N).
        let f = kit_common::tile_frame(in.world_normal, in.world_position.xyz);
        var albedo = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, f.uv);
        let orm = textureSample(pbr_bindings::metallic_roughness_texture, pbr_bindings::metallic_roughness_sampler, f.uv);
        let factor = pbr_bindings::material.base_color;
        var tint = vec3(1.0);
        var n = kit_common::tiled_normal(f);
        // COLOR_0 on a tileable slot (the river mouth fan, scripts/blender/kit/_mouth.py): rgb scales
        // the slot tint and a is the detail weight, so slots of different sets can meet at one colour.
        // At a = 0 the set shows its mean (the top mip of the same texture, no added binding) and no
        // normal relief. The bound set must carry a full mip chain (the BC7 and ASTC families do).
        @if(VERTEX_COLORS) {
            tint = in.color.rgb;
            let mean = textureSampleLevel(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, f.uv, 16.0);
            albedo = vec4(mix(mean.rgb, albedo.rgb, in.color.a), albedo.a);
            n = normalize(mix(f.N, n, in.color.a));
        }
        pbr_input.material.base_color = vec4(factor.rgb * tint * albedo.rgb, factor.a);
        pbr_input.material.perceptual_roughness = orm.g;
        pbr_input.material.metallic = orm.b;
        pbr_input.N = n;
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
        // Run-up holds only on faces flat enough to keep it: cliff walls and steep banks stay dry,
        // so the band doesn't crawl up every exposed coast side (owner, 2026-10-18).
        let hold = smoothstep(0.3, 0.6, in.world_normal.y);
        let damp = 1.0 - smoothstep(0.0, 0.32 + 0.08 * water_common::value_noise(wp.xz / 7.0), y);
        let soaked = 1.0 - smoothstep(reach - 0.01, reach + 0.05, y);
        shore_wet = hold * clamp(max(damp * 0.75, soaked), 0.0, 1.0) * step(-0.02, y) + step(y, -0.02);
        @if(VERTEX_COLORS) {
            shore_wet *= in.color.a;  // the detail weight fades the live band too (mouth fan banks)
        }
        var c = pbr_input.material.base_color;
        // The seabed slot starts at the water at rest (`_beach.py` `slot_of`), so the retreating
        // swash bares its gravel next to dry sand. Above the water and in the shallows it takes the
        // sand slot's dry tint instead, fading back to its gravel by 0.3 m deep, and then darkens
        // like the sand: the shore reads as one wet sand from the run-up to the water.
        if ((kit_common::kit.flags & kit_common::FLAG_WET_SEABED) != 0u) {
            // The sample (gravel set times the slot tint) rescaled by dry sand's mean over the
            // gravel set's: the bed keeps its grain at sand's colour and brightness. Below the
            // shallows the bed is its own gravel, untouched, as before the band reached it.
            // Wet sand is smooth: most of the gravel's grain goes to the set's mean (its top mip).
            let to_gravel = smoothstep(-0.02, -0.3, y);
            let tiled = kit_common::tile_frame(in.world_normal, wp);
            let mean = textureSampleLevel(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, tiled.uv, 16.0).rgb * pbr_bindings::material.base_color.rgb;
            let smooth_bed = mix(mean, c.rgb, 0.3) * DRY_SAND_OVER_SEABED;
            c = vec4(mix(smooth_bed, c.rgb, to_gravel), c.a);
            shore_wet *= 1.0 - to_gravel;
        }
        pbr_input.material.base_color = vec4(c.rgb * mix(1.0, 0.55, shore_wet), c.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.18, shore_wet * step(-0.02, y));
    }

    // §9.4 seasonal blossom (FLAG_BLOSSOM, docs/kit/hooks-contract.md): the unique ORM's A is
    // 1 - blossom coverage. Sampled here, in uniform control flow, and used only by a season that
    // hides blossom.
    var blossom = 0.0;
    @if(VERTEX_UVS_A) {
    if ((kit_common::kit.flags & kit_common::FLAG_BLOSSOM) != 0u) {
        blossom = 1.0 - textureSample(pbr_bindings::metallic_roughness_texture, pbr_bindings::metallic_roughness_sampler, in.uv).a;
    }
    }

    // §9.4 foliage seasonal tint: the instance's MeshTag is its season index. Untagged instances
    // (every MVP instance) read 0 and skip it. Out of season, a blossom slot shifts each texel by
    // its blossom coverage × (leaf - flower) before the tint, which also clears the blossom's share
    // of texels that filtering blends with leaf.
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
    if ((kit_common::kit.flags & kit_common::FLAG_FOLIAGE) != 0u) {
        @if(KIT_INSTANCED) {
        let season = in.instance_index;
        }
        @else {
        let season = mesh_functions::get_tag(in.instance_index);
        }
        if (season != 0u) {
            var c = pbr_input.material.base_color;
            if (!hooks::blossom_shown(season)) {
                c = vec4(max(c.rgb + blossom * kit_common::kit.blossom.rgb, vec3(0.0)), c.a);
            }
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

    // Canopy gaps (tree_hero_oak bakes roughness 1.0 on the dark dome between its baked leaves): the
    // shell is smooth there and SSAO cannot see the leaf depth, so F0 0.04 still drew a sky sheen at
    // roughness 1. Bevy reads F0 below 0.02 as baked specular occlusion (no Fresnel, no IBL specular),
    // so unique foliage fades reflectance to 0 over roughness 0.95-1.0, after snow and rain lowered
    // it. No other foliage slot reaches 0.95 (slot roughness <= 0.9, unique foliage ORM G <= 0.88).
    if ((kit_common::kit.flags & kit_common::FLAG_FOLIAGE) != 0u && !kit_common::tileable()) {
        pbr_input.material.reflectance *= 1.0 - smoothstep(0.95, 1.0, pbr_input.material.perceptual_roughness);
    }

    // Debug LOD tint on the instanced path (kit_instances' meta pad words; the per-entity path
    // swaps in a tint material instead). Colours are hex_play::debug::LOD_TINTS.
    @if(KIT_INSTANCED) {
    if (kit_instances::kit_meta._pad0 != 0u) {
        var tint = vec3(0.266, 0.004, 0.330);
        if (kit_instances::kit_meta._pad1 == 0u) {
            tint = vec3(0.991, 0.906, 0.145);
        } else if (kit_instances::kit_meta._pad1 == 1u) {
            tint = vec3(0.129, 0.569, 0.549);
        }
        let lin = pow(tint, vec3(2.2));
        pbr_input.material.base_color = vec4(lin, pbr_input.material.base_color.a);
        pbr_input.material.emissive = vec4(lin * 0.25, pbr_input.material.emissive.a);
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
