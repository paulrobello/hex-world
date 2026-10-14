// In-world affordances (crates/hex-render/src/play/affordances.rs, design/ui/legend/in-world.md).
// No textures: every width, dash, offset and hatch is in screen pixels, so lines hold their width
// at every zoom.
//
// Line and tick meshes are one quad per segment: POSITION is endpoint a, COLOR.xyz endpoint b,
// UV_0 (t, side) picks the corner (t: 0 at a, 1 at b; side: -1 or +1). The vertex shader projects
// both endpoints, takes the screen-space normal and pushes the corner out by the half-extent in
// pixels (core plus halo), converting back to clip space with the vertex's own w.
// Fill meshes are plain triangles at their world position.
import bevy_pbr::render::{
    forward_io::{Vertex, decompress_vertex},
    mesh_functions,
    mesh_view_bindings as vb,
    view_transformations as vt,
};

struct Params {
    // Linear rgb; a = opacity.
    color: vec4<f32>,
    // Core width px, dash on px (tick: stroke length px), dash off px, normal offset px (+ = right
    // of a->b on screen, the inside of a hex edge running clockwise on screen).
    shape: vec4<f32>,
    // Halo sigma 1 px, alpha 1, sigma 2 px, alpha 2.
    halo: vec4<f32>,
    // Core emissive multiplier, mode (0 line, 1 fill, 2 tick), px per CSS px, hatch spacing px
    // (fill) or tick angle rad (tick).
    misc: vec4<f32>,
    // Hatch stripe linear rgb and alpha (fill); stripe width is shape.x.
    hatch: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> p: Params;

struct Out {
    @builtin(position) clip: vec4<f32>,
    // Pixels from a along the segment, and from the centre line across it.
    @location(0) @interpolate(linear) along: f32,
    @location(1) @interpolate(linear) across: f32,
    @location(2) @interpolate(flat) len: f32,
}

fn extent(scale: f32) -> f32 {
    var e = 0.5 * p.shape.x * scale + 1.0;
    if (p.halo.y > 0.0) { e = max(e, 0.5 * p.shape.x * scale + 3.0 * p.halo.x * scale); }
    if (p.halo.w > 0.0) { e = max(e, 0.5 * p.shape.x * scale + 3.0 * p.halo.z * scale); }
    return e;
}

@vertex
fn vertex(v: Vertex) -> Out {
    let u = decompress_vertex(v, v.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let wa = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(u.position, 1.0)).xyz;
    var out: Out;
    let mode = p.misc.y;
    if (mode > 0.5 && mode < 1.5) {
        out.clip = vt::position_world_to_clip(wa);
        out.along = 0.0;
        out.across = 0.0;
        out.len = 0.0;
        return out;
    }
    let wb = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(u.color.xyz, 1.0)).xyz;
    let scale = p.misc.z;
    let ca = vt::position_world_to_clip(wa);
    let cb = vt::position_world_to_clip(wb);
    let half_vp = vb::view.viewport.zw * 0.5;
    var sa = ca.xy / max(ca.w, 1e-4) * half_vp;
    var sb = cb.xy / max(cb.w, 1e-4) * half_vp;
    var d = sb - sa;
    var dir = select(vec2(1.0, 0.0), normalize(d), dot(d, d) > 1e-8);
    var other = cb;
    if (mode > 1.5) {
        // Tick: a fixed-length stroke centred on a, turned from the segment's direction.
        let ang = p.misc.w;
        dir = vec2(dir.x * cos(ang) - dir.y * sin(ang), dir.x * sin(ang) + dir.y * cos(ang));
        let half_len = 0.5 * p.shape.y * scale;
        sb = sa + dir * half_len;
        sa = sa - dir * half_len;
        other = ca;
    }
    let len = length(sb - sa);
    // Screen-space normal, right of a->b (y up), and the inward offset of the centre line.
    let n = vec2(dir.y, -dir.x);
    let off = n * p.shape.w * scale;
    let e = extent(scale);
    let t = u.uv.x;
    let side = u.uv.y;
    let end = select(sa, sb, t > 0.5);
    let along = select(-e, len + e, t > 0.5);
    let px = end + dir * select(-e, e, t > 0.5) + n * side * e + off;
    // Back to clip space at the endpoint's own depth and w.
    let c = select(ca, other, t > 0.5);
    out.clip = vec4(px / half_vp * c.w, c.z, c.w);
    out.along = along;
    out.across = side * e;
    out.len = len;
    return out;
}

fn halo(d: f32, scale: f32) -> f32 {
    var h = 0.0;
    if (p.halo.y > 0.0) {
        let s = p.halo.x * scale;
        h += p.halo.y * exp(-d * d / (2.0 * s * s));
    }
    if (p.halo.w > 0.0) {
        let s = p.halo.z * scale;
        h += p.halo.w * exp(-d * d / (2.0 * s * s));
    }
    return min(h, 1.0);
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let scale = p.misc.z;
    let mode = p.misc.y;
    if (mode > 0.5 && mode < 1.5) {
        // Fill, with optional 45-degree screen-space hatch stripes composited over it.
        let spacing = p.misc.w * scale;
        if (spacing <= 0.0) {
            return p.color;
        }
        let sp = (in.clip.x + in.clip.y) * 0.70710678;
        let m = sp - floor(sp / spacing) * spacing;
        let w = p.shape.x * scale;
        let s = clamp(w * 0.5 + 0.5 - abs(m - w * 0.5), 0.0, 1.0) * p.hatch.a;
        let a = s + p.color.a * (1.0 - s);
        let rgb = (p.hatch.rgb * s + p.color.rgb * p.color.a * (1.0 - s)) / max(a, 1e-4);
        return vec4(rgb, a);
    }
    // Distance to the segment (round ends), then core coverage and the halo falloff.
    let along_c = clamp(in.along, 0.0, in.len);
    let dist = length(vec2(in.along - along_c, in.across));
    let half_w = 0.5 * p.shape.x * scale;
    var core = clamp(half_w - dist + 0.5, 0.0, 1.0);
    var h = halo(max(dist - half_w, 0.0), scale);
    let on = p.shape.y * scale;
    let off = p.shape.z * scale;
    if (mode < 0.5 && on > 0.0 && off > 0.0) {
        // Dashes centred on the segment, so both corners of a hex edge end the same way.
        let period = on + off;
        let phase = in.along - 0.5 * in.len + 0.5 * on;
        let m = phase - floor(phase / period) * period;
        let dash = clamp(min(m, on - m) + 0.5, 0.0, 1.0);
        core *= dash;
        h *= mix(0.25, 1.0, dash);
    }
    let a = max(core, h) * p.color.a;
    if (a <= 0.002) {
        discard;
    }
    let rgb = p.color.rgb * mix(1.0, p.misc.x, core);
    return vec4(rgb, a);
}
