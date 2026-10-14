// Tilt-shift: variable-radius gather blur over a golden-angle disc. The radius grows with the
// screen-space distance from a horizontal focal band and with the depth difference from the focus
// point. Taps only contribute when their own circle of confusion reaches the centre pixel, so a
// sharp foreground never smears into the blurred background.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct TiltShift {
    focus_uv_y: f32,
    band_half: f32,
    focus_depth: f32,
    near: f32,
    max_radius: f32,
    depth_weight: f32,
    saturation: f32,
    taps: u32,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var texture_sampler: sampler;
@group(0) @binding(3) var<uniform> settings: TiltShift;

const GOLDEN_ANGLE: f32 = 2.39996323;

fn smooth01(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Circle-of-confusion radius, in fractions of screen height, for a pixel at `uv` with depth `d`.
fn coc(uv: vec2<f32>, d: f32) -> f32 {
    let screen_t = smooth01((abs(uv.y - settings.focus_uv_y) - settings.band_half) / 0.3);
    var depth_t = 0.0;
    if d > 0.0 {
        // Reverse-Z infinite projection: view distance = near / depth.
        let z = settings.near / d;
        let tol = 0.35 * settings.focus_depth;
        depth_t = smooth01((abs(z - settings.focus_depth) - tol) / settings.focus_depth);
    }
    return settings.max_radius * max(screen_t, depth_t * settings.depth_weight);
}

fn load_depth(uv: vec2<f32>, dims: vec2<f32>) -> f32 {
    let p = clamp(vec2<i32>(uv * dims), vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
    return textureLoad(depth_texture, p, 0);
}

fn lift(rgb: vec3<f32>) -> vec3<f32> {
    let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return max(mix(vec3<f32>(lum), rgb, 1.0 + settings.saturation), vec3<f32>(0.0));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(screen_texture));
    let center = textureSampleLevel(screen_texture, texture_sampler, in.uv, 0.0);
    let c0 = coc(in.uv, load_depth(in.uv, dims));
    // Under half a pixel of blur: keep the pixel sharp.
    if c0 * dims.y < 0.5 {
        return vec4<f32>(lift(center.rgb), center.a);
    }

    let aspect = dims.x / dims.y;
    let noise = fract(52.9829189 * fract(dot(in.position.xy, vec2<f32>(0.06711056, 0.00583715))));
    let n = f32(settings.taps);
    var acc = center.rgb;
    var wsum = 1.0;
    for (var i = 0u; i < settings.taps; i = i + 1u) {
        let r = sqrt((f32(i) + 0.5) / n);
        let a = (f32(i) + noise) * GOLDEN_ANGLE;
        let dist = r * c0;
        let suv = in.uv + vec2<f32>(cos(a) / aspect, sin(a)) * dist;
        if any(suv < vec2<f32>(0.0)) || any(suv > vec2<f32>(1.0)) {
            continue;
        }
        let s_coc = coc(suv, load_depth(suv, dims));
        // Soft one-pixel edge: a tap counts only if its own blur circle covers the centre.
        let w = clamp((s_coc - dist) * dims.y + 1.0, 0.0, 1.0);
        let col = textureSampleLevel(screen_texture, texture_sampler, suv, 0.0).rgb;
        acc = acc + min(col, vec3<f32>(64.0)) * w;
        wsum = wsum + w;
    }
    return vec4<f32>(lift(acc / wsum), center.a);
}
