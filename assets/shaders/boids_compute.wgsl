// GPU boids (crates/hex-render/src/boids.rs). The CPU plan (boids/plan.rs) owns every decision:
// which slots are birds and which fish, where each spawns, which roost a bird is sent to and when,
// and the fish field. This pass only moves the flock. Constants mirror plan.rs.
// anim (read by boid_material.wesl): x wing-beat or tail-wag phase, y flap drive (0 glide, 1 flap),
// z the fish palette's U offset (ctl.look.x).
struct Boid { pos: vec4<f32>, vel: vec4<f32>, anim: vec4<f32> };
struct Ctl { spawn: vec4<f32>, info: vec4<f32>, look: vec4<f32> };
struct Params {
    count: u32, birds: u32, fish: u32, roosts: u32,
    dt: f32, time: f32, mode: u32, field_w: u32,
    field_h: u32, field_x0: i32, field_z0: i32, flock: u32,
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
// A slot play leaves empty (plan::BIRD_PERCENT): parked, and collapsed by boid_material.wesl.
const IDLE: f32 = 4.0;

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
    return Boid(vec4<f32>(me.pos.xyz + v * params.dt, me.pos.w), vec4<f32>(v, FLY), me.anim);
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
        if (age >= APPROACH) {
            // Perched: level and facing out from the tree (roost w, the CPU's heading angle), so
            // the material draws the bird upright on the canopy, not along the slope it came in on.
            let face = roosts[ri].w;
            return Boid(vec4<f32>(r, me.pos.w), vec4<f32>(vec3<f32>(cos(face), 0.0, sin(face)) * 0.001, PERCH), me.anim);
        }
        let to = r - p;
        let dist = length(to);
        let desired = to / max(dist, 1e-3) * clamp(dist * 0.6, 2.0, 15.0);
        v = mix(v, desired, clamp(2.5 * dt, 0.0, 1.0));
        // Pulled onto the perch by APPROACH, so the CPU's roosting count is what is drawn.
        let w = pow(clamp(age / APPROACH, 0.0, 1.0), 6.0);
        let np = mix(p + v * dt, r, w);
        return Boid(vec4<f32>(np, me.pos.w), vec4<f32>(v, SETTLE), me.anim);
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
    // plan::FLOCK_SIZE: cohere and align with the own flock only, keep clear of every bird.
    let size = max(params.flock, 1u);
    let flock = i / size;
    for (var j = 0u; j < params.birds; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        if (o.vel.w != FLY) { continue; }
        let d = o.pos.xyz - p;
        let dist = length(d);
        if (dist < 3.0) { sep -= d / max(dist, 0.05); }
        if (dist < 22.0 && j / size == flock) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
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
    return Boid(vec4<f32>(p + v * dt, me.pos.w), vec4<f32>(v, FLY), me.anim);
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
        return Boid(vec4<f32>(c.spawn.xyz, me.pos.w), vec4<f32>(v, SWIM), me.anim);
    }
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    // plan::BoidPlan::schools: cohere and align with the own school only (a sea region's one
    // school, a river's group of 1 to 5), keep clear of every fish.
    for (var j = params.birds; j < params.birds + params.fish; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        let d = o.pos.xyz - p;
        let dist = length(d);
        if (dist < 4.0 && ctl[j].info.w == c.info.w) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
        }
        if (dist < 0.9) { sep -= d / max(dist * dist, 0.05); }
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
    return Boid(vec4<f32>(st.p, me.pos.w), vec4<f32>(st.v, SWIM), me.anim);
}

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params.count) { return; }
    let me = boids[i];
    let c = ctl[i];
    if (c.info.x > 1.5) {
        boids[i] = Boid(vec4<f32>(c.spawn.xyz, c.spawn.w), vec4<f32>(0.0, 0.0, 0.0, IDLE), vec4<f32>(0.0));
        return;
    }
    let fish = c.info.x > 0.5;
    if (me.pos.w != c.spawn.w) {
        // A new generation: (re)spawn the slot where the plan put it.
        let a = hash(i * 2654435761u + u32(c.spawn.w)) * TAU;
        var sp = 8.0;
        var mode = FLY;
        if (fish) { sp = 1.0; mode = SWIM; }
        let anim = vec4<f32>(hash(i * 977u + 5u) * TAU, 1.0, c.look.x, 0.0);
        boids[i] = Boid(vec4<f32>(c.spawn.xyz, c.spawn.w), vec4<f32>(cos(a) * sp, 0.0, sin(a) * sp, mode), anim);
        return;
    }
    var next: Boid;
    if (params.mode == 0u) {
        next = stress(i, me);
    } else if (fish) {
        next = swim(i, me, c);
    } else {
        next = fly(i, me, c);
    }
    next.anim = animate(i, next, c);
    boids[i] = next;
}

// Wing beats and tail wags advance with the boid clock, so they hold still with the flock when
// photo mode pauses the agents. Birds alternate bursts of flapping with glides (starlings), flap
// harder while climbing and settling, and fold their wings on the perch (no beat).
fn animate(i: u32, b: Boid, c: Ctl) -> vec4<f32> {
    let dt = params.dt;
    var a = b.anim;
    let mode = b.vel.w;
    var rate = 0.0;
    if (mode == SWIM) {
        rate = TAU * (1.6 + 0.9 * length(b.vel.xyz));
        a.y = 1.0;
    } else if (mode == PERCH) {
        a.y = 0.0;
    } else {
        let burst = sin(params.time * 0.8 + hash(i * 31u + 7u) * TAU);
        var drive = clamp(0.5 + 1.4 * burst + 0.3 * b.vel.y, 0.0, 1.0);
        rate = TAU * 3.2;
        if (mode == SETTLE) { drive = 1.0; rate = TAU * 4.5; }
        a.y = mix(a.y, drive, clamp(dt * 3.0, 0.0, 1.0));
    }
    a.x = a.x + rate * dt;
    if (a.x > TAU) { a.x = a.x - TAU * floor(a.x / TAU); }
    a.z = c.look.x;
    return a;
}
