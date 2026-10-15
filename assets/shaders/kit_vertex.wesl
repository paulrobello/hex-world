// Kit pieces, forward vertex stage (crates/hex-render/src/kit_material.rs, `KitExt::vertex_shader`).
// Bevy 0.20's bevy_pbr render/mesh.wesl vertex, plus the foliage wind (hex_render::foliage_wind) on
// slots with a wind flag whose mesh carries TEXCOORD_1; every other piece is placed exactly as
// before. kit_prepass.wesl displaces with the same call for depth, normals, motion and shadows, and
// both outputs are @invariant so the forward depth meets the prepass depth. The wind clock is
// `globals.time`, the `Time` the grass reads, which `--clock fixed` steps by 1/60 s per frame.
// Pinned against upstream by crates/hex-render/tests/bevy_prepass_tripwire.rs.
import bevy_pbr::render::{
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal, morph_tangent},
    forward_io::{Vertex, UncompressedVertex, decompress_vertex},
    mesh_bindings::mesh,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
};
import hex_render::{kit_common, foliage_wind};

// forward_io::VertexOutput's locations, with the position @invariant.
struct KitVertexOutput {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @if(VERTEX_UVS_A)
    @location(2) uv: vec2<f32>,
    @if(VERTEX_UVS_B)
    @location(3) uv_b: vec2<f32>,
    @if(VERTEX_TANGENTS)
    @location(4) world_tangent: vec4<f32>,
    @if(VERTEX_COLORS)
    @location(5) color: vec4<f32>,
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    @location(6) @interpolate(flat) instance_index: u32,
    @if(VISIBILITY_RANGE_DITHER)
    @location(7) @interpolate(flat) visibility_range_dither: i32,
}

@if(MORPH_TARGETS)
fn morph_vertex(vertex_in: UncompressedVertex, instance_index: u32) -> UncompressedVertex {
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

@vertex
fn vertex(vertex_no_morph: Vertex) -> KitVertexOutput {
    var out: KitVertexOutput;
    let uncompressed_vertex_no_morph = decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
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

@if(VERTEX_NORMALS && SKINNED)
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
@if(VERTEX_NORMALS && !SKINNED)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex_no_morph.instance_index
    );

@if(VERTEX_POSITIONS) {
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_UVS_B) {
    let wind = kit_common::kit.flags & (kit_common::FLAG_WIND_TREE | kit_common::FLAG_WIND_PLANT);
    if (wind != 0u) {
        out.world_position = vec4(out.world_position.xyz + foliage_wind::offset(
            out.world_position.xyz, mesh_world_from_local[3].xyz, vertex.uv_b, kit_common::kit.wind_amp,
            wind == kit_common::FLAG_WIND_TREE, globals.time), 1.0);
    }
}
    out.position = position_world_to_clip(out.world_position.xyz);
}

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;

@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex_no_morph.instance_index
    );

@if(VERTEX_COLORS)
    out.color = vertex.color;

@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_no_morph.instance_index;

@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);

    return out;
}
