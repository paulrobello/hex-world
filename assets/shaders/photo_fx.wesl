// Photo-mode effects (owner, 2026-10-14): chromatic aberration, a colour filter, a vignette and
// film grain, in that order, on the tone-mapped image. After tone mapping the values are
// display-referred but still linear (the sRGB encode happens at upscaling), so the filter, vignette
// and grain work on a gamma 2.2 copy, where their steps look even. Every size is a fraction of the
// target, so a 2x or 4x export looks like the viewfinder. Grain is the same in every channel, so a
// black-and-white image stays grey.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct PhotoFx {
    // 0 normal, 1 black and white, 2 sepia.
    filter_mode: u32,
    filter_strength: f32,
    // Darkening at the corners, 0..1.
    vignette: f32,
    // Normalised radius (0 centre, 1 corner) where the vignette starts.
    vignette_inner: f32,
    // Grain amplitude in gamma space.
    grain: f32,
    // Grain cells per target height.
    grain_rows: f32,
    // Red and blue shift at the corners, as a fraction of the target size.
    aberration: f32,
    seed: u32,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> fx: PhotoFx;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
const SEPIA: vec3<f32> = vec3<f32>(1.12, 0.96, 0.72);

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn lattice(x: i32, y: i32) -> f32 {
    let h = pcg(bitcast<u32>(x) ^ pcg(bitcast<u32>(y) ^ pcg(fx.seed)));
    return f32(h) * (1.0 / 4294967295.0);
}

// Smooth value noise on a lattice `grain_rows` cells tall, in 0..1.
fn grain_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let x = i32(i.x);
    let y = i32(i.y);
    let a = lattice(x, y);
    let b = lattice(x + 1, y);
    let c = lattice(x, y + 1);
    let d = lattice(x + 1, y + 1);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(screen_texture));
    let center = textureSampleLevel(screen_texture, texture_sampler, in.uv, 0.0);
    var lin = center.rgb;
    if fx.aberration > 0.0 {
        // Radial: none at the centre, `aberration` of the target at the corners.
        let shift = (in.uv - vec2<f32>(0.5)) * (2.0 * fx.aberration);
        lin.r = textureSampleLevel(screen_texture, texture_sampler, in.uv + shift, 0.0).r;
        lin.b = textureSampleLevel(screen_texture, texture_sampler, in.uv - shift, 0.0).b;
    }
    lin = max(lin, vec3<f32>(0.0));
    var g = pow(lin, vec3<f32>(1.0 / 2.2));

    if fx.filter_mode != 0u {
        let mono = pow(dot(lin, LUMA), 1.0 / 2.2);
        var f = vec3<f32>(mono);
        if fx.filter_mode == 2u {
            f = min(mono * SEPIA, vec3<f32>(1.0));
        }
        g = mix(g, f, fx.filter_strength);
    }

    if fx.vignette > 0.0 {
        let aspect = dims.x / dims.y;
        let corner = length(vec2<f32>(aspect, 1.0) * 0.5);
        let d = length((in.uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0)) / corner;
        g = g * (1.0 - fx.vignette * smoothstep(fx.vignette_inner, 1.0, d));
    }

    if fx.grain > 0.0 {
        let n = grain_noise(in.position.xy * (fx.grain_rows / dims.y)) - 0.5;
        // Stronger in the shadows and midtones, as film grain reads.
        let l = clamp(dot(g, LUMA), 0.0, 1.0);
        g = g + vec3<f32>(n * fx.grain * (1.0 - 0.5 * l));
    }

    return vec4<f32>(pow(max(g, vec3<f32>(0.0)), vec3<f32>(2.2)), center.a);
}
