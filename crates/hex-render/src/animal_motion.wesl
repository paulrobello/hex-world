// `hex_render::animal_motion`: the farm animals' walk and graze pose (crates/hex-render/src/animal.rs,
// which mirrors it on the CPU and documents the contract), shared by assets/shaders/kit_vertex.wesl
// (forward) and kit_prepass.wesl (depth, normals, motion and every shadow cascade), so each pass
// moves a vertex with the same expressions. Embedded like foliage_wind; declares no bindings and
// samples nothing.
//
// Inputs per vertex, in the piece frame (+z forward, +y up): position, normal, tangent direction and
// TEXCOORD_1 `motion` (U leg weight, V leg code on a leg or head weight off it). Per slot: `rig` =
// (hipY, front hip z, back hip z, swing rad) and `head` = (neck pivot y, neck pivot z, head rad at
// weight 1 nose down positive, bob rad). Per instance: the motion word (MeshTag; 0 is rest and
// returns the vertex untouched).

const TAU: f32 = 6.2831853;
const PI: f32 = 3.14159265;
// Fraction of the hip height a swinging hoof lifts at walk weight 1 (animal.rs `LIFT`).
const LIFT: f32 = 0.12;

struct AnimalPose {
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
}

// (y, z) of `p` turned by `a` about `c` in the (z, y) plane: +a takes a point below `c` forward.
fn turn(d: vec2<f32>, a: f32) -> vec2<f32> {
    let s = sin(a);
    let k = cos(a);
    return vec2(d.y * s + d.x * k, d.y * k - d.x * s);
}

// Gait phase offset of a leg code: the diagonal pairs (front-left and back-right at 0).
fn leg_offset(code: f32) -> f32 {
    let k = u32(code * 4.0);
    if (k == 1u || k == 2u) {
        return 0.5;
    }
    return 0.0;
}

// Swing in [-1, 1] (+1 hoof forward) and lift in [0, 1] at leg phase p: stance (p < 0.5) at an
// even rate on the ground, then an eased swing back with the hoof lifted.
fn gait(p: f32) -> vec2<f32> {
    if (p < 0.5) {
        return vec2(1.0 - 4.0 * p, 0.0);
    }
    let q = 2.0 * (p - 0.5);
    return vec2(-1.0 + 2.0 * q * q * (3.0 - 2.0 * q), sin(PI * q));
}

fn pose(
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
    motion: vec2<f32>,
    word: u32,
    rig: vec4<f32>,
    head: vec4<f32>,
) -> AnimalPose {
    var out: AnimalPose;
    out.position = position;
    out.normal = normal;
    out.tangent = tangent;
    if (word == 0u) {
        return out;
    }
    let phase = f32(word & 0xffffu) / 65536.0;
    let walk = f32((word >> 16u) & 0xffu) / 255.0;
    let head_w = f32(word >> 24u) / 255.0;
    var c = vec2(head.x, head.y);
    var a = 0.0;
    var drop = 0.0;
    if (motion.x > 0.0) {
        c = vec2(rig.x, select(rig.z, rig.y, motion.y < 0.5));
        let g = gait(fract(phase + leg_offset(motion.y)));
        a = rig.w * walk * g.x;
        drop = rig.x * ((1.0 - cos(a)) - LIFT * walk * g.y) * motion.x;
    } else if (motion.y > 0.0) {
        let bob = head.w * walk * sin(TAU * 2.0 * phase);
        a = -(head_w * head.z + bob) * motion.y;
    } else {
        return out;
    }
    let q = c + turn(vec2(position.y, position.z) - c, a);
    out.position = vec3(position.x, q.x - drop, q.y);
    let n = turn(vec2(normal.y, normal.z), a);
    out.normal = vec3(normal.x, n.x, n.y);
    let t = turn(vec2(tangent.y, tangent.z), a);
    out.tangent = vec3(tangent.x, t.x, t.y);
    return out;
}
