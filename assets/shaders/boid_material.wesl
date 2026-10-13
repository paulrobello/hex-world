import bevy_pbr::render::{
    mesh_bindings::mesh,
    view_transformations::position_world_to_clip
};

struct Boid { pos: vec4<f32>, vel: vec4<f32> };
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<storage, read> boids: array<Boid>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
};
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) shade: f32 };

@vertex
fn vertex(v: Vertex) -> VOut {
    // vertex_index includes the mesh's offset in Bevy's shared vertex slab.
    let local = v.vertex_index - mesh[v.instance_index].first_vertex_index;
    let b = boids[local / 4u];
    let fwd = normalize(vec3<f32>(b.vel.x, 0.0, b.vel.z) + vec3<f32>(0.0001, 0.0, 0.0));
    let right = vec3<f32>(fwd.z, 0.0, -fwd.x);
    let world = b.pos.xyz + right * v.position.x + vec3<f32>(0.0, v.position.y, 0.0) + fwd * v.position.z;
    var o: VOut;
    o.clip = position_world_to_clip(world);
    o.shade = 0.6 + v.position.y;
    return o;
}

@fragment
fn fragment(i: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(vec3<f32>(0.12, 0.12, 0.14) * i.shade, 1.0);
}
