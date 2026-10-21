// Kit prepass (crates/hex-render/src/kit_material.rs, `KitExt::prepass_vertex_shader` and
// `KitExt::prepass_fragment_shader`), which also draws every shadow cascade.
//
// Vertex: Bevy 0.20's bevy_pbr prepass/prepass.wesl vertex plus the foliage wind
// (hex_render::foliage_wind), the same call as kit_vertex.wesl, so depth, normals and shadows move
// with the forward pass; the position is @invariant in both. The previous position for motion
// vectors is the offset at the previous frame's wind clock under the previous transform.
//
// Fragment:
// Tileable slots write the world-space tiled normal from hex_render::kit_common, the same one the
// forward pass lights with, so SSAO reads the normal the surface is shaded with (faded by the COLOR_0
// detail weight on meshes that carry it, as kit_material.wesl does). Every other slot
// keeps Bevy 0.20's StandardMaterial prepass (bevy_pbr render/pbr_prepass.wesl, non-bindless and
// non-meshlet only, which KitExt always is): alpha discard, geometric normal, unique normal map.
//
// `KIT_INSTANCED` swaps the entry point (same contract as kit_vertex.wesl): settled parts draw in
// one call per (slot, LOD) through hex_render::kit_instances, so the per-mesh uniform and every
// module that imports it (prepass::io) stay out of the composed shader. The instanced entry
// decompresses against the per-slot metadata uniform; settled parts never move, so the previous
// transform is the transform. Both branches share the `shade_kit_prepass_vertex` tail.
import bevy_pbr::render::{
    pbr_prepass_functions,
    pbr_bindings,
    pbr_types,
    pbr_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
};
@if(!KIT_INSTANCED)
import bevy_pbr::prepass::io as prepass_io;
@if(!KIT_INSTANCED)
import bevy_pbr::render::{mesh_functions, skinning};
@if(!KIT_INSTANCED)
import bevy_pbr::render::mesh_bindings::mesh;
@if(!KIT_INSTANCED)
import bevy_pbr::render::morph::{morph_position, morph_normal, morph_tangent};
@if(!KIT_INSTANCED)
import bevy_pbr::prepass::io::decompress_vertex;
import bevy_render::globals::Globals;
import bevy_render::utils;
import hex_render::{kit_common, foliage_wind, animal_motion};
@if(KIT_INSTANCED)
import hex_render::kit_instances;

// The prepass and shadow views bind Globals at 1 (bevy_pbr prepass/mod.rs view layouts); the main
// pass's mesh_view_bindings::globals is at 11 and absent here.
@group(0) @binding(1) var<uniform> prepass_globals: Globals;

// prepass_io::VertexOutput's locations, with the position @invariant.
struct KitPrepassOutput {
    @invariant @builtin(position) position: vec4<f32>,
    @if(VERTEX_UVS_A)
    @location(0) uv: vec2<f32>,
    @if(VERTEX_UVS_B)
    @location(1) uv_b: vec2<f32>,
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    @location(2) world_normal: vec3<f32>,
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
    @location(3) world_tangent: vec4<f32>,
    @location(4) world_position: vec4<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(5) previous_world_position: vec4<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @location(6) unclipped_depth: f32,
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    @location(7) @interpolate(flat) instance_index: u32,
    @if(VERTEX_COLORS)
    @location(8) color: vec4<f32>,
    @if(VISIBILITY_RANGE_DITHER)
    @location(9) @interpolate(flat) visibility_range_dither: i32,
}

// prepass_io's vertex types, mirrored locally (the instanced entry cannot import prepass::io).
struct KitPrepassIn {
@if(KIT_INSTANCED)
    @location(6) kit_record: u32,
@if(!KIT_INSTANCED)
    @builtin(instance_index) instance_index: u32,
@if(VERTEX_POSITIONS_COMPRESSED)
    @location(0) compressed_position: vec4<f32>,
@else
    @location(0) position: vec3<f32>,
@if(VERTEX_UVS_A && VERTEX_UVS_A_COMPRESSED)
    @location(1) compressed_uv: vec2<f32>,
@if(VERTEX_UVS_A && !VERTEX_UVS_A_COMPRESSED)
    @location(1) uv: vec2<f32>,
@if(VERTEX_UVS_B && VERTEX_UVS_B_COMPRESSED)
    @location(2) compressed_uv_b: vec2<f32>,
@if(VERTEX_UVS_B && !VERTEX_UVS_B_COMPRESSED)
    @location(2) uv_b: vec2<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS && VERTEX_NORMALS_COMPRESSED)
    @location(3) compressed_normal: vec2<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS && !VERTEX_NORMALS_COMPRESSED)
    @location(3) normal: vec3<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS && VERTEX_TANGENTS_COMPRESSED)
    @location(4) compressed_tangent: vec2<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS && !VERTEX_TANGENTS_COMPRESSED)
    @location(4) tangent: vec4<f32>,
@if(VERTEX_COLORS)
    @location(7) color: vec4<f32>,
}

struct KitPrepassUncompressed {
    position: vec3<f32>,
@if(VERTEX_UVS_A)
    uv: vec2<f32>,
@if(VERTEX_UVS_B)
    uv_b: vec2<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
    normal: vec3<f32>,
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
    tangent: vec4<f32>,
@if(VERTEX_COLORS)
    color: vec4<f32>,
}

// prepass_io::FragmentOutput.
struct KitFragmentOutput {
@if(NORMAL_PREPASS)
    @location(0) normal: vec4<f32>,
@if(MOTION_VECTOR_PREPASS)
    @location(1) motion_vector: vec2<f32>,
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @builtin(frag_depth) frag_depth: f32,
}

// prepass_sample_color_and_alpha_discard over KitPrepassOutput (bevy's takes prepass_io's type).
fn kit_prepass_sample_color_and_alpha_discard(in: KitPrepassOutput) {
@if(MAY_DISCARD) {
    var output_color: vec4<f32> = pbr_bindings::material.base_color;
    let flags = pbr_bindings::material.flags;
    let alpha_cutoff = pbr_bindings::material.alpha_cutoff;
@if(VERTEX_UVS) {
@if(STANDARD_MATERIAL_BASE_COLOR_UV_B)
    var uv = in.uv_b;
@else
    var uv = in.uv;
    let uv_transform = pbr_bindings::material.uv_transform;
    uv = (uv_transform * vec3(uv, 1.0)).xy;
    if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        output_color = output_color * textureSampleBias(
            pbr_bindings::base_color_texture,
            pbr_bindings::base_color_sampler,
            uv,
            view.mip_bias
        );
    }
}
    pbr_prepass_functions::prepass_alpha_discard(flags, alpha_cutoff, output_color);
}
}

@if(MORPH_TARGETS && !KIT_INSTANCED)
fn morph_vertex(vertex_in: prepass_io::UncompressedVertex, instance_index: u32) -> prepass_io::UncompressedVertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let morph_descriptor_index = mesh[instance_index].morph_descriptor_index;
    let weight_count = bevy_pbr::render::morph::layer_count(morph_descriptor_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = bevy_pbr::render::morph::weight_at(i, morph_descriptor_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, morph_descriptor_index);
@if(VERTEX_NORMALS)
        vertex.normal += weight * morph_normal(vertex_index, i, morph_descriptor_index);
@if(VERTEX_TANGENTS)
        vertex.tangent += vec4(weight * morph_tangent(vertex_index, i, morph_descriptor_index), 0.0);
    }
    return vertex;
}

@if(MORPH_TARGETS && !KIT_INSTANCED)
fn morph_prev_vertex(vertex_in: prepass_io::UncompressedVertex, instance_index: u32) -> prepass_io::UncompressedVertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let morph_descriptor_index = mesh[instance_index].morph_descriptor_index;
    let weight_count = bevy_pbr::render::morph::layer_count(morph_descriptor_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = bevy_pbr::render::morph::weight_at(i, morph_descriptor_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, morph_descriptor_index);
    }
    return vertex;
}

// The wind offset of a vertex at rest world position `world` (zero on still slots and on meshes
// without TEXCOORD_1), at wind time `t`, under the instance transform whose origin is `origin`.
fn wind_offset(world: vec3<f32>, origin: vec3<f32>, sway: vec2<f32>, t: f32) -> vec3<f32> {
    let wind = kit_common::kit.flags & (kit_common::FLAG_WIND_TREE | kit_common::FLAG_WIND_PLANT);
    if (wind == 0u) {
        return vec3(0.0);
    }
    return foliage_wind::offset(world, origin, sway, kit_common::kit.wind_amp, wind == kit_common::FLAG_WIND_TREE, t);
}

// The shading tail both entries share: wind displacement, clip, varyings. Local
// normal/tangent transforms (inverse-transpose) because bevy's mesh_functions helpers read the
// per-mesh uniform, which the instanced path does not bind.
fn shade_kit_prepass_vertex(
    vertex: KitPrepassUncompressed,
    world_from_local: mat4x4<f32>,
    wind_origin: vec3<f32>,
    tag: u32,
) -> KitPrepassOutput {
    var out: KitPrepassOutput;
    let basis = mat3x3(world_from_local[0].xyz, world_from_local[1].xyz, world_from_local[2].xyz);
    let normal_basis = kit_common::kit_normal_basis(basis);

    out.world_position = world_from_local * vec4<f32>(vertex.position, 1.0);
@if(VERTEX_UVS_B)
    out.world_position = vec4(out.world_position.xyz + wind_offset(
        out.world_position.xyz, wind_origin, vertex.uv_b, prepass_globals.time), 1.0);
    out.position = position_world_to_clip(out.world_position.xyz);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;

@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS) {
@if(VERTEX_NORMALS)
    out.world_normal = normalize(normal_basis * vertex.normal);
@if(VERTEX_TANGENTS)
    out.world_tangent = vec4(normalize(basis * vertex.tangent.xyz), vertex.tangent.w);
}

@if(VERTEX_COLORS)
    out.color = vertex.color;

@if(MOTION_VECTOR_PREPASS) {
    // Settled parts never move: the previous position is the position.
    let prev_world_position = world_from_local * vec4<f32>(vertex.position, 1.0);
    var prev_origin = wind_origin;
    var prev_sway = vec2(0.0);
@if(VERTEX_UVS_B) {
    prev_sway = vertex.uv_b;
@if(VERTEX_COLORS) {
    if (vertex.color.a < 0.0) {
        prev_origin = wind_origin;
    }
}
}
    out.previous_world_position = vec4(prev_world_position.xyz + wind_offset(
        prev_world_position.xyz, prev_origin, prev_sway, prepass_globals.time - prepass_globals.delta_time), 1.0);
}

@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = tag;

    return out;
}

@if(KIT_INSTANCED)
@vertex
fn vertex(v: KitPrepassIn) -> KitPrepassOutput {
    // Decompress from kit_instances' per-slot metadata (mirrors prepass_io::decompress_vertex).
    var vertex: KitPrepassUncompressed;
@if(VERTEX_POSITIONS_COMPRESSED)
    vertex.position = bevy_render::utils::decompress_vertex_position(v.compressed_position, kit_instances::kit_meta.center, kit_instances::kit_meta.half);
@else
    vertex.position = v.position;
@if(VERTEX_UVS_A && VERTEX_UVS_A_COMPRESSED)
    vertex.uv = bevy_render::utils::decompress_vertex_uv(v.compressed_uv, kit_instances::kit_meta.uv_a);
@if(VERTEX_UVS_A && !VERTEX_UVS_A_COMPRESSED)
    vertex.uv = v.uv;
@if(VERTEX_UVS_B && VERTEX_UVS_B_COMPRESSED)
    vertex.uv_b = bevy_render::utils::decompress_vertex_uv(v.compressed_uv_b, kit_instances::kit_meta.uv_b);
@if(VERTEX_UVS_B && !VERTEX_UVS_B_COMPRESSED)
    vertex.uv_b = v.uv_b;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS && VERTEX_NORMALS_COMPRESSED)
    vertex.normal = bevy_render::utils::decompress_vertex_normal(v.compressed_normal);
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS && !VERTEX_NORMALS_COMPRESSED)
    vertex.normal = v.normal;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS && VERTEX_TANGENTS_COMPRESSED)
    vertex.tangent = bevy_render::utils::decompress_vertex_tangent(v.compressed_tangent);
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS && !VERTEX_TANGENTS_COMPRESSED)
    vertex.tangent = v.tangent;
@if(VERTEX_COLORS)
    vertex.color = v.color;
    let inst = kit_instances::kit_instance(v.kit_record);
    return shade_kit_prepass_vertex(vertex, kit_instances::kit_world_from_local(inst), inst.translation, inst.tag);
}

@if(!KIT_INSTANCED)
@vertex
fn vertex(vertex_no_morph: prepass_io::Vertex) -> KitPrepassOutput {
    let uncompressed_vertex_no_morph = prepass_io::decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
@if(MORPH_TARGETS)
    var vertex = morph_vertex(uncompressed_vertex_no_morph, vertex_no_morph.instance_index);
@else
    var vertex = uncompressed_vertex_no_morph;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
    var wind_origin = mesh_world_from_local[3].xyz;

    var local: KitPrepassUncompressed;
    local.position = vertex.position;
@if(VERTEX_UVS_A)
    local.uv = vertex.uv;
@if(VERTEX_UVS_B)
    local.uv_b = vertex.uv_b;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
    local.normal = vertex.normal;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
    local.tangent = vertex.tangent;
@if(VERTEX_COLORS)
    local.color = vertex.color;
    // The farm animal pose, as kit_vertex.wesl (the previous position for motion vectors takes the
    // same pose: the per-entity path holds no previous motion word).
@if(VERTEX_UVS_B) {
    if ((kit_common::kit.flags & kit_common::FLAG_ANIMAL) != 0u) {
        var n = vec3(0.0, 1.0, 0.0);
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
        n = local.normal;
        var t = vec3(1.0, 0.0, 0.0);
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
        t = local.tangent.xyz;
        let posed = animal_motion::pose(local.position, n, t, local.uv_b,
            mesh_functions::get_tag(vertex_no_morph.instance_index),
            kit_common::kit.animal_rig, kit_common::kit.animal_head);
        local.position = posed.position;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
        local.normal = posed.normal;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
        local.tangent = vec4(posed.tangent, local.tangent.w);
    }
}
    var out = shade_kit_prepass_vertex(local, mesh_world_from_local, wind_origin, vertex_no_morph.instance_index);
    // The LOD cross-fade level, as bevy's prepass.wesl writes it (the fragment dithers on it).
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);
    return out;
}

@if(PREPASS_FRAGMENT)
@fragment
fn fragment(
    in: KitPrepassOutput,
    @builtin(front_facing) is_front: bool,
) -> KitFragmentOutput {
    let flags = pbr_bindings::material.flags;
    let uv_transform = pbr_bindings::material.uv_transform;

@if(VISIBILITY_RANGE_DITHER)
    pbr_functions::visibility_range_dither(in.position, in.visibility_range_dither);

    // `slot_requests` rejects tileable + alpha_cutoff, so tileable slots never discard here.
    kit_prepass_sample_color_and_alpha_discard(in);

    var out: KitFragmentOutput;

@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;

@if(NORMAL_PREPASS) {
    if (kit_common::tileable()) {
        let f = kit_common::tile_frame(in.world_normal, in.world_position.xyz);
        var n = kit_common::tiled_normal(f);
        @if(VERTEX_COLORS) {
            n = kit_common::detail_normal(f, n, in.color.a);
        }
        out.normal = vec4(n * 0.5 + vec3(0.5), 1.0);
    } else if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        let double_sided = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
        // Foliage keeps its front normal on back faces, as in kit_material.wesl.
        let foliage = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS)
            == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK;
        @if(KIT_INSTANCED) {
        // Records never mirror (positive determinant), so the winding's front is the front.
        let is_logically_front = is_front || foliage;
        }
        @else {
        let mesh_flags = mesh[in.instance_index].flags;
        let is_logically_front = pbr_functions::winding_corrected_front_facing(mesh_flags, is_front) || foliage;
        }
        let world_normal = pbr_functions::prepare_world_normal(in.world_normal, double_sided, is_logically_front);
        var normal = world_normal;

@if(VERTEX_UVS) {
@if(VERTEX_TANGENTS) {
@if(STANDARD_MATERIAL_NORMAL_MAP) {
@if(STANDARD_MATERIAL_NORMAL_MAP_UV_B)
        let uv = (uv_transform * vec3(in.uv_b, 1.0)).xy;
@else
        let uv = (uv_transform * vec3(in.uv, 1.0)).xy;
        let Nt = textureSampleBias(pbr_bindings::normal_map_texture, pbr_bindings::normal_map_sampler, uv, view.mip_bias).rgb;
        let TBN = pbr_functions::calculate_tbn_mikktspace(normal, in.world_tangent);
        normal = pbr_functions::apply_normal_mapping(flags, TBN, double_sided, is_logically_front, Nt);
}
}
}

        out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
    } else {
        out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
    }
}

@if(MOTION_VECTOR_PREPASS)
    out.motion_vector = pbr_prepass_functions::calculate_motion_vector(in.world_position, in.previous_world_position);

    return out;
}
@else
@fragment
fn fragment(in: KitPrepassOutput) {
    kit_prepass_sample_color_and_alpha_discard(in);
}
