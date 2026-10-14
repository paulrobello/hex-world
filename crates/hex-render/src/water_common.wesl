// `hex_render::water_common`: wave, swash and caustic functions shared by assets/shaders/water.wesl
// and water_terrain.wesl (crates/hex-render/src/water.rs). Embedded like kit_common (see its header
// for why asset-path WESL imports 404 on the web). Every function takes world-space XZ, so water
// is continuous across tile joints and rotations. Declares no bindings.

const TAU: f32 = 6.28318530718;
const GRAVITY: f32 = 9.81;
// Waves run toward the beach (-x), slightly oblique.
const WIND_ANGLE: f32 = 2.8;
// Largest Gerstner wavelength (m) and the ratio between successive waves. Deep-water dispersion
// (omega = sqrt(g k)) makes the periods incommensurate, so the sum never visibly repeats.
const WAVE_L0: f32 = 7.5;
const WAVE_RATIO: f32 = 0.74;
const WAVE_AMP_PER_M: f32 = 0.015;
// Sum of Q k A over all waves: below 1, so crests sharpen without looping over.
const WAVE_STEEPNESS: f32 = 0.8;

fn hash11(n: f32) -> f32 {
    return fract(sin(n * 127.1 + 31.7) * 43758.5453);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let q = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

struct Gerstner {
    // Displacement of the surface point (m).
    disp: vec3<f32>,
    // Height gradient (dh/dx, dh/dz) of the undisplaced parameterization.
    slope: vec2<f32>,
    // Sum of Q k A sin: how close the crests are to pinching (0..WAVE_STEEPNESS).
    pinch: f32,
}

fn gerstner(p: vec2<f32>, t: f32, count: u32) -> Gerstner {
    var g: Gerstner;
    g.disp = vec3(0.0);
    g.slope = vec2(0.0);
    g.pinch = 0.0;
    let n = f32(count);
    for (var i = 0u; i < count; i++) {
        let fi = f32(i);
        let a = WIND_ANGLE + (hash11(fi + 1.0) - 0.5) * 1.4;
        let d = vec2(cos(a), sin(a));
        let l = WAVE_L0 * pow(WAVE_RATIO, fi);
        let k = TAU / l;
        let w = sqrt(GRAVITY * k);
        let amp = WAVE_AMP_PER_M * l;
        let q = min(WAVE_STEEPNESS / (k * amp * n), 1.0);
        let f = k * dot(d, p) - w * t + hash11(fi + 7.3) * TAU;
        let c = cos(f);
        let s = sin(f);
        g.disp += vec3(q * amp * d.x * c, amp * s, q * amp * d.y * c);
        g.slope += d * (k * amp * c);
        g.pinch += q * k * amp * s;
    }
    return g;
}

// Normal of a Gerstner sum scaled by `atten` (shore and river damping).
fn gerstner_normal(g: Gerstner, atten: f32) -> vec3<f32> {
    return normalize(vec3(-g.slope.x * atten, 1.0 - g.pinch * atten, -g.slope.y * atten));
}

// Wave damping: none on open water deeper than 2.5 m, gone at the waterline and in the river.
fn wave_atten(depth: f32, river: f32) -> f32 {
    return smoothstep(0.05, 2.5, depth) * (1.0 - river);
}

// Swash on the beach, in units of the material's wash amplitude: about -0.4 (backwash) to 1
// (run-up). Two incommensurate periods, phase-lagged with distance from the coast.
fn swash(shore: f32, t: f32) -> f32 {
    let a = 0.6 * sin(0.95 * t - 0.45 * shore) + 0.4 * sin(0.61 * t - 0.3 * shore + 1.7);
    return a * 0.7 + 0.3;
}

// The kit coast bed (docs/kit/water-contract.md §3, water/tiles.rs `coast_height`): water at rest
// at y = -1.4, the beach slope rising 1.4 m over the 8 m to the grass, the shelf falling 0.12 m/m.
// tiles.rs tests hold these to the Rust constants.
const COAST_WATER_Y: f32 = -1.4;
const COAST_LAND: f32 = 1.4;
const COAST_BEACH_TOP_S: f32 = -8.0;
const COAST_SHELF_SLOPE: f32 = 0.12;

// Seaward distance `s` (the water surface's shore distance) of a coast-bed point `rel` m above the
// water at rest: the coast profile inverted on the beach slope and the shelf, where it is one to
// one. A kit bed carries no shore distance of its own, and its profile is held to ±0.03 m there.
fn coast_shore(rel: f32) -> f32 {
    let t = 1.0 - sqrt(max(1.0 - rel / COAST_LAND, 0.0));
    return select(COAST_BEACH_TOP_S * t, -rel / COAST_SHELF_SLOPE, rel < 0.0);
}

// Smooth value noise (about 0..1, one cell per unit): a breakup that binds no texture.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash22(i).x;
    let b = hash22(i + vec2(1.0, 0.0)).x;
    let c = hash22(i + vec2(0.0, 1.0)).x;
    let d = hash22(i + vec2(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// How much of the swash a point feels: full within 2.5 m of the coast (and inland of it), on open
// sea only.
fn swash_weight(depth: f32, river: f32) -> f32 {
    return (1.0 - smoothstep(0.3, 2.5, depth)) * (1.0 - river);
}

// One animated Voronoi edge layer: bright on cell borders, the network caustics form.
fn caustic_layer(p: vec2<f32>, t: f32) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2(f32(x), f32(y));
            let h = hash22(i + o);
            let c = o + 0.5 + 0.4 * sin(t * (0.5 + 0.6 * h) + TAU * h);
            let d = length(c - f);
            if (d < d1) {
                d2 = d1;
                d1 = d;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return 1.0 - smoothstep(0.0, 0.18, d2 - d1);
}

// Caustic intensity (about 0..1) on a bed point. `hq` adds a second, crossing layer.
fn caustics(p: vec2<f32>, t: f32, hq: bool) -> f32 {
    let warp = 0.25 * vec2(sin(p.y * 1.3 + t * 0.7), sin(p.x * 1.1 - t * 0.6));
    let a = caustic_layer(p * 0.85 + warp, t * 0.9);
    if (!hq) {
        return a * a;
    }
    let b = caustic_layer(p * 1.27 - warp.yx + vec2(t * 0.05, -t * 0.03), t * 1.1 + 3.0);
    return a * b * 1.6 + 0.15 * a * a;
}
