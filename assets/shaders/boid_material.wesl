// Boid kit pieces (crates/hex-play/src/boids.rs): StandardMaterial + BoidExt (bindings 100-103).
// The mesh holds one copy of a piece's LOD0 per boid slot the form may draw (a bird species,
// bird_starling or bird_goldfinch, over its own flocks; a fish species, fish_trout or fish_mackerel,
// over every fish slot, drawing only the fish of its spawn water); this vertex shader maps copy
// k to its slot (boids::FormSlots::slot), reads that boid from the
// buffer the compute pass writes (boids_compute.wgsl), poses the piece from TEXCOORD_1 by its
// contract (the GLB root extras `boid`), scales it to its display size and places it there. The
// fragment shades it like a kit piece: StandardMaterial's maps at UV0, `masks` AO in A.
import bevy_pbr::render::{
    mesh_bindings::mesh,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
};

struct Boid { pos: vec4<f32>, vel: vec4<f32>, anim: vec4<f32> };

// Mirrors `BoidForm` (boids.rs).
struct BoidForm {
    kind: u32,
    base: u32,
    boid_verts: u32,
    flags: u32,
    scale: f32,
    lift: f32,
    wag: f32,
    fold_k: f32,
    hinge: vec2<f32>,
    fold_roll: f32,
    fold_back: f32,
    flock: u32,
    stride: u32,
    offset: u32,
}

const FORM_FISH: u32 = 1u;
const FLAG_MASKS: u32 = 1u;
// boids::FLAG_RIVER_FISH: a fish form that draws the river-palette fish (fish_trout); without it a
// fish form draws the ocean-palette ones (fish_mackerel). boids::fish_form_draws mirrors the test.
const FLAG_RIVER_FISH: u32 = 2u;
// boids::RIVER_U: the river palette's half of the fish atlas. RIVER_LIFT brings its mean linear
// luminance (0.29) to the ocean palette's (0.45).
const RIVER_U: f32 = 0.5;
const RIVER_LIFT: f32 = 1.5;
// SEA_LIFT: the sea fish's (fish_mackerel) dark dorsal palette, the side the camera sees
// (linear luminance 0.077 over its up-facing faces), lifted the same way (owner A12-Q3, 2026-10-20:
// it read as a dark navy silhouette under the sea).
const SEA_LIFT: f32 = 2.0;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> form: BoidForm;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var boid_masks_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var boid_masks_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var<storage, read> boids: array<Boid>;

// vel.w, as boids_compute.wgsl writes it.
const SETTLE: f32 = 1.0;
const PERCH: f32 = 2.0;
const SWIM: f32 = 3.0;
const IDLE: f32 = 4.0;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
    @location(4) tangent: vec4<f32>,
};

// A wing point or direction in the mirrored (outward, up) plane of its side, rotated by `a`.
fn turn(d: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(d.x * c - d.y * s, d.x * s + d.y * c);
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // vertex_index includes the mesh's offset in Bevy's shared vertex slab.
    let local = v.vertex_index - mesh[v.instance_index].first_vertex_index;
    // A bird species' copies run over its own flocks' slots (stride flocks apart from the offset
    // flock); the fish's are every slot from base (flock, stride 1, offset 0). boids::FormSlots::slot.
    let copy = local / form.boid_verts;
    let b = boids[form.base + ((copy / form.flock) * form.stride + form.offset) * form.flock + copy % form.flock];
    let mode = b.vel.w;
    let fish_form = form.kind == FORM_FISH;
    // A fish's species follows its spawn water, as its palette offset (anim.z) does.
    let river_fish = b.anim.z > RIVER_U * 0.5;
    let other_fish = fish_form && river_fish != ((form.flags & FLAG_RIVER_FISH) != 0u);
    if (mode > IDLE - 0.5 || fish_form != (mode > SWIM - 0.5) || other_fish) {
        // An idle slot (boids_compute.wgsl), or a slot another form draws: every vertex on one
        // point, so nothing rasterizes.
        out.position = position_world_to_clip(b.pos.xyz);
        return out;
    }
    var p = v.position;
    var n = v.normal;
    var t = v.tangent.xyz;
    let w = v.uv_b.x;
    var lift = 0.0;
    if (fish_form) {
        // Tail wag: x += U^2 * amplitude * sin(phase).
        p.x += w * w * form.wag * sin(b.anim.x);
    } else if (w > 0.0) {
        // Wings: offset from the shoulder hinge line in the side's mirrored (outward, up) plane.
        let side = select(-1.0, 1.0, p.x >= 0.0);
        var d = vec2<f32>(abs(p.x) - form.hinge.x, p.y - form.hinge.y);
        var dn = vec2<f32>(n.x * side, n.y);
        var dt = vec2<f32>(t.x * side, t.y);
        var a = 0.0;
        if (mode > PERCH - 0.5) {
            // Perch fold: span scaled toward the shoulder, rolled down onto the flank, slid back.
            d *= form.fold_k;
            a = form.fold_roll;
            p.z -= form.fold_back * w;
        } else {
            // Flap about the hinge: a slight dihedral while gliding, beats while flapping.
            a = 0.12 + b.anim.y * 0.72 * sin(b.anim.x);
        }
        d = turn(d, a);
        dn = turn(dn, a);
        dt = turn(dt, a);
        p = vec3<f32>(side * (form.hinge.x + d.x), form.hinge.y + d.y, p.z);
        n = vec3<f32>(side * dn.x, dn.y, n.z);
        t = vec3<f32>(side * dt.x, dt.y, t.z);
    }
    if (!fish_form && mode > SETTLE - 0.5) {
        // A perched (or settling) boid sits at its roost point: lift the pivot over the feet.
        lift = form.lift;
    }
    var fwd = vec3<f32>(0.0, 0.0, 1.0);
    if (length(b.vel.xyz) > 1e-6) { fwd = normalize(b.vel.xyz); }
    // Keep pitch gentle so a climbing bird does not stand on its tail.
    fwd = normalize(vec3<f32>(fwd.x, clamp(fwd.y, -0.5, 0.5), fwd.z) + vec3<f32>(1e-5, 0.0, 0.0));
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), fwd));
    let up = cross(fwd, right);
    // A river fish's growth (boids_compute.wgsl anim.w, plan::RIVER_GROW).
    let s = p * form.scale * (1.0 + select(0.0, b.anim.w, fish_form));
    let world = b.pos.xyz + vec3<f32>(0.0, lift, 0.0) + right * s.x + up * s.y + fwd * s.z;
    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    out.world_normal = normalize(right * n.x + up * n.y + fwd * n.z);
    @if(VERTEX_UVS_A)
    out.uv = v.uv + vec2<f32>(b.anim.z, 0.0);
    @if(VERTEX_UVS_B)
    out.uv_b = v.uv_b;
    @if(VERTEX_TANGENTS)
    out.world_tangent = vec4<f32>(normalize(right * t.x + up * t.y + fwd * t.z), v.tangent.w);
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = v.instance_index;
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    @if(VERTEX_UVS_A) {
    if ((form.flags & FLAG_MASKS) != 0u) {
        pbr_input.diffuse_occlusion *= textureSample(boid_masks_texture, boid_masks_sampler, in.uv).a;
    }
    }
    @if(VERTEX_UVS_A) {
    if (form.kind == FORM_FISH) {
        // The river palette under murky river water (owner, 2026-10-14: river fish were not
        // seen): lifted toward the ocean palette's brightness, hue kept. The sea fish's dark back
        // under the sea's water column likewise (SEA_LIFT).
        let lift = select(SEA_LIFT, RIVER_LIFT, (form.flags & FLAG_RIVER_FISH) != 0u);
        let c = pbr_input.material.base_color.rgb;
        pbr_input.material.base_color = vec4<f32>(min(c * lift, vec3<f32>(1.0)), pbr_input.material.base_color.a);
    }
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
