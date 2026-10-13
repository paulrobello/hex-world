struct Boid { pos: vec4<f32>, vel: vec4<f32> };
@group(0) @binding(0) var<storage, read_write> boids: array<Boid>;
struct Params { count: u32, dt: f32, _pad0: u32, _pad1: u32 };
@group(0) @binding(1) var<uniform> params: Params;

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params.count) { return; }
    var me = boids[i];
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    // O(n^2) is fine for the P0 count (1,000); spatial hashing arrives with P2 flocks.
    for (var j = 0u; j < params.count; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        let d = o.pos.xyz - me.pos.xyz;
        let dist = length(d);
        if (dist < 40.0) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
            if (dist < 8.0) { sep -= d / max(dist, 0.01); }
        }
    }
    var v = me.vel.xyz;
    if (n > 0.0) {
        v += (center / n - me.pos.xyz) * 0.005 + (align / n - v) * 0.05;
    }
    v += sep * 0.1;
    v += vec3<f32>(-me.pos.x, 80.0 - me.pos.y, -me.pos.z) * 0.0005; // stay over the world
    let speed = clamp(length(v), 6.0, 18.0);
    v = normalize(v) * speed;
    me.vel = vec4<f32>(v, 0.0);
    me.pos = vec4<f32>(me.pos.xyz + v * params.dt, 1.0);
    boids[i] = me;
}
