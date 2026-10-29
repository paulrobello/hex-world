// Kit pieces, forward vertex stage (crates/hex-render/src/kit_material.rs, `KitExt::vertex_shader`).
// Bevy 0.20's bevy_pbr render/mesh.wesl vertex, plus the foliage wind (hex_render::foliage_wind) on
// slots with a wind flag whose mesh carries TEXCOORD_1; every other piece is placed exactly as
// before. kit_prepass.wesl displaces with the same call for depth, normals, motion and shadows, and
// both outputs are @invariant so the forward depth meets the prepass depth. The wind clock is
// `globals.time`, the `Time` the grass reads, which `--clock fixed` steps by 1/60 s per frame.
// Pinned against upstream by crates/hex-render/tests/bevy_prepass_tripwire.rs.
//
// `KIT_INSTANCED` swaps the entry point: settled parts draw in one call per (slot, LOD) through
// hex_render::kit_instances' record buffer, so the per-mesh uniform (`mesh`, and the forward_io
// module that imports it) must stay out of the composed shader — wesl compiles imported modules
// whole, and any `forward_io` import drags `mesh_bindings::mesh` into group 2. The instanced
// entry decompresses against the per-slot metadata uniform instead. Both branches produce the
// same `KitVertexOutput`; the shading tail is shared in `shade_kit_vertex`.
import bevy_pbr::render::{
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal, morph_tangent},
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
};
@if(!KIT_INSTANCED)
import bevy_pbr::render::forward_io::{Vertex, UncompressedVertex, decompress_vertex};
@if(!KIT_INSTANCED)
import bevy_pbr::render::mesh_bindings::mesh;
import hex_render::{kit_common, foliage_wind, animal_motion};
import bevy_render::utils;
@if(KIT_INSTANCED)
import hex_render::kit_instances;

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

// The decompressed vertex fields the shading tail needs. Under KIT_INSTANCED this stands in for
// forward_io::UncompressedVertex (same def gates, local so forward_io stays out of the module);
// the batched entry copies its decompressed vertex into it.
struct KitUncompressed {
@if(VERTEX_POSITIONS)
    position: vec3<f32>,
@if(VERTEX_NORMALS)
    normal: vec3<f32>,
@if(VERTEX_UVS_A)
    uv: vec2<f32>,
@if(VERTEX_UVS_B)
    uv_b: vec2<f32>,
@if(VERTEX_TANGENTS)
    tangent: vec4<f32>,
@if(VERTEX_COLORS)
    color: vec4<f32>,
}

// forward_io::Vertex's layout, mirrored locally for the instanced entry. `kit_record` is
// the instanced u32 (one per instance, sliced per draw) selecting the instance record —
// see kit_instances.wesl for why it is not `instance_index`.
struct KitVertexIn {
@if(KIT_INSTANCED)
    @location(6) kit_record: u32,
@if(!KIT_INSTANCED)
    @builtin(instance_index) instance_index: u32,
@if(VERTEX_POSITIONS && VERTEX_POSITIONS_COMPRESSED)
    @location(0) compressed_position: vec4<f32>,
@if(VERTEX_POSITIONS && !VERTEX_POSITIONS_COMPRESSED)
    @location(0) position: vec3<f32>,
@if(VERTEX_NORMALS && VERTEX_NORMALS_COMPRESSED)
    @location(1) compressed_normal: vec2<f32>,
@if(VERTEX_NORMALS && !VERTEX_NORMALS_COMPRESSED)
    @location(1) normal: vec3<f32>,
@if(VERTEX_UVS_A && VERTEX_UVS_A_COMPRESSED)
    @location(2) compressed_uv: vec2<f32>,
@if(VERTEX_UVS_A && !VERTEX_UVS_A_COMPRESSED)
    @location(2) uv: vec2<f32>,
@if(VERTEX_UVS_B && VERTEX_UVS_B_COMPRESSED)
    @location(3) compressed_uv_b: vec2<f32>,
@if(VERTEX_UVS_B && !VERTEX_UVS_B_COMPRESSED)
    @location(3) uv_b: vec2<f32>,
@if(VERTEX_TANGENTS && VERTEX_TANGENTS_COMPRESSED)
    @location(4) compressed_tangent: vec2<f32>,
@if(VERTEX_TANGENTS && !VERTEX_TANGENTS_COMPRESSED)
    @location(4) tangent: vec4<f32>,
@if(VERTEX_COLORS)
    @location(5) color: vec4<f32>,
}

@if(MORPH_TARGETS && !KIT_INSTANCED)
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

// The shading tail both entries share: normals, wind displacement, clip, varyings. Local
// normal/tangent transforms (inverse-transpose) because bevy's mesh_functions helpers read the
// per-mesh uniform, which the instanced path does not bind. `field_in` is the part's packed wind
// field when `has_field` (the instanced path, hex_render::kit_instances::kit_field); tree slots
// sway in it instead of sampling the field per vertex. kit_prepass.wesl places a vertex with the
// same calls, so both depths agree.
fn shade_kit_vertex(
    vertex: KitUncompressed,
    world_from_local: mat4x4<f32>,
    wind_origin_in: vec3<f32>,
    tag: u32,
    field_in: vec4<f32>,
    has_field: bool,
) -> KitVertexOutput {
    var out: KitVertexOutput;
    var wind_origin = wind_origin_in;
@if(VERTEX_COLORS) {
    // COLOR_0.a below zero marks the wind origin (the foliage sway pivots there); the colour must
    // then read as neutral white. WGSL parameters are immutable, so work on a copy.
    var color = vertex.color;
    if (color.a < 0.0) {
        wind_origin = color.xyz;
        color = vec4(1.0);
    }
    out.color = color;
}

    let basis = mat3x3(world_from_local[0].xyz, world_from_local[1].xyz, world_from_local[2].xyz);
    let normal_basis = kit_common::kit_normal_basis(basis);

@if(VERTEX_NORMALS)
    out.world_normal = normalize(normal_basis * vertex.normal);

@if(VERTEX_POSITIONS) {
    out.world_position = world_from_local * vec4<f32>(vertex.position, 1.0);
@if(VERTEX_UVS_B) {
    let wind = kit_common::kit.flags & (kit_common::FLAG_WIND_TREE | kit_common::FLAG_WIND_PLANT);
    if (wind != 0u) {
        let rest = out.world_position.xyz;
        let tree = wind == kit_common::FLAG_WIND_TREE;
        let t = globals.time;
        var f = foliage_wind::unpack(field_in);
        if (!(tree && has_field)) {
            f = foliage_wind::field(rest, wind_origin, tree, t);
        }
        out.world_position = vec4(rest + foliage_wind::offset_at(
            f, rest, wind_origin, vertex.uv_b, kit_common::kit.wind_amp, tree, t), 1.0);
    }
}
    out.position = position_world_to_clip(out.world_position.xyz);
}

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;

@if(VERTEX_TANGENTS)
    out.world_tangent = vec4(normalize(basis * vertex.tangent.xyz), vertex.tangent.w);

@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = tag;

    return out;
}

@if(KIT_INSTANCED)
@vertex
fn vertex(vertex_no_morph: KitVertexIn) -> KitVertexOutput {
    // Decompress from kit_instances' per-slot metadata (mirrors forward_io::decompress_vertex).
    var vertex: KitUncompressed;
@if(VERTEX_POSITIONS && VERTEX_POSITIONS_COMPRESSED)
    vertex.position = bevy_render::utils::decompress_vertex_position(vertex_no_morph.compressed_position, kit_instances::kit_meta.center, kit_instances::kit_meta.half);
@if(VERTEX_POSITIONS && !VERTEX_POSITIONS_COMPRESSED)
    vertex.position = vertex_no_morph.position;
@if(VERTEX_NORMALS && VERTEX_NORMALS_COMPRESSED)
    vertex.normal = bevy_render::utils::decompress_vertex_normal(vertex_no_morph.compressed_normal);
@if(VERTEX_NORMALS && !VERTEX_NORMALS_COMPRESSED)
    vertex.normal = vertex_no_morph.normal;
@if(VERTEX_UVS_A && VERTEX_UVS_A_COMPRESSED)
    vertex.uv = bevy_render::utils::decompress_vertex_uv(vertex_no_morph.compressed_uv, kit_instances::kit_meta.uv_a);
@if(VERTEX_UVS_A && !VERTEX_UVS_A_COMPRESSED)
    vertex.uv = vertex_no_morph.uv;
@if(VERTEX_UVS_B && VERTEX_UVS_B_COMPRESSED)
    vertex.uv_b = bevy_render::utils::decompress_vertex_uv(vertex_no_morph.compressed_uv_b, kit_instances::kit_meta.uv_b);
@if(VERTEX_UVS_B && !VERTEX_UVS_B_COMPRESSED)
    vertex.uv_b = vertex_no_morph.uv_b;
@if(VERTEX_TANGENTS && VERTEX_TANGENTS_COMPRESSED)
    vertex.tangent = bevy_render::utils::decompress_vertex_tangent(vertex_no_morph.compressed_tangent);
@if(VERTEX_TANGENTS && !VERTEX_TANGENTS_COMPRESSED)
    vertex.tangent = vertex_no_morph.tangent;
@if(VERTEX_COLORS)
    vertex.color = vertex_no_morph.color;
    let inst = kit_instances::kit_instance(vertex_no_morph.kit_record);
    var out = shade_kit_vertex(vertex, kit_instances::kit_world_from_local(inst), inst.translation, inst.tag,
        kit_instances::kit_field(vertex_no_morph.kit_record), true);
    // A band draw (crates/hex-render/src/kit_instances.rs, `KitSlotDraw::band`): the part's level in
    // its LOD's cross-fade band, as the per-entity entry below writes it from the VisibilityRange.
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = kit_instances::kit_lod_dither_level(inst.translation, view.lod_view_world_position);
    return out;
}

@if(!KIT_INSTANCED)
@vertex
fn vertex(vertex_no_morph: Vertex) -> KitVertexOutput {
    let uncompressed_vertex_no_morph = decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
@if(MORPH_TARGETS)
    var vertex = morph_vertex(uncompressed_vertex_no_morph, vertex_no_morph.instance_index);
@else
    var vertex = uncompressed_vertex_no_morph;

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
    var wind_origin = mesh_world_from_local[3].xyz;

    var local: KitUncompressed;
@if(VERTEX_POSITIONS)
    local.position = vertex.position;
@if(VERTEX_NORMALS)
    local.normal = vertex.normal;
@if(VERTEX_UVS_A)
    local.uv = vertex.uv;
@if(VERTEX_UVS_B)
    local.uv_b = vertex.uv_b;
@if(VERTEX_TANGENTS)
    local.tangent = vertex.tangent;
@if(VERTEX_COLORS)
    local.color = vertex.color;
    // A farm animal's walk and graze pose (hex_render::animal_motion) by its MeshTag motion word,
    // in the piece frame. Only the per-entity path: the herd draws here, and settled (instanced)
    // parts keep their rest pose.
@if(VERTEX_POSITIONS && VERTEX_UVS_B) {
    if ((kit_common::kit.flags & kit_common::FLAG_ANIMAL) != 0u) {
        var n = vec3(0.0, 1.0, 0.0);
@if(VERTEX_NORMALS)
        n = local.normal;
        var t = vec3(1.0, 0.0, 0.0);
@if(VERTEX_TANGENTS)
        t = local.tangent.xyz;
        let posed = animal_motion::pose(local.position, n, t, local.uv_b,
            mesh_functions::get_tag(vertex_no_morph.instance_index),
            kit_common::kit.animal_rig, kit_common::kit.animal_head);
        local.position = posed.position;
@if(VERTEX_NORMALS)
        local.normal = posed.normal;
@if(VERTEX_TANGENTS)
        local.tangent = vec4(posed.tangent, local.tangent.w);
    }
}
    var out = shade_kit_vertex(local, mesh_world_from_local, wind_origin, vertex_no_morph.instance_index,
        vec4(0.0), false);
    // The LOD cross-fade level of this entity's VisibilityRange band, as bevy's mesh.wesl writes it;
    // kit_material.wesl dithers on it like the prepass, so two LODs split the band pixel for pixel.
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);
    return out;
}
