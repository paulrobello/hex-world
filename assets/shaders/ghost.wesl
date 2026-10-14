// The ghost's see-through meshes (crates/hex-render/src/play/ghost.rs). One uniform colour and no
// texture, so the pipeline samples only the view's textures (the kit forward pipeline is at
// WebGPU's 16 beside refracting water). A fixed key light shades the faces so the shape reads.
import bevy_pbr::render::forward_io::VertexOutput;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> ghost_color: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let key = normalize(vec3(0.35, 0.85, 0.4));
    let shade = 0.6 + 0.4 * max(dot(n, key), 0.0);
    return vec4(ghost_color.rgb * shade, ghost_color.a);
}
