// `hex_render::grass_common`: the grass blade bindings, instance unpacking and pose shared by
// assets/shaders/grass.wesl (forward) and grass_prepass.wesl (depth, normal and motion prepass), so
// both passes place every vertex with the same expressions (crates/hex-render/src/play/grass.rs,
// instanced by play/grass_gpu.rs). Embedded like kit_common.
//
// Instanced: the shared blade mesh's vertex is a corner (offset across in widths, height fraction
// along, plane 0 or 1) and the instance is the blade (root, tile-local; yaw in whole degrees +
// thinning rank in [0, 0.9); height and width in m; colour). `blade` turns them into the pose's
// inputs: every corner of a blade carries its root, so the wind is sampled once per blade at the
// root and the root never moves: the bend grows with the square of the height along the blade.
// `blade` is (half-width offset across the blade in m, height fraction along it), `shape` is (yaw +
// rank, blade height in m). A blade is two crossed planes: the second carries its height negated
// and spans the blade's facing instead of its right axis. Both planes share the root, lean, tilt
// and bend, so their tips meet.
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

// One tile's group: its transform now and one frame ago, and its season tag (0 is summer).
struct GrassTile {
    world_from_local: mat4x4<f32>,
    previous_world_from_local: mat4x4<f32>,
    tag: u32,
}

@group(2) @binding(0) var<uniform> grass: GrassParams;
@group(2) @binding(1) var<uniform> tile: GrassTile;

struct Blade {
    blade: vec2<f32>,
    shape: vec2<f32>,
}

fn blade(corner: vec3<f32>, yaw_rank: f32, size: vec2<f32>) -> Blade {
    return Blade(vec2(corner.x * size.y, corner.y), vec2(yaw_rank, select(size.x, -size.x, corner.z > 0.5)));
}

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
    let crossed = shape.y < 0.0;
    let right = normalize((world_from_local * vec4(cos(yaw), 0.0, sin(yaw), 0.0)).xyz);
    let face = vec2(-right.z, right.x);
    // The plane's span and its unbent normal: the first plane spans `right` and faces `face`, the
    // crossed one spans `face` and faces -`right`.
    let span = select(right, vec3(face.x, 0.0, face.y), crossed);
    let flat_n = select(face, -right.xz, crossed);

    // Far blades thin out by rank (the kept ones widen to hold the cover), then all shrink to
    // nothing toward fade_end, so no band pops.
    let d = distance(camera, root_w);
    let keep = mix(1.0, grass.thin_keep, smoothstep(grass.thin_start, grass.thin_end, d));
    let s = smoothstep(rank, rank + 0.1, keep) * (1.0 - smoothstep(grass.fade_start, grass.fade_end, d));
    // Clumps: a low-frequency field (about 0.35 per m, world space so it runs across tile borders)
    // stands lush patches taller and thin ones stubbier.
    let tall = 0.75 + 0.7 * wind::value_noise(root_w.xz * 0.35, 0x91e1u);
    let h = abs(shape.y) * tall * s;
    let wide = 1.0 + 1.5 * (1.0 - keep);

    let w = wind::sample(root_w.xz, t);
    let stiff = 0.7 + 0.6 * fract(rank * 37.0);
    // A random lean along the facing and a slighter tilt along the right axis, then the wind.
    let lean = face * (fract(rank * 113.0) - 0.5) * 0.5 + right.xz * (fract(rank * 71.0) - 0.5) * 0.3;
    var tip = lean + w.push * grass.wind_scale * stiff;
    let l = length(tip);
    if (l > 1.1) {
        tip *= 1.1 / l;
    }
    let a = blade.y;
    let k = dot(tip, tip);
    let off = tip * (h * a * a);
    let world = root_w + span * (blade.x * s * wide) + vec3(off.x, h * a * (1.0 - 0.3 * k * a * a), off.y);

    // One side per plane (from its root), so a triangle's normals never interpolate through zero.
    let side = select(-1.0, 1.0, dot(flat_n, camera.xz - root_w.xz) >= 0.0);
    let along = normalize(vec3(2.0 * a * tip.x, 1.0 - 0.9 * k * a * a, 2.0 * a * tip.y));
    return Pose(world, side * normalize(cross(span, along)), w.gust);
}
