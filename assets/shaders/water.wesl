// Hero water (crates/hex-render/src/water.rs, PRD §9.1). Custom Material: bindings 100-102 only.
// Vertex: summed Gerstner displacement plus shoreline swash, at the vertex's world XZ. Fragment:
// analytic wave normal + scrolling ripple detail (sea) or two-phase flow-map ripples (river), foam,
// Bevy PBR lighting for the reflective surface (IBL, sun glints, shadows) and, under
// WATER_REFRACTION, refraction of the opaque pass with depth absorption and bed caustics. Water is
// not in the depth prepass, so the prepass depth under a water pixel is the bed. `freeze` is the
// §9.4 hook for frozen rivers, 0 (open water) in the MVP.
import bevy_pbr::render::{
    forward_io::{Vertex, decompress_vertex},
    mesh_functions,
    mesh_bindings::mesh,
    mesh_view_bindings as vb,
    view_transformations as vt,
    pbr_types,
    pbr_functions,
};
import bevy_pbr::prepass::utils as prepass_utils;
import hex_render::water_common as wc;

struct WaterParams {
    water_time: f32,
    wave_count: u32,
    sea_level: f32,
    wash_amp: f32,
    // §9.4 water freeze (crates/hex-render/src/hooks.rs): 0 is open water, the MVP value; 1 is a
    // frozen sheet. Read only by the WATER_FREEZE variant (water.rs `WaterKey::freeze`).
    freeze: f32,
    // Review-only churn foam offset (`--water_look churn=`, water.rs `WaterLook`): the churn term
    // is scaled by 1 + this, 0 in every shipped build.
    churn_delta: f32,
    _pad1: f32,
    _pad2: f32,
    // Boat hulls (water.rs `WaterParams::hulls`, hex_play::boats): two per boat, (centre xz,
    // heading xz) then (half length, half beam, top y, centre offset along the heading).
    hulls: array<vec4<f32>, 12>,
    boat_count: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> water: WaterParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var water_noise_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var water_noise_sampler: sampler;

struct WaterVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    // Undisplaced world XZ: the fragment re-evaluates the waves here.
    @location(1) rest_xz: vec2<f32>,
    @location(2) flow: vec2<f32>,
    // (river, depth at rest, shore distance)
    @location(3) site: vec3<f32>,
    @location(4) @interpolate(flat) instance_index: u32,
    // 1 on the cut faces of the volume at exposed tile edges (water.rs `skirt_mesh`), 0 on the surface.
    @location(5) skirt: f32,
    // Cut faces: terrain height under this point of the edge (the face is clipped below it).
    // Surface: the river-mouth foam weight (water.rs `MEET_TAG`), 0 on every other tile.
    @location(6) ground: f32,
    // Surface: the river churn (water.rs `foam_tag`, the flow's advective acceleration in m/s²).
    @location(7) churn: f32,
}

// water.rs `SKIRT_TAG`: a cut-face vertex carries SKIRT_TAG + terrain height in UV_0.y.
const SKIRT_TAG: f32 = 10.0;
// water.rs `MEET_TAG`: a surface vertex carries the mouth foam weight times MEET_TAG in UV_0.y.
const MEET_TAG: f32 = 0.4;

@vertex
fn vertex(v: Vertex) -> WaterVarying {
    var out_ground = 0.0;
    let u = decompress_vertex(v, v.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let rest = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(u.position, 1.0)).xyz;
    let river = u.color.z;
    let depth = u.color.w;
    let shore = u.uv.x;
    let t = water.water_time;
    // Surface vertices and skirt tops (at rest on the sea level) move with the waves, by the same
    // function at the same world XZ as the rim they share, so the cut face never parts from the surface.
    let skirt = u.uv.y > 0.5;
    out_ground = select(max(u.uv.y, 0.0) / MEET_TAG, u.uv.y - SKIRT_TAG, skirt);
    let moves = !skirt || rest.y > water.sea_level - 1e-4;
    let g = wc::gerstner(rest.xz, t, water.wave_count);
    var p = rest;
    if (moves) {
        p += g.disp * wc::wave_atten(depth, river);
        p.y += water.wash_amp * wc::swash(shore, t) * wc::swash_weight(depth, river);
    }
    // Freeze: ice holds the surface at rest (no waves, no swash). Compiled only into the frozen
    // variant (WATER_FREEZE), so open water runs the approved shader unchanged.
    @if(WATER_FREEZE) {
        p = mix(p, rest, water.freeze);
    }
    var out: WaterVarying;
    out.skirt = select(0.0, 1.0, skirt);
    out.ground = out_ground;
    out.churn = select(max(-u.uv.y, 0.0), 0.0, skirt);
    out.position = vt::position_world_to_clip(p);
    out.world_position = vec4(p, 1.0);
    out.rest_xz = rest.xz;
    out.flow = u.color.xy;
    out.site = vec3(river, depth, shore);
    out.instance_index = v.instance_index;
    return out;
}

// Ripple slope (dh/dx, dh/dz) from the noise texture's normal channels; `scale_m` metres per tile.
fn ripple(p: vec2<f32>, scale_m: f32) -> vec2<f32> {
    let n = textureSample(water_noise_texture, water_noise_sampler, p / scale_m).xy * 2.0 - 1.0;
    return n;
}

fn noise_b(p: vec2<f32>, scale_m: f32) -> f32 {
    return textureSample(water_noise_texture, water_noise_sampler, p / scale_m).b;
}

// World position of the opaque surface behind screen point `frag` (prepass depth); `far` is true
// on sky pixels (reverse-Z depth 0).
struct Behind {
    pos: vec3<f32>,
    far: bool,
}

fn behind(frag: vec2<f32>) -> Behind {
    let d = prepass_utils::prepass_depth(vec4(frag, 0.0, 1.0), 0u);
    var b: Behind;
    b.far = d <= 1e-7;
    b.pos = vt::position_ndc_to_world(vt::frag_coord_to_ndc(vec4(frag, max(d, 1e-7), 1.0)));
    return b;
}

fn view_z(p: vec3<f32>) -> f32 {
    return vt::position_world_to_view(p).z;
}

const F0_WATER: f32 = 0.02;
// F0 = 0.16 reflectance^2.
const REFLECTANCE_WATER: f32 = 0.354;
// Beer-Lambert absorption (1/m): clear sea against a tea-green river.
const SIGMA_SEA: vec3<f32> = vec3(0.42, 0.075, 0.052);
const SIGMA_RIVER: vec3<f32> = vec3(0.38, 0.12, 0.16);
// In-scattering albedo of a fully absorbing column, lit as a diffuse layer.
const SCATTER_SEA: vec3<f32> = vec3(0.012, 0.085, 0.105);
const SCATTER_RIVER: vec3<f32> = vec3(0.045, 0.11, 0.075);
// Freeze: the ice sheet's albedo, roughness and how much of the water below it hides.
const ICE_ALBEDO: vec3<f32> = vec3(0.40, 0.55, 0.66);
const ICE_ROUGHNESS: f32 = 0.3;
const ICE_COVER: f32 = 0.7;

// Whether world point `wp` lies inside a boat hull's plan ellipse, below its top.
fn inside_hull(wp: vec3<f32>) -> bool {
    for (var i = 0u; i < min(water.boat_count, 6u); i++) {
        let a = water.hulls[2u * i];
        let b = water.hulls[2u * i + 1u];
        let d = wp.xz - a.xy;
        let x = (dot(d, a.zw) - b.w) / b.x;
        let z = dot(d, vec2(-a.w, a.z)) / b.y;
        if (x * x + z * z < 1.0 && wp.y < b.z) {
            return true;
        }
    }
    return false;
}

@fragment
fn fragment(in: WaterVarying) -> @location(0) vec4<f32> {
    let t = water.water_time;
    let river = in.site.x;
    let depth = in.site.y;
    let p = in.rest_xz;
    let wp = in.world_position.xyz;

    // Normal: the analytic Gerstner normal, plus ripples. Sea ripples drift in two incommensurate
    // directions; river ripples are advected by the flow in two phases half a period apart, each
    // fading out as it resets (two-phase flow mapping).
    let g = wc::gerstner(p, t, water.wave_count);
    let atten = wc::wave_atten(depth, river);
    let n_wave = wc::gerstner_normal(g, atten);
    let sea = ripple(p + t * vec2(0.21, -0.07), 4.3) + 0.6 * ripple(vec2(p.y, -p.x) + t * vec2(-0.13, 0.19), 1.9);
    let speed = length(in.flow);
    let period = 1.6;
    let ph0 = fract(t / period + noise_b(p, 11.0));
    let ph1 = fract(ph0 + 0.5);
    let w1 = abs(1.0 - 2.0 * ph0);
    let r0 = ripple(p - in.flow * ph0 * period, 2.3);
    let r1 = ripple(p - in.flow * ph1 * period + vec2(0.37, 0.61), 2.3);
    let riv = mix(r0, r1, w1) * (0.35 + 0.6 * speed);
    let detail = mix(sea * 0.22, riv, river);
    let skirt = in.skirt > 0.5;
    // A cut face exists only where water stands above the terrain: inside a bank it is clipped away.
    if (skirt && in.world_position.y < in.ground) {
        discard;
    }
    // Inside a boat's hull the surface would show through the floor on swell: none is drawn there.
    if (!skirt && inside_hull(wp)) {
        discard;
    }
    // On a cut face the view looks straight into the volume: no surface slope to refract along.
    let n = select(normalize(n_wave + vec3(detail.x, 0.0, detail.y)), vec3(0.0, 1.0, 0.0), skirt);

    // What lies behind the surface: the bed (or bank, or sky) from the depth prepass.
    let frag = in.position.xy;
    let b0 = behind(frag);
    var bed = b0.pos;
    var far = b0.far;
    var col_uv = vt::frag_coord_to_uv(frag);
    let wz = view_z(wp);

@if(WATER_REFRACTION) {
    // Refract: offset the lookup along the surface slope, more for thicker water; reject samples
    // that land on something in front of the water (a bank), which would smear it into the water.
    let thick0 = select(min(distance(b0.pos, wp), 2.0), 2.0, b0.far);
    let shifted = wp + vec3(n.x, 0.0, n.z) * (-0.35 * thick0);
    let clip = vb::view.clip_from_world * vec4(shifted, 1.0);
    let uv1 = vt::ndc_to_uv(clip.xy / clip.w);
    let frag1 = uv1 * vb::view.viewport.zw + vb::view.viewport.xy;
    if (all(uv1 > vec2(0.001)) && all(uv1 < vec2(0.999))) {
        let b1 = behind(frag1);
        // A shifted sample may land on sky past the patch's rim; only accept sky if it was behind.
        if ((b1.far && b0.far) || (!b1.far && view_z(b1.pos) < wz - 0.02)) {
            bed = b1.pos;
            far = b1.far;
            col_uv = uv1;
        }
    }
}

    let path = select(max(distance(bed, wp), 0.0), 60.0, far);
    // Vertical water column under the surface point (for foam and caustics).
    let column = select(max(wp.y - bed.y, 0.0), 60.0, far);
    let sigma = mix(SIGMA_SEA, SIGMA_RIVER, river);
    let trans = exp(-sigma * path);

    // Foam: shoreline contact, wash lines rolling up the beach, wave crests near pinching, and
    // fast river flow; broken up by the noise so it reads as bubbles, not a band.
    let nb = noise_b(p + t * vec2(0.05, -0.03), 3.1);
    let contact = (1.0 - smoothstep(0.0, 0.1, column)) * (1.0 - 0.75 * river);
    let lines = smoothstep(0.6, 0.95, sin(column * 7.0 - t * 2.1 + nb * 4.0)) * (1.0 - smoothstep(0.05, 1.2, column)) * (1.0 - river);
    let crest = smoothstep(0.45, 0.75, g.pinch * atten);
    let rapids = river * smoothstep(0.8, 1.3, speed) * 0.45;
    // River mouth: a band where the outflow meets the surf, surging as the swash runs in.
    let meet = select(in.ground, 0.0, skirt) * (0.5 + 0.5 * clamp(wc::swash(in.site.z, t), 0.0, 1.0));
    // River churn: white water where the flow turns or slows (the bends, the fork's meeting water),
    // none on straights. Medium and up break it up with the ripples' flow-advected two-phase noise,
    // so the streaks travel with the current; Low keeps the drifting breakup.
    var streak = nb;
    @if(WATER_REFRACTION) {
        let s0 = noise_b(p - in.flow * ph0 * period, 2.9);
        let s1 = noise_b(p - in.flow * ph1 * period + vec2(0.37, 0.61), 2.9);
        streak = mix(s0, s1, w1);
    }
    let churn = river * smoothstep(0.05, 0.22, in.churn) * (0.1 + 0.3 * streak) * (1.0 + water.churn_delta);
    let raw = max(contact, lines * 0.75) + crest + rapids + meet + churn;
    let foam = select(clamp((raw - (1.0 - nb) * 0.85) * 2.4, 0.0, 1.0), 0.0, skirt);

    // Surface lighting from Bevy's PBR: IBL reflection and the sun's specular (glints) with its
    // shadows, plus the in-scattered body as a diffuse layer and foam as a rough white one.
    var pbr = pbr_types::pbr_input_new();
    pbr.flags = mesh[in.instance_index].flags;
    pbr.is_orthographic = vb::view.clip_from_view[3].w == 1.0;
    pbr.V = pbr_functions::calculate_view(in.world_position, pbr.is_orthographic);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    pbr.world_normal = n_wave;
    pbr.N = n;
    let body = mix(SCATTER_SEA, SCATTER_RIVER, river) * (vec3(1.0) - trans);
    pbr.material.base_color = vec4(mix(body, vec3(0.82, 0.85, 0.86), foam), 1.0);
    pbr.material.perceptual_roughness = mix(0.07, 0.6, foam);
    pbr.material.metallic = 0.0;
    pbr.material.reflectance = vec3(mix(REFLECTANCE_WATER, 0.5, foam));
    // Cut face: the in-scattering volume lit from above (sun with its shadows, sky), no surface
    // reflection of its own.
    if (skirt) {
        pbr.material.base_color = vec4(mix(SCATTER_SEA, SCATTER_RIVER, river) * (vec3(1.0) - trans), 1.0);
        pbr.material.perceptual_roughness = 1.0;
        pbr.material.reflectance = vec3(0.0);
    }
    // Freeze: a flat, pale, rough sheet with a trace of the ripple detail (WATER_FREEZE only).
    @if(WATER_FREEZE) {
        let ice = water.freeze * (1.0 - in.skirt);
        pbr.N = normalize(mix(n, normalize(vec3(detail.x * 0.15, 1.0, detail.y * 0.15)), ice));
        pbr.material.base_color = vec4(mix(pbr.material.base_color.rgb, ICE_ALBEDO, ice), 1.0);
        pbr.material.perceptual_roughness = mix(pbr.material.perceptual_roughness, ICE_ROUGHNESS, ice);
        pbr.material.reflectance = mix(pbr.material.reflectance, vec3(0.5), ice);
    }

    let n_dot_v = max(dot(n, pbr.V), 1e-4);
    let fresnel = select(F0_WATER + (1.0 - F0_WATER) * pow(1.0 - n_dot_v, 5.0), 0.0, skirt);
    var hit = 0.0;
    var hit_uv = vec2(0.0);

@if(WATER_HQ) {
    // Screen-space reflection of nearby geometry (banks, beach): march the reflected ray against the
    // prepass depth; where it hits, the hit colour replaces the IBL's specular term.
    let r = reflect(-pbr.V, n);
    var s = 0.3;
    for (var i = 0; i < 20; i++) {
        let q = wp + r * s;
        let qc = vb::view.clip_from_world * vec4(q, 1.0);
        if (qc.w <= 0.0) {
            break;
        }
        let quv = vt::ndc_to_uv(qc.xy / qc.w);
        if (any(quv < vec2(0.0)) || any(quv > vec2(1.0))) {
            break;
        }
        let qb = behind(quv * vb::view.viewport.zw + vb::view.viewport.xy);
        let dz = view_z(qb.pos) - view_z(q);
        if (!qb.far && dz > 0.0 && dz < 0.6 + s * 0.08) {
            let edge = min(min(quv.x, 1.0 - quv.x), min(quv.y, 1.0 - quv.y));
            hit = smoothstep(0.0, 0.08, edge) * (1.0 - smoothstep(12.0, 24.0, s));
            hit_uv = quv;
            break;
        }
        s *= 1.22;
    }
    pbr.specular_occlusion = 1.0 - hit;
}

    var color = pbr_functions::apply_pbr_lighting(pbr).rgb;
    if (skirt) {
        // Light fades with depth below the (animated) surface; just under it, the underside of the
        // surface shows as a bright band (total internal reflection outside Snell's window).
        let surf = water.sea_level + g.disp.y * atten + water.wash_amp * wc::swash(in.site.z, t) * wc::swash_weight(depth, river);
        let below = max(surf - wp.y, 0.0);
        color *= exp(-below * 0.35) * 1.4;
        color += color * 5.0 * exp(-below * 7.0) + vec3(0.6, 0.75, 0.75) * dot(color, vec3(0.33)) * 6.0 * exp(-below * 40.0);
    }

@if(WATER_HQ) {
    if (hit > 0.0 && !skirt) {
        let refl = textureSampleLevel(vb::view_transmission_texture, vb::view_transmission_sampler, hit_uv, 0.0).rgb;
        color += refl * fresnel * hit * (1.0 - foam);
    }
}

@if(WATER_REFRACTION) {
    var under = textureSampleLevel(vb::view_transmission_texture, vb::view_transmission_sampler, col_uv, 0.0).rgb;
    // Caustics on the bed: strongest in the first metre or so, gone at the waterline and in depth.
    let fade = smoothstep(0.03, 0.4, column) * exp(-column * 0.45) * (1.0 - 0.6 * river);
    @if(WATER_HQ)
    let hq = true;
    @else
    let hq = false;
    under *= 1.0 + 2.4 * wc::caustics(bed.xz * 0.55, t, hq) * fade;
    @if(WATER_FREEZE) {
        under *= 1.0 - ICE_COVER * water.freeze * (1.0 - in.skirt);
    }
    color += under * trans * (1.0 - fresnel) * (1.0 - foam);
    return pbr_functions::main_pass_post_lighting_processing(pbr, vec4(color, 1.0));
}
@else {
    // Low tier: no transmission texture; blend over the opaque scene with the same transmittance.
    @if(WATER_FREEZE)
    let tr = dot(trans, vec3(0.3333)) * (1.0 - fresnel) * (1.0 - foam) * (1.0 - ICE_COVER * water.freeze * (1.0 - in.skirt));
    @else
    let tr = dot(trans, vec3(0.3333)) * (1.0 - fresnel) * (1.0 - foam);
    let alpha = clamp(1.0 - tr, 0.02, 1.0);
    return pbr_functions::main_pass_post_lighting_processing(pbr, vec4(color / alpha, alpha));
}
}
