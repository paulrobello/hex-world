//! Production kit pieces' shared runtime: the compiled-in manifest, the edge-to-yaw turn every tile
//! piece uses, and the windmill rotor's spin. The showcase layout is `kit_scene` (`gate-tools`).

use bevy::prelude::*;

/// The kit manifest, compiled in.
pub const MANIFEST: &str = include_str!("../../../assets/kit/manifest.json");

pub fn piece_rotation(rot: u8) -> Quat {
    // Edges run clockwise viewed from +Y: +x -> +z is a negative rotation about +Y.
    Quat::from_rotation_y(-(60.0 * (rot % 6) as f32).to_radians())
}

/// Rotor speed (PRD §7.2: windmill sails turn as a rigid transform): a steady, slow 4 rpm.
pub const ROTOR_RPM: f64 = 4.0;

/// A spinning part of a kit instance: the rotor node's rest transform in its parent's frame (the
/// world here: placement, then the node's own transform; in play the hero's body part, so the node's
/// own transform) and its phase. It turns about its local +X axis.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct KitRotor {
    pub base: Transform,
    pub phase: f32,
}

/// The rotor angle (radians, in [0, 2π)) at `elapsed` seconds of the app clock: `--clock fixed`
/// steps that clock by exactly 1/60 s a frame, so a shot at frame N always sees the same angle.
pub fn rotor_angle(elapsed: f64, phase: f32) -> f32 {
    let w = ROTOR_RPM * std::f64::consts::TAU / 60.0;
    (elapsed * w + f64::from(phase)).rem_euclid(std::f64::consts::TAU) as f32
}

/// A per-instance phase from where the rotor stands (its hub, to the centimetre), so neighbouring
/// windmills do not turn in step. Integer hashing keeps native and web identical.
pub fn rotor_phase(at: Vec3) -> f32 {
    let q = |v: f32| (v * 100.0).round() as i32 as u32;
    let mut h = q(at.x).wrapping_mul(0x9E37_79B1) ^ q(at.z).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

pub fn spin_rotors(time: Res<Time>, mut rotors: Query<(&KitRotor, &mut Transform)>) {
    let t = time.elapsed_secs_f64();
    for (r, mut tf) in &mut rotors {
        *tf = r.base * Transform::from_rotation(Quat::from_rotation_x(rotor_angle(t, r.phase)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_maps_edge0_to_edge_k() {
        // Edge k's outward direction in XZ is at 60°·k (axial.rs: edge 1 = +z).
        for k in 0..6u8 {
            let v = piece_rotation(k) * Vec3::X;
            let a = (60.0 * k as f32).to_radians();
            assert!(
                (v - Vec3::new(a.cos(), 0.0, a.sin())).length() < 1e-5,
                "k={k} {v}"
            );
        }
    }

    #[test]
    fn rotor_turns_steadily_on_the_app_clock() {
        use std::f32::consts::{FRAC_PI_4, TAU};
        let period = 60.0 / ROTOR_RPM;
        assert!(rotor_angle(0.0, 0.0).abs() < 1e-6);
        assert!((rotor_angle(period / 8.0, 0.0) - FRAC_PI_4).abs() < 1e-5);
        let a = rotor_angle(period * 1000.0 + 1.0, 0.5);
        assert!(
            (a - rotor_angle(1.0, 0.5)).abs() < 1e-4,
            "no drift over long runs"
        );
        // 60 fixed frames (1 s) at 4 rpm turn the sails 24°.
        assert!((rotor_angle(1.0, 0.0) - 24f32.to_radians()).abs() < 1e-5);
        let p = rotor_phase(Vec3::new(69.3, 12.0, -24.0));
        assert!((0.0..TAU).contains(&p));
        assert_eq!(p, rotor_phase(Vec3::new(69.3, 12.0, -24.0)));
        assert_ne!(p, rotor_phase(Vec3::new(41.6, 12.0, -24.0)));
    }
}
