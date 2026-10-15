// Grass blades, prepass vertex stage (crates/hex-render/src/play/grass.rs): depth, the normal for
// SSAO and the motion vector for TAA, through Bevy's default prepass fragment. The output mirrors
// bevy_pbr::prepass::io::VertexOutput's locations under the same defs, with the position
// @invariant so the forward pass (grass.wesl) meets this depth exactly. The previous position is
// the pose at the previous frame's wind clock under the previous transform.
import bevy_pbr::render::{
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
};
import hex_render::grass_common as gc;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) blade: vec2<f32>,
    @location(2) shape: vec2<f32>,
}

struct PrepassOut {
    @invariant @builtin(position) position: vec4<f32>,
    @if(VERTEX_UVS_A)
    @location(0) uv: vec2<f32>,
    @if(VERTEX_UVS_B)
    @location(1) uv_b: vec2<f32>,
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    @location(2) world_normal: vec3<f32>,
    @location(4) world_position: vec4<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(5) previous_world_position: vec4<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @location(6) unclipped_depth: f32,
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    @location(7) @interpolate(flat) instance_index: u32,
}

@vertex
fn vertex(v: Vertex) -> PrepassOut {
    var out: PrepassOut;
    let model = mesh_functions::get_world_from_local(v.instance_index);
    let p = gc::pose(model, v.position, v.blade, v.shape, view.world_position, gc::grass.time);
    out.position = position_world_to_clip(p.world);
    out.world_position = vec4(p.world, 1.0);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
@if(VERTEX_UVS_A)
    out.uv = v.blade;
@if(VERTEX_UVS_B)
    out.uv_b = v.shape;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = normalize(mix(vec3(0.0, 1.0, 0.0), p.normal, 0.35));
@if(MOTION_VECTOR_PREPASS) {
    let prev = mesh_functions::get_previous_world_from_local(v.instance_index);
    let q = gc::pose(prev, v.position, v.blade, v.shape, view.world_position, gc::grass.prev_time);
    out.previous_world_position = vec4(q.world, 1.0);
}
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = v.instance_index;
    return out;
}
