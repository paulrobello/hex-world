// `hex_render::foliage_wind`: kit foliage sway (PRD §9.1), shared by assets/shaders/kit_vertex.wesl
// (forward) and kit_prepass.wesl (depth, normal, motion and every shadow cascade), so each pass moves
// a vertex with the same expressions. Embedded like kit_common; declares no bindings and samples
// nothing. The push comes from hex_render::wind, the field the grass bends in, so grass, reeds,
// flowers and trees move with the same gusts.
//
// Inputs per vertex: its rest world position, the instance origin (the piece pivot: a tree's trunk
// base) and TEXCOORD_1 `wind` (U sway weight in [0, 1], V phase in [0, 1]; project.md "Tree wind
// data"); per slot, `amp`, metres of displacement per unit of wind push (`Slot::wind`).
// - Tree: the trunk leans from its base by (height / 10 m)², the field sampled at the trunk base so
//   the crown moves as one, plus a slow natural sway; branches flex by U² on top; outer cards
//   (U above 0.6) flutter on their own phase V.
// - Plant: bends by U² (U = 0 at each plant's own base), the field sampled at the vertex, plus a
//   faster sway on the plant's phase V.
// The instance origin hashes to a phase offset so neighbours never sway in lockstep. The offset is
// horizontal except the cards' flutter, and exactly zero at a plant's base (U = 0) and at a tree's
// trunk base and roots (height 0 or below, U = 0).

import hex_render::wind;

const TAU: f32 = 6.2831853;
// Tree: lean reference height (m), natural sway rate (Hz) and depth (fraction of the push, scaled by
// the wind's own strength), branch flex (fraction of the lean amplitude at U = 1), card flutter.
const TREE_REF_M: f32 = 10.0;
const TREE_SWAY_HZ: f32 = 0.27;
const TREE_SWAY: f32 = 0.35;
const TREE_FLEX: f32 = 0.35;
const TREE_FLEX_HZ: f32 = 0.55;
const TREE_FLUTTER_HZ: f32 = 1.9;
const TREE_FLUTTER_M: f32 = 0.05;
// Plant: sway rate (Hz) and depth.
const PLANT_SWAY_HZ: f32 = 0.8;
const PLANT_SWAY: f32 = 0.45;

// Phase offset in [0, 1) of an instance, from its origin on a 0.5 m lattice.
fn instance_phase(origin: vec3<f32>) -> f32 {
    return wind::lattice(vec2<i32>(floor(origin.xz * 2.0)), 0x5eedu);
}

// The push with a sway of depth `depth` along it at phase `ph` (cycles), grown with the push itself.
fn swayed(push: vec2<f32>, ph: f32, depth: f32) -> vec2<f32> {
    let strength = length(push);
    let dir = push / max(strength, 1e-3);
    return push + dir * (sin(TAU * ph) * depth * (0.12 + strength));
}

// World-space offset of a vertex at rest position `world` (see the header).
fn offset(world: vec3<f32>, origin: vec3<f32>, sway: vec2<f32>, amp: f32, tree: bool, t: f32) -> vec3<f32> {
    let u = clamp(sway.x, 0.0, 1.0);
    let ip = instance_phase(origin);
    if (tree) {
        let w = wind::sample(origin.xz, t);
        let h = max(world.y - origin.y, 0.0) / TREE_REF_M;
        let lean = swayed(w.push, TREE_SWAY_HZ * t + ip, TREE_SWAY) * (amp * h * h);
        let flex = swayed(w.push, TREE_FLEX_HZ * t + ip + sway.y, TREE_SWAY) * (amp * TREE_FLEX * u * u);
        let card = smoothstep(0.6, 1.0, u) * TREE_FLUTTER_M * (0.3 + w.gust)
            * sin(TAU * (TREE_FLUTTER_HZ * t + sway.y + ip));
        let across = vec2(-w.push.y, w.push.x) / max(length(w.push), 1e-3);
        let d = lean + flex + across * card;
        return vec3(d.x, 0.6 * card, d.y);
    }
    let w = wind::sample(world.xz, t);
    let d = swayed(w.push, PLANT_SWAY_HZ * t + ip + sway.y, PLANT_SWAY) * (amp * u * u);
    return vec3(d.x, 0.0, d.y);
}
