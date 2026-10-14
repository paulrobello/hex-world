// GPU boids (crates/hex-render/src/boids.rs). The CPU plan (boids/plan.rs) owns every decision:
// which slots are birds and which fish, where each spawns, which roost a bird is sent to and when,
// and the fish field. This pass only moves the flock. Constants mirror plan.rs.
struct Boid { pos: vec4<f32>, vel: vec4<f32> };
struct Ctl { spawn: vec4<f32>, info: vec4<f32> };
struct Params {
    count: u32, birds: u32, fish: u32, roosts: u32,
    dt: f32, time: f32, mode: u32, field_w: u32,
    field_h: u32, field_x0: i32, field_z0: i32, _pad: u32,
    sky: vec4<f32>,
};
@group(0) @binding(0) var<storage, read_write> boids: array<Boid>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> ctl: array<Ctl>;
@group(0) @binding(3) var<storage, read> roosts: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> chunks: array<i32>;
@group(0) @binding(5) var<storage, read> field: array<vec4<f32>>;

// vel.w: what the boid is doing, read by boid_material.wesl.
const FLY: f32 = 0.0;
const SETTLE: f32 = 1.0;
const PERCH: f32 = 2.0;
const SWIM: f32 = 3.0;

// plan.rs: the fish field's chunks, and the roost and sky constants.
const CHUNK: f32 = 16.0;
const SIDE: u32 = 17u;
const STEP: f32 = 1.0;
const LAND: vec4<f32> = vec4<f32>(0.0, -1.0, -4.0, 0.0);
const APPROACH: f32 = 8.0;
const SKY_LOW: f32 = 22.0;
const SKY_HIGH: f32 = 44.0;
const TAU: f32 = 6.2831853;

fn hash(n: u32) -> f32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    x = (x >> 22u) ^ x;
    return f32(x) / 4294967295.0;
}

fn norm2(v: vec2<f32>) -> vec2<f32> {
    let l = length(v);
    if (l > 1e-6) { return v / l; }
    return vec2<f32>(0.0);
}

// The fish band at XZ p, bilinear: (lowest centre y, highest centre y, room, 0). FishField::sample.
fn band(p: vec2<f32>) -> vec4<f32> {
    let c = floor(p / CHUNK);
    let cx = i32(c.x) - params.field_x0;
    let cz = i32(c.y) - params.field_z0;
    if (cx < 0 || cz < 0 || cx >= i32(params.field_w) || cz >= i32(params.field_h)) {
        return LAND;
    }
    let k = chunks[u32(cz) * params.field_w + u32(cx)];
    if (k < 0) {
        return LAND;
    }
    let g = clamp((p - c * CHUNK) / STEP, vec2<f32>(0.0), vec2<f32>(f32(SIDE - 1u) - 0.001));
    let i = floor(g);
    let f = g - i;
    let base = u32(k) * SIDE * SIDE + u32(i.y) * SIDE + u32(i.x);
    let a = mix(field[base], field[base + 1u], f.x);
    let b = mix(field[base + SIDE], field[base + SIDE + 1u], f.x);
    return mix(a, b, f.y);
}

// FishField::grad.
fn room_grad(p: vec2<f32>) -> vec2<f32> {
    let h = 0.5;
    return vec2<f32>(
        band(p + vec2<f32>(h, 0.0)).z - band(p - vec2<f32>(h, 0.0)).z,
        band(p + vec2<f32>(0.0, h)).z - band(p - vec2<f32>(0.0, h)).z,
    ) / (2.0 * h);
}

struct Step { p: vec3<f32>, v: vec3<f32> };

// plan::confine: integrate; if the new XZ has no room, stay and turn up the room gradient; then
// clamp y into the band.
fn confine(p: vec3<f32>, v: vec3<f32>, dt: f32) -> Step {
    var np = p + v * dt;
    var nv = v;
    var b = band(np.xz);
    if (b.z < 0.0) {
        let g = room_grad(p.xz);
        var away = -norm2(v.xz);
        if (dot(g, g) > 1e-8) { away = normalize(g); }
        let speed = length(v.xz);
        nv = vec3<f32>(away.x * speed, v.y, away.y * speed);
        np = vec3<f32>(p.x, np.y, p.z);
        b = band(p.xz);
    }
    np.y = clamp(np.y, b.x, max(b.y, b.x));
    return Step(np, nv);
}

// The P0 stress flock: every slot a bird over the world.
fn stress(i: u32, me: Boid) -> Boid {
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    // O(n^2) over the 1,000-boid budget.
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
    v = normalize(v + vec3<f32>(1e-5, 0.0, 0.0)) * speed;
    return Boid(vec4<f32>(me.pos.xyz + v * params.dt, me.pos.w), vec4<f32>(v, FLY));
}

// A play bird: flocks over the world, flies to its roost when sent and sits there, launches when
// released.
fn fly(i: u32, me: Boid, c: Ctl) -> Boid {
    let p = me.pos.xyz;
    var v = me.vel.xyz;
    let dt = params.dt;
    let k = dt * 60.0;
    let ri = i32(c.info.y);
    if (ri >= 0 && u32(ri) < params.roosts) {
        let r = roosts[ri].xyz;
        let age = params.time - c.info.z;
        var hd = vec3<f32>(0.0, 0.0, 1.0);
        if (length(v.xz) > 1e-6) { hd = normalize(vec3<f32>(v.x, 0.0, v.z)); }
        if (age >= APPROACH) {
            return Boid(vec4<f32>(r, me.pos.w), vec4<f32>(hd * 0.001, PERCH));
        }
        let to = r - p;
        let dist = length(to);
        let desired = to / max(dist, 1e-3) * clamp(dist * 0.6, 2.0, 15.0);
        v = mix(v, desired, clamp(2.5 * dt, 0.0, 1.0));
        // Pulled onto the perch by APPROACH, so the CPU's roosting count is what is drawn.
        let w = pow(clamp(age / APPROACH, 0.0, 1.0), 6.0);
        let np = mix(p + v * dt, r, w);
        return Boid(vec4<f32>(np, me.pos.w), vec4<f32>(v, SETTLE));
    }
    if (me.vel.w == PERCH || me.vel.w == SETTLE) {
        // Released: up and out of the canopy.
        let a = hash(i * 13u + u32(params.time * 7.0)) * TAU;
        v = vec3<f32>(cos(a) * 5.0, 7.0, sin(a) * 5.0);
    }
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    for (var j = 0u; j < params.birds; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        if (o.vel.w != FLY) { continue; }
        let d = o.pos.xyz - p;
        let dist = length(d);
        if (dist < 22.0) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
            if (dist < 3.0) { sep -= d / max(dist, 0.05); }
        }
    }
    if (n > 0.0) {
        v += ((center / n - p) * 0.004 + (align / n - v) * 0.04) * k;
    }
    v += sep * (0.12 * k);
    // Stay over the world, inside the altitude band.
    let off = p.xz - params.sky.xz;
    let od = length(off);
    if (od > params.sky.w) {
        v -= vec3<f32>(off.x, 0.0, off.y) / od * ((od - params.sky.w) * 0.01 * k);
    }
    if (p.y < SKY_LOW) { v.y += (SKY_LOW - p.y) * 0.05 * k; }
    if (p.y > SKY_HIGH) { v.y -= (p.y - SKY_HIGH) * 0.05 * k; }
    v.y -= v.y * 0.02 * k;
    let speed = clamp(length(v), 5.0, 12.0);
    v = normalize(v + vec3<f32>(1e-5, 0.0, 0.0)) * speed;
    return Boid(vec4<f32>(p + v * dt, me.pos.w), vec4<f32>(v, FLY));
}

// A fish: schools with its neighbours, wanders, holds the upper water column and turns away from
// shores and cut faces; `confine` keeps it in the band.
fn swim(i: u32, me: Boid, c: Ctl) -> Boid {
    let p = me.pos.xyz;
    var v = me.vel.xyz;
    let dt = params.dt;
    let here = band(p.xz);
    if (here.z < 0.0) {
        // Stranded: the water under it went (bulldoze, undo). The plan moved its spawn.
        return Boid(vec4<f32>(c.spawn.xyz, me.pos.w), vec4<f32>(v, SWIM));
    }
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    for (var j = params.birds; j < params.birds + params.fish; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        let d = o.pos.xyz - p;
        let dist = length(d);
        if (dist < 4.0) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
            if (dist < 0.9) { sep -= d / max(dist * dist, 0.05); }
        }
    }
    if (n > 0.0) {
        v += (center / n - p) * (0.8 * dt) + (align / n - v) * (1.2 * dt);
    }
    v += sep * (0.6 * dt);
    // Wander: a slow turn that changes every few seconds.
    let slot = u32(params.time * 0.35 + hash(i * 3u) * 8.0);
    let turn = (hash(i * 7919u + slot) - 0.5) * 1.6 * dt;
    let s = sin(turn);
    let co = cos(turn);
    v = vec3<f32>(v.x * co - v.z * s, v.y, v.x * s + v.z * co);
    // Hold the upper part of the column, where the fish read through the surface.
    let target_y = mix(here.x, here.y, 0.7);
    v.y += (target_y - p.y) * (0.6 * dt) - v.y * (1.5 * dt);
    // Turn away from shores and cut faces before reaching them.
    let ahead = band(p.xz + norm2(v.xz) * 2.5);
    let near = min(here.z, ahead.z);
    if (near < 1.0) {
        let g = room_grad(p.xz);
        let push = (1.0 - near) * 4.0 * dt;
        v = vec3<f32>(v.x + g.x * push, v.y, v.z + g.y * push);
    }
    var h = v.xz;
    let hs = length(h);
    if (hs < 0.6) {
        var dir = vec2<f32>(1.0, 0.0);
        if (hs > 1e-4) { dir = h / hs; }
        h = dir * 0.6;
    } else if (hs > 2.0) {
        h = h / hs * 2.0;
    }
    v = vec3<f32>(h.x, clamp(v.y, -0.4, 0.4), h.y);
    let st = confine(p, v, dt);
    return Boid(vec4<f32>(st.p, me.pos.w), vec4<f32>(st.v, SWIM));
}

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params.count) { return; }
    let me = boids[i];
    let c = ctl[i];
    let fish = c.info.x > 0.5;
    if (me.pos.w != c.spawn.w) {
        // A new generation: (re)spawn the slot where the plan put it.
        let a = hash(i * 2654435761u + u32(c.spawn.w)) * TAU;
        var sp = 8.0;
        var mode = FLY;
        if (fish) { sp = 1.0; mode = SWIM; }
        boids[i] = Boid(vec4<f32>(c.spawn.xyz, c.spawn.w), vec4<f32>(cos(a) * sp, 0.0, sin(a) * sp, mode));
        return;
    }
    if (params.mode == 0u) {
        boids[i] = stress(i, me);
    } else if (fish) {
        boids[i] = swim(i, me, c);
    } else {
        boids[i] = fly(i, me, c);
    }
}
