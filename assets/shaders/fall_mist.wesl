// Waterfall spray and mist (crates/hex-play/src/water/mist.rs): one quad per puff, all falls in
// one mesh. POSITION is the puff's origin on its tier landing, NORMAL its drift (down the fall, out
// over the lower tile), UV_0 the corner (±1, ±1), COLOR (phase, seed, radius scale m, 1 in the
// plunge pool). Each puff loops through its life as a pure function of the water clock
// (mist.time): it rises, drifts out, swells and fades. The vertex shader turns the quad to the
// camera; the fragment draws a soft, wispy falloff (no texture) lit like the scene, and fades it
// out toward the water it rises from so no puff shows a hard edge where it meets the surface.
import bevy_pbr::render::{
    forward_io::{Vertex, decompress_vertex},
    mesh_functions,
    mesh_bindings::mesh,
    mesh_view_bindings as vb,
    view_transformations as vt,
    pbr_types,
    pbr_functions,
};

struct MistParams {
    time: f32,
    alpha: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> mist: MistParams;

struct MistOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    // The quad corner (±1, ±1), turned by the puff's seed so no two puffs share a pattern.
    @location(1) corner: vec2<f32>,
    // The life envelope's opacity.
    @location(2) @interpolate(flat) fade: f32,
    @location(3) @interpolate(flat) seed: f32,
    // The water the puff rises from (world y).
    @location(4) @interpolate(flat) base_y: f32,
    @location(5) @interpolate(flat) instance_index: u32,
    // 1 for a plunge-pool puff, 0 for a lip's spray.
    @location(6) @interpolate(flat) pool: f32,
}

@vertex
fn vertex(v: Vertex) -> MistOut {
    let u = decompress_vertex(v, v.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let origin = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(u.position, 1.0)).xyz;
    let phase = u.color.x;
    let seed = u.color.y;
    let size = u.color.z;
    let pool = u.color.w > 0.5;
    let drift = vec3(u.normal.x, 0.0, u.normal.z);
    let lat = vec3(-drift.z, 0.0, drift.x);
    // Seconds per cycle: the pool's cloud lingers longer than a lip's spray.
    let life = select(mix(2.2, 3.4, seed), mix(3.4, 5.2, seed), pool);
    let a = fract(mist.time / life + phase);
    // Thrown out from the impact and slowing; rising steadily as it thins; a slow sway.
    // Both stay at the landing (owner 2026-11-24): the pool's mist and a lip's spray hug the
    // spot the sheet hits, never drifting out over the water or up above the fall.
    let reach = select(0.45, 0.35, pool) * size * (1.0 - (1.0 - a) * (1.0 - a));
    let rise = size * (0.05 + select(0.3, 0.25, pool) * a);
    let sway = 0.3 * size * sin(6.2831853 * (seed + 0.35 * a));
    let centre = origin + drift * reach + vec3(0.0, rise, 0.0) + lat * sway;
    let radius = size * (0.45 + 0.5 * sqrt(a));
    let right = normalize(vb::view.world_from_view[0].xyz);
    let up = normalize(vb::view.world_from_view[1].xyz);
    let c = u.uv;
    let world = centre + (right * c.x + up * c.y) * radius;
    var out: MistOut;
    out.position = vt::position_world_to_clip(world);
    out.world_position = vec4(world, 1.0);
    let ang = 6.2831853 * seed;
    out.corner = vec2(c.x * cos(ang) - c.y * sin(ang), c.x * sin(ang) + c.y * cos(ang));
    out.fade = smoothstep(0.0, 0.18, a) * (1.0 - smoothstep(0.35, 1.0, a));
    out.seed = seed;
    out.base_y = origin.y;
    out.instance_index = v.instance_index;
    out.pool = select(0.0, 1.0, pool);
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2(1.0, 0.0));
    let c = hash21(i + vec2(0.0, 1.0));
    let d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

@fragment
fn fragment(in: MistOut) -> @location(0) vec4<f32> {
    let r2 = dot(in.corner, in.corner);
    if (r2 >= 1.0) {
        discard;
    }
    // A soft core falling to nothing at the rim, broken into wisps that drift slowly.
    let soft = exp(-2.6 * r2) * (1.0 - r2);
    let q = in.corner * 1.7 + vec2(in.seed * 17.0, in.seed * 5.0) + vec2(0.0, -0.12 * mist.time);
    let wisps = 0.45 + 0.4 * value_noise(q) + 0.25 * value_noise(q * 2.3 + 3.1);
    let wp = in.world_position.xyz;
    let ground = smoothstep(in.base_y - 0.1, in.base_y + 0.45, wp.y);
    // A lip's spray stands in front of a white sheet, so it is denser than the pool's mist.
    let boost = select(2.3, 1.0, in.pool > 0.5);
    let alpha = clamp(soft * wisps * in.fade * ground * mist.alpha * boost, 0.0, 1.0);
    if (alpha < 0.003) {
        discard;
    }
    // Lit as a rough white volume facing up and toward the eye: the sun, sky and exposure of the
    // scene, then its fog.
    var pbr = pbr_types::pbr_input_new();
    pbr.flags = mesh[in.instance_index].flags;
    pbr.is_orthographic = vb::view.clip_from_view[3].w == 1.0;
    pbr.V = pbr_functions::calculate_view(in.world_position, pbr.is_orthographic);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    let n = normalize(pbr.V + vec3(0.0, 1.2, 0.0));
    pbr.world_normal = n;
    pbr.N = n;
    // A cool grey, a shade below the sheet's white, so the spray reads against the falling water.
    pbr.material.base_color = vec4(0.74, 0.79, 0.83, 1.0);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.metallic = 0.0;
    pbr.material.reflectance = vec3(0.0);
    let color = pbr_functions::apply_pbr_lighting(pbr).rgb;
    return pbr_functions::main_pass_post_lighting_processing(pbr, vec4(color, alpha));
}
