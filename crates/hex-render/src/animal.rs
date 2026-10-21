//! Farm animal motion (PRD §7.2): the walk and graze pose the kit vertex stages draw from the
//! pieces' motion data (`animal_motion.wesl`, in the forward and prepass vertex stages, so depth,
//! normals and every shadow cascade move with the forward pass).
//!
//! - **Asset side.** Every primitive of every LOD of an animal piece carries TEXCOORD_1 `motion`
//!   (`scripts/blender/kit/_animals.py` `Motion`): U is the leg weight, 0 off the legs and
//!   `(hipY - y) / hipY` on them; V is the leg code where U > 0 (0.125 front-left, 0.375
//!   front-right, 0.625 back-left, 0.875 back-right) and the head weight where U = 0 (0 on the body
//!   and tail, rising along the neck to 1 at the muzzle tip). The pivots ride in each piece's bake
//!   report `animal` block; [`RIGS`] copies them (checked by `rigs_match_the_bake_reports`).
//! - **Per slot.** `KitParams::animal_rig` = (hipY, front hip z, back hip z, swing amplitude in
//!   radians) and `KitParams::animal_head` = (neck pivot y, neck pivot z, head angle in radians at
//!   head weight 1, nose down positive, bob amplitude in radians), on slots with `FLAG_ANIMAL`.
//! - **Per instance.** The entity's `MeshTag` is the motion word ([`motion_word`]): bits 0-15 the
//!   gait phase in cycles, 16-23 the walk weight, 24-31 the head weight. 0 is the rest pose and the
//!   shader returns the vertex untouched, so static animals, the ghost, the thumbnails and merged
//!   chunks (untagged) keep the authored pose exactly. An animal slot is never a foliage slot, so the
//!   tag never reads as a season index there (`docs/kit/hooks-contract.md`).
//! - **Gait.** Diagonal pairs (bake report `gaitPhase`: front-left and back-right at 0, the others at
//!   0.5) with half the cycle in stance: the leg swings about its hip from `+A` to `-A` at an even
//!   rate, then back with an eased swing that lifts the hoof (a leg shortening toward the hoof, read
//!   as a knee). The hoof is held on the ground through the stance (the rotation's rise is taken off
//!   in proportion to U), and a cycle covers [`AnimalRig::stride`], so a hoof in stance keeps still
//!   on the ground while the body walks over it when the phase advances by distance / stride.
use bevy::math::{Vec2, Vec3, Vec4};

/// One animal piece's rig: the pivots in the piece frame (+z forward, +y up) and the motion it is
/// drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimalRig {
    /// Height of the hip pivots (m); the leg weight is 1 at the hoof, 0 at this height.
    pub hip_y: f32,
    /// z of the front and back hip pivots (m).
    pub hip_front_z: f32,
    pub hip_back_z: f32,
    /// Leg swing amplitude (rad) at walk weight 1.
    pub swing: f32,
    /// The neck pivot (y, z) the head pitches about.
    pub neck_pivot: [f32; 2],
    /// Head angle (rad) at head weight 1, nose down positive: a standing piece's dip to the grass
    /// (its bake report's `grazeDegToGround`), a grazing piece's raise (negative) from its
    /// head-down rest.
    pub head: f32,
    /// Head bob amplitude (rad) at walk weight 1, two nods a gait cycle.
    pub bob: f32,
}

/// Fraction of the hip height a swinging hoof lifts at walk weight 1.
pub const LIFT: f32 = 0.12;

/// The animal pieces' rigs.
pub const RIGS: &[(&str, AnimalRig)] = &[
    (
        "animal_cow",
        AnimalRig {
            hip_y: 1.0,
            hip_front_z: 0.55,
            hip_back_z: -0.58,
            swing: 0.2,
            neck_pivot: [0.98, 0.35],
            head: 55.5 * DEG,
            bob: 3.0 * DEG,
        },
    ),
    (
        "animal_cow_b",
        AnimalRig {
            hip_y: 1.0,
            hip_front_z: 0.55,
            hip_back_z: -0.58,
            swing: 0.2,
            neck_pivot: [0.98, 0.35],
            head: -30.0 * DEG,
            bob: 3.0 * DEG,
        },
    ),
    (
        "animal_cow_c",
        AnimalRig {
            hip_y: 1.0,
            hip_front_z: 0.55,
            hip_back_z: -0.58,
            swing: 0.2,
            neck_pivot: [0.98, 0.35],
            head: 55.5 * DEG,
            bob: 3.0 * DEG,
        },
    ),
    (
        "animal_sheep",
        AnimalRig {
            hip_y: 0.42,
            hip_front_z: 0.25,
            hip_back_z: -0.27,
            swing: 0.3,
            neck_pivot: [0.5, 0.2],
            head: 59.5 * DEG,
            bob: 4.0 * DEG,
        },
    ),
    (
        "animal_sheep_b",
        AnimalRig {
            hip_y: 0.28,
            hip_front_z: 0.15,
            hip_back_z: -0.162,
            swing: 0.3,
            neck_pivot: [0.3411, 0.116],
            head: 58.0 * DEG,
            bob: 4.0 * DEG,
        },
    ),
    (
        "animal_sheep_c",
        AnimalRig {
            hip_y: 0.42,
            hip_front_z: 0.25,
            hip_back_z: -0.27,
            swing: 0.3,
            neck_pivot: [0.5, 0.2],
            head: -22.0 * DEG,
            bob: 4.0 * DEG,
        },
    ),
];

const DEG: f32 = std::f32::consts::PI / 180.0;

/// Walk weight below which [`AnimalRig::stride`] stops shrinking, so the phase stays finite as an
/// animal slows to a stop (its swing is that small by then).
pub const STRIDE_MIN_WALK: f32 = 0.25;

/// The rig of animal piece `piece`.
pub fn rig(piece: &str) -> Option<&'static AnimalRig> {
    RIGS.iter().find(|(id, _)| *id == piece).map(|(_, r)| r)
}

/// `sin` for the small swing angles (below 0.5 rad) by its odd series to x^7: `+ - * /` alone, so
/// the herd's phase agrees on native and wasm (error below 1e-8 there).
fn small_sin(x: f32) -> f32 {
    let x2 = x * x;
    x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0)))
}

impl AnimalRig {
    /// Metres the body covers in one gait cycle at walk weight `walk`: the hoof travels
    /// `2 hipY sin(swing walk)` back along the ground in each half-cycle stance, and the body must
    /// cover the same, so the stance hoof stays put.
    pub fn stride(&self, walk: f32) -> f32 {
        4.0 * self.hip_y * small_sin(self.swing * walk.max(STRIDE_MIN_WALK))
    }

    /// `KitParams::animal_rig` and `KitParams::animal_head`.
    pub fn uniforms(&self) -> (Vec4, Vec4) {
        (
            Vec4::new(self.hip_y, self.hip_front_z, self.hip_back_z, self.swing),
            Vec4::new(self.neck_pivot[0], self.neck_pivot[1], self.head, self.bob),
        )
    }
}

/// The motion word (`MeshTag`) for gait `phase` (cycles, wrapped to [0, 1)), `walk` and `head`
/// weights (clamped to [0, 1]): 0, the rest pose, whenever both weights round to 0.
pub fn motion_word(phase: f32, walk: f32, head: f32) -> u32 {
    let w = (walk.clamp(0.0, 1.0) * 255.0).round() as u32;
    let h = (head.clamp(0.0, 1.0) * 255.0).round() as u32;
    if w == 0 && h == 0 {
        return 0;
    }
    let p = ((phase - phase.floor()) * 65536.0) as u32 & 0xffff;
    p | (w << 16) | (h << 24)
}

/// The (phase, walk, head) a motion word holds, as the shader reads it.
pub fn unpack(word: u32) -> (f32, f32, f32) {
    (
        (word & 0xffff) as f32 / 65536.0,
        ((word >> 16) & 0xff) as f32 / 255.0,
        (word >> 24) as f32 / 255.0,
    )
}

/// Gait phase offset of the leg with code `code` (bake report `gaitPhase`): the diagonal pairs.
pub fn leg_offset(code: f32) -> f32 {
    let k = (code * 4.0) as u32;
    if k == 1 || k == 2 { 0.5 } else { 0.0 }
}

/// The leg's swing in [-1, 1] (+1 hoof forward) and hoof lift in [0, 1] at leg phase `p` in [0, 1):
/// stance (p < 0.5) from +1 to -1 at an even rate, flat on the ground; swing back eased, lifted.
pub fn gait(p: f32) -> (f32, f32) {
    if p < 0.5 {
        (1.0 - 4.0 * p, 0.0)
    } else {
        let q = 2.0 * (p - 0.5);
        (
            -1.0 + 2.0 * q * q * (3.0 - 2.0 * q),
            (std::f32::consts::PI * q).sin(),
        )
    }
}

/// `p` (y, z) turned by `a` about `c` in the (z, y) plane: +a takes a point below `c` forward.
fn turn(p: Vec2, c: Vec2, a: f32) -> Vec2 {
    let (s, k) = a.sin_cos();
    let d = p - c;
    // d is (y, z): z' = z cos - y sin, y' = z sin + y cos.
    c + Vec2::new(d.y * s + d.x * k, d.y * k - d.x * s)
}

/// CPU mirror of `animal_motion.wesl` `position`: vertex `pos` with motion data `motion` posed by
/// motion word `word`.
pub fn pose_point(rig: &AnimalRig, word: u32, pos: Vec3, motion: Vec2) -> Vec3 {
    if word == 0 {
        return pos;
    }
    let (phase, walk, head) = unpack(word);
    let yz = Vec2::new(pos.y, pos.z);
    if motion.x > 0.0 {
        let front = motion.y < 0.5;
        let hip = Vec2::new(
            rig.hip_y,
            if front {
                rig.hip_front_z
            } else {
                rig.hip_back_z
            },
        );
        let lp = (phase + leg_offset(motion.y)).fract();
        let (s, lift) = gait(lp);
        let a = rig.swing * walk * s;
        let mut q = turn(yz, hip, a);
        q.x -= rig.hip_y * (1.0 - a.cos()) * motion.x;
        q.x += LIFT * rig.hip_y * walk * lift * motion.x;
        return Vec3::new(pos.x, q.x, q.y);
    }
    if motion.y > 0.0 {
        let bob = rig.bob * walk * (std::f32::consts::TAU * 2.0 * phase).sin();
        let a = -(head * rig.head + bob) * motion.y;
        let q = turn(yz, Vec2::from(rig.neck_pivot), a);
        return Vec3::new(pos.x, q.x, q.y);
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORTS: &[(&str, &str)] = &[
        (
            "animal_cow",
            include_str!("../../../assets-source/kit/animal_cow/bake-report.json"),
        ),
        (
            "animal_cow_b",
            include_str!("../../../assets-source/kit/animal_cow_b/bake-report.json"),
        ),
        (
            "animal_cow_c",
            include_str!("../../../assets-source/kit/animal_cow_c/bake-report.json"),
        ),
        (
            "animal_sheep",
            include_str!("../../../assets-source/kit/animal_sheep/bake-report.json"),
        ),
        (
            "animal_sheep_b",
            include_str!("../../../assets-source/kit/animal_sheep_b/bake-report.json"),
        ),
        (
            "animal_sheep_c",
            include_str!("../../../assets-source/kit/animal_sheep_c/bake-report.json"),
        ),
    ];

    /// Each rig's pivots are its bake report's (`hipY`, `hips`, `neckPivot`), its gait the report's
    /// diagonal pairs, and its head angle the report's graze: a standing piece dips by
    /// `grazeDegToGround`; a piece already grazing (0 there) raises its head instead.
    #[test]
    fn rigs_match_the_bake_reports() {
        assert_eq!(RIGS.len(), REPORTS.len());
        for (piece, text) in REPORTS {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let a = &v["animal"];
            let r = rig(piece).unwrap();
            let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
            assert_eq!(r.hip_y, f(&a["hipY"]), "{piece}");
            for (leg, code) in a["legCodes"].as_object().unwrap() {
                let hip = &a["hips"][leg];
                assert_eq!(f(&hip[1]), r.hip_y, "{piece} {leg}");
                let z = if leg.starts_with("leg_f") {
                    r.hip_front_z
                } else {
                    r.hip_back_z
                };
                assert_eq!(f(&hip[2]), z, "{piece} {leg}");
                assert_eq!(
                    leg_offset(f(code)),
                    f(&a["gaitPhase"][leg]),
                    "{piece} {leg}"
                );
            }
            let pivot = &a["neckPivot"];
            assert_eq!(r.neck_pivot, [f(&pivot[1]), f(&pivot[2])], "{piece}");
            let graze = f(&a["grazeDegToGround"]);
            if graze > 0.0 {
                assert!((r.head - graze * DEG).abs() < 1e-6, "{piece}");
            } else {
                assert!(r.head < 0.0, "{piece}: a grazing piece raises its head");
            }
        }
    }

    #[test]
    fn the_motion_word_is_zero_at_rest_and_round_trips() {
        assert_eq!(motion_word(0.37, 0.0, 0.0), 0);
        assert_eq!(motion_word(0.9, 0.001, 0.0), 0);
        let w = motion_word(1.25, 0.5, 1.0);
        let (p, walk, head) = unpack(w);
        assert!((p - 0.25).abs() < 1e-4);
        assert!((walk - 0.5).abs() < 0.003);
        assert_eq!(head, 1.0);
        assert_eq!(unpack(motion_word(-0.25, 1.0, 0.0)).0, 0.75);
    }

    /// Rest is exact: word 0 returns every vertex as it is.
    #[test]
    fn word_zero_is_the_authored_pose() {
        let r = rig("animal_cow").unwrap();
        for (p, m) in [
            (Vec3::new(0.18, 0.0, 0.55), Vec2::new(1.0, 0.125)),
            (Vec3::new(0.0, 0.95, 1.425), Vec2::new(0.0, 1.0)),
            (Vec3::new(0.1, 1.2, 0.0), Vec2::ZERO),
        ] {
            assert_eq!(pose_point(r, 0, p, m), p);
        }
    }

    /// Walking at a steady walk weight 1 with the phase advanced by distance / stride, each hoof in
    /// stance stays put on the ground (within 1.5 cm of travel and 2 mm of height over its stance)
    /// while the body walks a stride, and lifts in its swing.
    #[test]
    fn a_stance_hoof_does_not_skate() {
        for (piece, r) in RIGS {
            let stride = r.stride(1.0);
            for (code, z) in [
                (0.125, r.hip_front_z),
                (0.375, r.hip_front_z),
                (0.625, r.hip_back_z),
                (0.875, r.hip_back_z),
            ] {
                let hoof = Vec3::new(0.0, 0.0, z);
                let mut stance: Vec<Vec3> = vec![];
                let mut lifted = 0.0f32;
                let steps = 400;
                for i in 0..steps {
                    let body = stride * i as f32 / steps as f32;
                    let phase = body / stride;
                    let p = pose_point(r, motion_word(phase, 1.0, 0.0), hoof, Vec2::new(1.0, code));
                    let world = p + Vec3::Z * body;
                    if (phase + leg_offset(code)).fract() < 0.5 {
                        stance.push(world);
                    } else {
                        lifted = lifted.max(p.y);
                    }
                }
                let (lo, hi) = stance.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    (lo.min(p.z), hi.max(p.z))
                });
                assert!(hi - lo < 0.015, "{piece} {code}: hoof skates {} m", hi - lo);
                for p in &stance {
                    assert!(
                        p.y.abs() < 0.002,
                        "{piece} {code}: stance hoof at y {}",
                        p.y
                    );
                }
                assert!(
                    lifted > 0.08 * r.hip_y,
                    "{piece} {code}: swing lift {lifted}"
                );
            }
        }
    }

    /// Head weight 1 takes a standing piece's muzzle down to the grass (its bake report's 0.08 m
    /// target) and a bob at walk weight 0 is none.
    #[test]
    fn a_standing_piece_dips_its_muzzle_to_the_grass() {
        for (piece, text) in REPORTS {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let a = &v["animal"];
            if a["grazeDegToGround"].as_f64().unwrap() == 0.0 {
                continue;
            }
            let tip = &a["muzzleTip"];
            let tip = Vec3::new(
                0.0,
                tip[1].as_f64().unwrap() as f32,
                tip[2].as_f64().unwrap() as f32,
            );
            let r = rig(piece).unwrap();
            let down = pose_point(r, motion_word(0.0, 0.0, 1.0), tip, Vec2::new(0.0, 1.0));
            assert!(
                (down.y - 0.08).abs() < 0.02,
                "{piece}: muzzle at {}",
                down.y
            );
        }
    }

    #[test]
    fn the_shader_constants_match() {
        let src = include_str!("animal_motion.wesl");
        assert!(src.contains(&format!("const LIFT: f32 = {LIFT};")));
        let common = include_str!("kit_common.wesl");
        assert!(common.contains(&format!(
            "const FLAG_ANIMAL: u32 = {}u;",
            crate::kit_material::FLAG_ANIMAL
        )));
    }
}
