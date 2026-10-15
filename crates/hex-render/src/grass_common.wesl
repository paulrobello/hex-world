// `hex_render::grass_common`: the grass blade pose shared by assets/shaders/grass.wesl (forward) and
// grass_prepass.wesl (depth, normal and motion prepass), so both passes place every vertex with the
// same expressions (crates/hex-render/src/play/grass.rs). Embedded like kit_common.
//
// A blade's vertices all carry its root (POSITION, tile-local), so the wind is sampled once per
// blade at the root and the root never moves: the bend grows with the square of the height along
// the blade. UV_0 is (half-width offset across the blade in m, height fraction along it), UV_1 is
// (yaw in whole degrees + thinning rank in [0, 0.9), blade height in m).
import hex_render::wind;

struct GrassParams {
    time: f32,
    prev_time: f32,
    fade_start: f32,
    fade_end: f32,
    thin_start: f32,
    thin_end: f32,
    thin_keep: f32,
    wind_scale: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> grass: GrassParams;

struct Pose {
    world: vec3<f32>,
    // Unit normal of the bent blade, turned toward the camera.
    normal: vec3<f32>,
    gust: f32,
}

fn pose(world_from_local: mat4x4<f32>, root: vec3<f32>, blade: vec2<f32>, shape: vec2<f32>, camera: vec3<f32>, t: f32) -> Pose {
    let root_w = (world_from_local * vec4(root, 1.0)).xyz;
    let yaw = radians(floor(shape.x));
    let rank = fract(shape.x);
    let right = normalize((world_from_local * vec4(cos(yaw), 0.0, sin(yaw), 0.0)).xyz);
    let face = vec2(-right.z, right.x);

    // Far blades thin out by rank (the kept ones widen to hold the cover), then all shrink to
    // nothing toward fade_end, so no band pops.
    let d = distance(camera, root_w);
    let keep = mix(1.0, grass.thin_keep, smoothstep(grass.thin_start, grass.thin_end, d));
    let s = smoothstep(rank, rank + 0.1, keep) * (1.0 - smoothstep(grass.fade_start, grass.fade_end, d));
    let tall = 0.7 + 0.6 * wind::value_noise(root_w.xz / 5.0, 0x91e1u);
    let h = shape.y * tall * s;
    let wide = 1.0 + 1.5 * (1.0 - keep);

    let w = wind::sample(root_w.xz, t);
    let stiff = 0.7 + 0.6 * fract(rank * 37.0);
    var tip = face * (fract(rank * 113.0) - 0.5) * 0.5 + w.push * grass.wind_scale * stiff;
    let l = length(tip);
    if (l > 1.1) {
        tip *= 1.1 / l;
    }
    let a = blade.y;
    let k = dot(tip, tip);
    let off = tip * (h * a * a);
    let world = root_w + right * (blade.x * s * wide) + vec3(off.x, h * a * (1.0 - 0.3 * k * a * a), off.y);

    // One side per blade (from its root), so a triangle's normals never interpolate through zero.
    let side = select(-1.0, 1.0, dot(face, camera.xz - root_w.xz) >= 0.0);
    let along = normalize(vec3(2.0 * a * tip.x, 1.0 - 0.9 * k * a * a, 2.0 * a * tip.y));
    return Pose(world, side * normalize(cross(right, along)), w.gust);
}
