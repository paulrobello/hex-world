import bevy_pbr::render::{
    mesh_bindings::mesh,
    view_transformations::position_world_to_clip
};

// The boid buffer the compute pass (boids_compute.wgsl) writes; vel.w is what the boid is doing.
struct Boid { pos: vec4<f32>, vel: vec4<f32> };
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<storage, read> boids: array<Boid>;

const SETTLE: f32 = 1.0;
const PERCH: f32 = 2.0;
const SWIM: f32 = 3.0;

// Procedural placeholder forms until bird and fish kit pieces exist: 8 vertices each, local frame
// +z forward, +x right, +y up (metres). Triangles (boids.rs BOID_TRIS): wings or flanks (0 1 3,
// 0 3 2), the tail (3 4 5), the upper and lower keel (0 6 3, 0 3 7).
const BIRD = array<vec3<f32>, 8>(
    vec3<f32>(0.0, 0.03, 0.55),
    vec3<f32>(-1.15, 0.0, -0.05),
    vec3<f32>(1.15, 0.0, -0.05),
    vec3<f32>(0.0, 0.0, -0.32),
    vec3<f32>(-0.24, 0.0, -0.66),
    vec3<f32>(0.24, 0.0, -0.66),
    vec3<f32>(0.0, 0.16, 0.06),
    vec3<f32>(0.0, -0.14, 0.06),
);
const FISH = array<vec3<f32>, 8>(
    vec3<f32>(0.0, 0.0, 0.44),
    vec3<f32>(-0.13, 0.0, 0.06),
    vec3<f32>(0.13, 0.0, 0.06),
    vec3<f32>(0.0, 0.0, -0.24),
    vec3<f32>(-0.17, 0.0, -0.46),
    vec3<f32>(0.17, 0.0, -0.46),
    vec3<f32>(0.0, 0.14, 0.0),
    vec3<f32>(0.0, -0.11, 0.03),
);

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
};
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec3<f32> };

fn hash(n: u32) -> f32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    x = (x >> 22u) ^ x;
    return f32(x) / 4294967295.0;
}

@vertex
fn vertex(v: Vertex) -> VOut {
    // vertex_index includes the mesh's offset in Bevy's shared vertex slab.
    let local = v.vertex_index - mesh[v.instance_index].first_vertex_index;
    let id = local / 8u;
    let corner = local % 8u;
    let b = boids[id];
    let mode = b.vel.w;
    let fish = mode > SWIM - 0.5;
    // Animation phase from the distance travelled, so wings and tails beat with the motion.
    let phase = dot(b.pos.xyz, vec3<f32>(0.9, 1.3, 0.7)) * 0.6 + hash(id) * 6.2831853;
    var s: vec3<f32>;
    var color: vec3<f32>;
    if (fish) {
        var forms = FISH;
        s = forms[corner];
        // Tail wag.
        let wag = sin(phase * 2.5);
        if (corner == 3u) { s.x += wag * 0.04; }
        if (corner == 4u || corner == 5u) { s.x += wag * 0.12; }
        // Koi orange, gold and white (linear), bright enough to read through the absorbing water.
        let k = hash(id * 7u + 1u);
        var body = vec3<f32>(1.0, 0.32, 0.03);
        if (k > 0.66) { body = vec3<f32>(0.92, 0.9, 0.84); } else if (k > 0.33) { body = vec3<f32>(1.0, 0.62, 0.06); }
        color = body;
        if (corner == 7u) { color = mix(body, vec3<f32>(1.0), 0.6); }
        if (corner >= 4u && corner <= 5u) { color = body * 0.6; }
        s *= 1.2;
    } else {
        var forms = BIRD;
        s = forms[corner];
        if (mode > PERCH - 0.5) {
            // Perched: wings folded along the body.
            if (corner == 1u || corner == 2u) { s = vec3<f32>(sign(s.x) * 0.2, 0.1, -0.3); }
        } else {
            // Flapping (faster while settling in).
            let rate = select(1.0, 1.8, mode > SETTLE - 0.5);
            let f = sin(phase * rate);
            if (corner == 1u || corner == 2u) { s = vec3<f32>(s.x * (0.85 + 0.15 * abs(f)), f * 0.55, s.z); }
        }
        // A gull (linear): white body, wings shading to dark tips, grey back. Reads over grass and
        // over the dark void past the world's edge.
        color = vec3<f32>(0.62, 0.62, 0.6);
        if (corner == 1u || corner == 2u) { color = vec3<f32>(0.012, 0.012, 0.014); }
        if (corner == 6u) { color = vec3<f32>(0.16, 0.17, 0.19); }
        s *= 1.25;
    }
    var fwd = vec3<f32>(0.0, 0.0, 1.0);
    if (length(b.vel.xyz) > 1e-6) { fwd = normalize(b.vel.xyz); }
    // Keep pitch gentle so a climbing bird does not stand on its tail.
    fwd = normalize(vec3<f32>(fwd.x, clamp(fwd.y, -0.5, 0.5), fwd.z) + vec3<f32>(1e-5, 0.0, 0.0));
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), fwd));
    let up = cross(fwd, right);
    let world = b.pos.xyz + right * s.x + up * s.y + fwd * s.z;
    var o: VOut;
    o.clip = position_world_to_clip(world);
    o.color = color;
    return o;
}

@fragment
fn fragment(i: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(i.color, 1.0);
}
