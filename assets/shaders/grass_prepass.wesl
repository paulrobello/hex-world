// Grass blades, prepass (crates/hex-render/src/play/grass.rs, instanced by play/grass_gpu.rs):
// depth, the normal for SSAO and the motion vector for TAA. The vertex stage takes the same corner
// and instance record as grass.wesl and poses them with grass_common::pose, with the position
// @invariant so the forward pass meets this depth exactly; the fragment writes what Bevy's default
// prepass fragment writes. The previous position is the pose at the previous frame's wind clock
// under the group's previous transform.
import bevy_pbr::render::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
};
import hex_render::grass_common as gc;

@if(MOTION_VECTOR_PREPASS)
import bevy_pbr::prepass::bindings::previous_view_uniforms;

struct Vertex {
    @location(0) corner: vec3<f32>,
    @location(1) root: vec3<f32>,
    @location(2) yaw_rank: f32,
    @location(3) size: vec2<f32>,
}

struct PrepassOut {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(2) previous_world_position: vec4<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @location(3) unclipped_depth: f32,
}

@vertex
fn vertex(v: Vertex) -> PrepassOut {
    var out: PrepassOut;
    let b = gc::blade(v.corner, v.yaw_rank, v.size);
    let p = gc::pose(gc::tile.world_from_local, v.root, b.blade, b.shape, view.world_position, gc::grass.time);
    out.position = position_world_to_clip(p.world);
    out.world_position = vec4(p.world, 1.0);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
    out.world_normal = normalize(mix(vec3(0.0, 1.0, 0.0), p.normal, 0.35));
@if(MOTION_VECTOR_PREPASS) {
    let q = gc::pose(gc::tile.previous_world_from_local, v.root, b.blade, b.shape, view.world_position, gc::grass.prev_time);
    out.previous_world_position = vec4(q.world, 1.0);
}
    return out;
}

@if(PREPASS_FRAGMENT)
struct PrepassFragment {
    @if(NORMAL_PREPASS)
    @location(0) normal: vec4<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(1) motion_vector: vec2<f32>,
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    @builtin(frag_depth) frag_depth: f32,
}

@if(PREPASS_FRAGMENT)
@fragment
fn fragment(in: PrepassOut) -> PrepassFragment {
    var out: PrepassFragment;
@if(NORMAL_PREPASS)
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION)
    out.frag_depth = in.unclipped_depth;
@if(MOTION_VECTOR_PREPASS) {
    let clip_position_t = view.unjittered_clip_from_world * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = previous_view_uniforms.unjittered_clip_from_world * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;
    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
}
    return out;
}
