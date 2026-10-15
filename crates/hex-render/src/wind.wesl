// `hex_render::wind`: the world's wind field, shared by every swaying material (grass blades now,
// tree sway later; crates/hex-render/src/play/grass.rs). Embedded like kit_common: an asset-path
// WESL import 404s on the web. Declares no bindings and samples nothing (no sampled-texture
// budget is left beside refracting water); all noise is integer-hashed value noise, unrolled.
//
// `sample(p, t)` at world xz `p` (m) and time `t` (s) returns the horizontal push, a bend per unit
// of height (0 is still air, about 0.8 the strongest gust), and the gust level in 0..1 for shading.
// Every layer scrolls its sample position with time (never time as a noise axis), so the field
// moves without boiling. Direction and strength come from separate noise fields:
// - Direction: the prevailing wind turned by a slow 90 m field and a 20 m detail field at 35%.
// - Gusts: two octaves at 24 m and 12 m scrolled downwind at 6.5 m/s: bright bands of bent grass
//   rolling across a meadow every few seconds.
// - Patches: 7 m noise scrolled at 2 m/s, the patch-to-patch strength variation.
// - Flutter: 0.9 m noise scrolled at 4.5 m/s, a small fast push across the wind.
// Callers pass the wind clock: `Time` advances by exactly 1/60 s per frame under `--clock fixed`,
// so captures are reproducible.

// Prevailing direction (unit, world xz) and the layers' scales (m), speeds (m/s) and amplitudes.
const PREVAILING: vec2<f32> = vec2(0.8, 0.6);
const DIR_SCALE: f32 = 90.0;
const DIR_DETAIL_SCALE: f32 = 20.0;
const DIR_SPEED: f32 = 1.2;
const DIR_SWING: f32 = 0.9;
const DIR_DETAIL: f32 = 0.35;
const GUST_SCALE: f32 = 24.0;
const GUST_SPEED: f32 = 6.5;
const GUST_AMP: f32 = 0.55;
const PATCH_SCALE: f32 = 7.0;
const PATCH_SPEED: f32 = 2.0;
const PATCH_AMP: f32 = 0.14;
const FLUTTER_SCALE: f32 = 0.9;
const FLUTTER_SPEED: f32 = 4.5;
const FLUTTER_AMP: f32 = 0.07;
const BASE: f32 = 0.12;

struct Wind {
    push: vec2<f32>,
    gust: f32,
}

fn hash(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return (h >> 22u) ^ h;
}

fn lattice(i: vec2<i32>, seed: u32) -> f32 {
    let h = hash((bitcast<u32>(i.x) * 0x8da6b343u) ^ (bitcast<u32>(i.y) * 0xd8163841u) ^ seed);
    return f32(h & 0xffffffu) / 16777216.0;
}

// Value noise in [0, 1) on a unit lattice, smoothstep-blended.
fn value_noise(p: vec2<f32>, seed: u32) -> f32 {
    let c = floor(p);
    let f = p - c;
    let u = f * f * (3.0 - 2.0 * f);
    let i = vec2<i32>(c);
    let a = lattice(i, seed);
    let b = lattice(i + vec2(1, 0), seed);
    let d = lattice(i + vec2(0, 1), seed);
    let e = lattice(i + vec2(1, 1), seed);
    return mix(mix(a, b, u.x), mix(d, e, u.x), u.y);
}

fn sample(p: vec2<f32>, t: f32) -> Wind {
    let drift = PREVAILING * t;
    let d0 = value_noise((p - drift * DIR_SPEED) / DIR_SCALE, 0x1729u);
    let d1 = value_noise((p - drift * (DIR_SPEED * 2.0)) / DIR_DETAIL_SCALE, 0x2843u);
    let swing = ((d0 - 0.5) + DIR_DETAIL * (d1 - 0.5)) * 2.0 * DIR_SWING;
    let c = cos(swing);
    let s = sin(swing);
    let dir = vec2(PREVAILING.x * c - PREVAILING.y * s, PREVAILING.x * s + PREVAILING.y * c);

    let q = p - drift * GUST_SPEED;
    let g = (value_noise(q / GUST_SCALE, 0x3b1du) + 0.5 * value_noise(q / (0.5 * GUST_SCALE), 0x51c3u)) / 1.5;
    let gust = smoothstep(0.36, 0.66, g);
    let patches = value_noise((p - drift * PATCH_SPEED) / PATCH_SCALE, 0x6e2fu);
    let strength = max(BASE + GUST_AMP * gust + PATCH_AMP * (patches - 0.5) * 2.0, 0.0);
    let f = value_noise((p - drift * FLUTTER_SPEED) / FLUTTER_SCALE, 0x7a95u);
    let flutter = (f - 0.5) * 2.0 * FLUTTER_AMP * (0.6 + gust);
    return Wind(dir * strength + vec2(-dir.y, dir.x) * flutter, gust);
}
