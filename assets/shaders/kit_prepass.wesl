// Kit prepass (crates/hex-render/src/kit_material.rs, `KitExt::prepass_vertex_shader` and
// `KitExt::prepass_fragment_shader`), which also draws every shadow cascade.
//
// Vertex: Bevy 0.20's bevy_pbr prepass/prepass.wesl vertex plus the foliage wind
// (hex_render::foliage_wind), the same call as kit_vertex.wesl, so depth, normals and shadows move
// with the forward pass; the position is @invariant in both. The previous position for motion
// vectors is the offset at the previous frame's wind clock under the previous transform.
//
// Fragment:
// Tileable slots write the world-space tiled normal from hex_render::kit_common, the same one the forward
// pass lights with, so SSAO reads the normal the surface is shaded with. Every other slot keeps
// Bevy 0.20's StandardMaterial prepass (bevy_pbr render/pbr_prepass.wesl, non-bindless and
// non-meshlet only, which KitExt always is): alpha discard, geometric normal, unique normal map.
import bevy_pbr::{
    render::{
        pbr_prepass_functions,
        pbr_bindings,
        pbr_types,
        pbr_functions,
        mesh_bindings::mesh,
        mesh_view_bindings::view,
    },
    prepass::io as prepass_io,
};
import bevy_pbr::render::{
    mesh_functions,
    skinning,
    morph,
    morph::{morph_position, morph_normal, morph_tangent},
    view_transformations::position_world_to_clip,
};
import bevy_render::globals::Globals;
import hex_render::{kit_common, foliage_wind};

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

@if(MORPH_TARGETS)
fn morph_vertex(vertex_in: prepass_io::UncompressedVertex, instance_index: u32) -> prepass_io::UncompressedVertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let morph_descriptor_index = mesh[instance_index].morph_descriptor_index;
    let weight_count = morph::layer_count(morph_descriptor_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = morph::weight_at(i, morph_descriptor_index);
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

@if(MORPH_TARGETS)
fn morph_prev_vertex(vertex_in: prepass_io::UncompressedVertex, instance_index: u32) -> prepass_io::UncompressedVertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let morph_descriptor_index = mesh[instance_index].morph_descriptor_index;
    let weight_count = morph::layer_count(morph_descriptor_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = morph::prev_weight_at(i, morph_descriptor_index);
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

@vertex
fn vertex(vertex_no_morph: prepass_io::Vertex) -> KitPrepassOutput {
    var out: KitPrepassOutput;
    let uncompressed_vertex_no_morph = prepass_io::decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
@if(MORPH_TARGETS)
    var vertex = morph_vertex(uncompressed_vertex_no_morph, vertex_no_morph.instance_index);
@else
    var vertex = uncompressed_vertex_no_morph;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);

@if(SKINNED)
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex_no_morph.instance_index
    );
@else
    var world_from_local = mesh_world_from_local;

    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_UVS_B)
    out.world_position = vec4(out.world_position.xyz + wind_offset(
        out.world_position.xyz, mesh_world_from_local[3].xyz, vertex.uv_b, prepass_globals.time), 1.0);
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
@if(VERTEX_NORMALS && SKINNED)
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
@elif(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex_no_morph.instance_index
    );

@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex_no_morph.instance_index
    );
}

@if(VERTEX_COLORS)
    out.color = vertex.color;

@if(MOTION_VECTOR_PREPASS) {
@if(MORPH_TARGETS && HAS_PREVIOUS_MORPH)
    let prev_vertex = morph_prev_vertex(uncompressed_vertex_no_morph, vertex_no_morph.instance_index);
@elif(MORPH_TARGETS)
    let prev_vertex = uncompressed_vertex_no_morph;
@else
    let prev_vertex = uncompressed_vertex_no_morph;

@if(SKINNED && HAS_PREVIOUS_SKIN)
    let prev_model = skinning::skin_prev_model(
        prev_vertex.joint_indices,
        prev_vertex.joint_weights,
        vertex_no_morph.instance_index
    );
@elif(SKINNED)
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_no_morph.instance_index);
@else
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_no_morph.instance_index);

    out.previous_world_position = mesh_functions::mesh_position_local_to_world(
        prev_model,
        vec4<f32>(prev_vertex.position, 1.0)
    );
@if(VERTEX_UVS_B) {
    let prev_origin = mesh_functions::get_previous_world_from_local(vertex_no_morph.instance_index)[3].xyz;
    out.previous_world_position = vec4(out.previous_world_position.xyz + wind_offset(
        out.previous_world_position.xyz, prev_origin, vertex.uv_b,
        prepass_globals.time - prepass_globals.delta_time), 1.0);
}
}

@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_no_morph.instance_index;

@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);

    return out;
}

@if(PREPASS_FRAGMENT)
@fragment
fn fragment(
    in: prepass_io::VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> prepass_io::FragmentOutput {
    let flags = pbr_bindings::material.flags;
    let uv_transform = pbr_bindings::material.uv_transform;

@if(VISIBILITY_RANGE_DITHER)
    pbr_functions::visibility_range_dither(in.position, in.visibility_range_dither);

    // `slot_requests` rejects tileable + alpha_cutoff, so tileable slots never discard here.
    pbr_prepass_functions::prepass_sample_color_and_alpha_discard(in);

    var out: prepass_io::FragmentOutput;

@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;

@if(NORMAL_PREPASS)
    if (kit_common::tileable()) {
        let f = kit_common::tile_frame(in.world_normal, in.world_position.xyz);
        out.normal = vec4(kit_common::tiled_normal(f) * 0.5 + vec3(0.5), 1.0);
    } else if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        let double_sided = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
        let mesh_flags = mesh[in.instance_index].flags;
        // Foliage keeps its front normal on back faces, as in kit_material.wesl.
        let foliage = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS)
            == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK;
        let is_logically_front = pbr_functions::winding_corrected_front_facing(mesh_flags, is_front) || foliage;
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

@if(MOTION_VECTOR_PREPASS)
    out.motion_vector = pbr_prepass_functions::calculate_motion_vector(in.world_position, in.previous_world_position);

    return out;
}
@else
@fragment
fn fragment(in: prepass_io::VertexOutput) {
    pbr_prepass_functions::prepass_sample_color_and_alpha_discard(in);
}
