//! Orbit, pan and zoom (PRD §4.5, D14) and pointer picking on the ground plane.
//!
//! The rig keeps no state of its own: each camera action derives the focus point, distance, yaw and
//! pitch from the current transform, so the `camera` script command and scripted views keep working.
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hex_core::{Axial, HEX_RADIUS};
use hex_render::{ActiveScenario, Scenario};

use crate::actions::{Action, ActionQueue};

/// Street-level close-up to far strategic (§4.5 zoom range), in metres from the focus point.
pub const MIN_DISTANCE: f32 = 12.0;
pub const MAX_DISTANCE: f32 = 2500.0;
pub(crate) const MIN_PITCH: f32 = 0.14; // about 8°
pub(crate) const MAX_PITCH: f32 = 1.55; // about 89°
const ORBIT_RAD_PER_PX: f32 = 0.005;
const ZOOM_PER_NOTCH: f32 = 0.88;
/// Focus distance used when the camera looks at or above the horizon.
const HORIZON_FOCUS: f32 = 100.0;
/// Photo free camera: mouse-look rate, the pitch limit short of straight up or down, and the
/// lowest eye height (it never flies under the ground plane).
const LOOK_RAD_PER_PX: f32 = 0.003;
const FREE_MAX_PITCH: f32 = 1.5;
const FREE_MIN_HEIGHT: f32 = 0.5;

/// Yaw (about +Y, 0 looking down -Z) and pitch of a view direction. Roll does not move the
/// forward vector, so a rolled photo camera reads back the same yaw and pitch.
pub fn yaw_pitch(forward: Vec3) -> (f32, f32) {
    let f = forward.normalize_or_zero();
    ((-f.x).atan2(-f.z), f.y.clamp(-1.0, 1.0).asin())
}

/// The photo camera's orientation: yaw, then pitch, then roll about the view axis.
pub fn free_rotation(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll)
}

/// Where the view centre meets the ground (y = 0), the point the camera orbits.
pub fn focus(t: &Transform) -> Vec3 {
    let f = t.forward();
    if f.y < -0.05 {
        t.translation + f * (-t.translation.y / f.y)
    } else {
        t.translation + f * HORIZON_FOCUS
    }
}

/// Yaw, pitch and distance of a camera offset from its focus point.
pub(crate) fn spherical(offset: Vec3) -> (f32, f32, f32) {
    let d = offset.length().max(1e-3);
    (
        offset.x.atan2(offset.z),
        (offset.y / d).clamp(-1.0, 1.0).asin(),
        d,
    )
}

/// The camera transform orbiting `focus` at yaw, pitch and distance.
pub(crate) fn place(focus: Vec3, yaw: f32, pitch: f32, dist: f32) -> Transform {
    let offset = Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    ) * dist;
    Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y)
}

/// Applies one camera action; other actions return the transform unchanged.
pub fn apply(t: Transform, action: &Action) -> Transform {
    match *action {
        Action::Fly(d) => {
            let (yaw, _) = yaw_pitch(*t.forward());
            let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
            let mut out = t;
            out.translation += right * d.x + Vec3::Y * d.y + *t.forward() * d.z;
            out.translation.y = out.translation.y.max(FREE_MIN_HEIGHT);
            return out;
        }
        Action::Look(d) => {
            let (yaw, pitch) = yaw_pitch(*t.forward());
            let mut out = t;
            out.rotation = free_rotation(
                yaw - d.x * LOOK_RAD_PER_PX,
                (pitch - d.y * LOOK_RAD_PER_PX).clamp(-FREE_MAX_PITCH, FREE_MAX_PITCH),
                0.0,
            );
            return out;
        }
        _ => {}
    }
    let c = focus(&t);
    let (yaw, pitch, dist) = spherical(t.translation - c);
    match *action {
        Action::Orbit(d) => place(
            c,
            yaw - d.x * ORBIT_RAD_PER_PX,
            (pitch + d.y * ORBIT_RAD_PER_PX).clamp(MIN_PITCH, MAX_PITCH),
            dist,
        ),
        Action::Pan(d) => {
            let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
            let right = flat(*t.right());
            // Looking straight down, "forward" on the ground is the camera's up.
            let fwd = match flat(*t.forward()) {
                v if v != Vec3::ZERO => v,
                _ => flat(*t.up()),
            };
            let mut out = t;
            out.translation += (right * d.x + fwd * d.y) * dist;
            out
        }
        Action::Zoom(n) => {
            let d = (dist * ZOOM_PER_NOTCH.powf(n)).clamp(MIN_DISTANCE, MAX_DISTANCE);
            let mut out = t;
            out.translation = c + (t.translation - c).normalize_or_zero() * d;
            out
        }
        _ => t,
    }
}

/// The cell containing ground point (x, z): inverse of `Axial::to_world` with cube rounding.
pub fn axial_at(x: f32, z: f32) -> Axial {
    let r = z / (1.5 * HEX_RADIUS);
    let q = x / (3f32.sqrt() * HEX_RADIUS) - r / 2.0;
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    Axial::new(rq as i32, rr as i32)
}

/// The cell under a view ray on the ground plane.
pub fn ground_cell(ray: Ray3d) -> Option<Axial> {
    let d = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    let p = ray.get_point(d);
    Some(axial_at(p.x, p.z))
}

/// Hovers the cell under the pointer in the play scenario. It emits only when that cell changes
/// (the pointer moved to another cell, or the camera moved under it), so a scripted `hover` stays
/// pinned while the OS cursor rests over the window.
pub fn pick_hover(
    active: Res<ActiveScenario>,
    game: Res<crate::game::GameState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut last: Local<Option<Axial>>,
    mut queue: ResMut<ActionQueue>,
) {
    if active.0 != Scenario::Play {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    // Walking, the pointer looks around rather than pointing at cells.
    let cell = (!game.walk_mode)
        .then_some(())
        .and(window.cursor_position())
        .and_then(|p| {
            let (camera, gt) = cameras.iter().next()?;
            ground_cell(camera.viewport_to_world(gt, p).ok()?)
        });
    if cell != *last {
        *last = cell;
        queue.0.push_back(Action::Hover(cell));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_core::spiral;

    #[test]
    fn axial_at_inverts_to_world() {
        for c in spiral(61) {
            let [x, z] = c.to_world(HEX_RADIUS);
            assert_eq!(axial_at(x, z), c);
            // Just inside each edge midpoint and each corner.
            for k in 0..6 {
                let a = (60.0 * k as f32).to_radians();
                let m = HEX_RADIUS * 3f32.sqrt() / 2.0 * 0.97;
                assert_eq!(axial_at(x + m * a.cos(), z + m * a.sin()), c, "edge {k}");
                let a = (30.0 + 60.0 * k as f32).to_radians();
                let m = HEX_RADIUS * 0.95;
                assert_eq!(axial_at(x + m * a.cos(), z + m * a.sin()), c, "corner {k}");
            }
        }
    }

    #[test]
    fn neighbors_are_picked_across_each_edge() {
        let c = Axial::new(2, -1);
        let [x, z] = c.to_world(HEX_RADIUS);
        for k in 0..6u8 {
            let a = (60.0 * k as f32).to_radians();
            let m = HEX_RADIUS * 3f32.sqrt() / 2.0 * 1.05;
            assert_eq!(axial_at(x + m * a.cos(), z + m * a.sin()), c.neighbor(k));
        }
    }

    #[test]
    fn ray_hits_the_ground_cell() {
        let [x, z] = Axial::new(1, 2).to_world(HEX_RADIUS);
        let ray = Ray3d::new(
            Vec3::new(x, 50.0, z + 20.0),
            Dir3::new(Vec3::new(0.0, -50.0, -20.0)).unwrap(),
        );
        assert_eq!(ground_cell(ray), Some(Axial::new(1, 2)));
        let up = Ray3d::new(Vec3::Y, Dir3::Y);
        assert_eq!(ground_cell(up), None);
    }

    fn cam() -> Transform {
        Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::new(10.0, 0.0, 0.0), Vec3::Y)
    }

    #[test]
    fn orbit_keeps_focus_and_distance() {
        let t = cam();
        let (f0, d0) = (focus(&t), (t.translation - focus(&t)).length());
        let o = apply(t, &Action::Orbit(Vec2::new(120.0, 30.0)));
        assert!((focus(&o) - f0).length() < 1e-2, "{} vs {f0}", focus(&o));
        assert!(((o.translation - f0).length() - d0).abs() < 1e-2);
        assert!(o.translation.y > 0.0);
        // Pitch is clamped below vertical and above the ground.
        let top = apply(t, &Action::Orbit(Vec2::new(0.0, 10_000.0)));
        let low = apply(t, &Action::Orbit(Vec2::new(0.0, -10_000.0)));
        assert!(top.translation.y < (top.translation - f0).length());
        assert!(low.translation.y > 0.0);
    }

    #[test]
    fn free_camera_flies_along_the_view_and_looks_without_orbiting() {
        let t = cam();
        let fwd = *t.forward();
        let f = apply(t, &Action::Fly(Vec3::new(0.0, 0.0, 5.0)));
        assert!((f.translation - (t.translation + fwd * 5.0)).length() < 1e-4);
        assert_eq!(f.rotation, t.rotation, "flying never turns the view");
        let up = apply(t, &Action::Fly(Vec3::new(0.0, 3.0, 0.0)));
        assert!((up.translation.y - t.translation.y - 3.0).abs() < 1e-4);
        let side = apply(t, &Action::Fly(Vec3::new(2.0, 0.0, 0.0)));
        assert!(
            (side.translation.y - t.translation.y).abs() < 1e-4,
            "strafe is level"
        );
        assert!(side.translation.dot(fwd) - t.translation.dot(fwd) < 1e-3);
        let floor = apply(t, &Action::Fly(Vec3::new(0.0, -1_000.0, 0.0)));
        assert_eq!(floor.translation.y, FREE_MIN_HEIGHT);

        let l = apply(t, &Action::Look(Vec2::new(100.0, 0.0)));
        assert_eq!(
            l.translation, t.translation,
            "look turns in place, unlike orbit"
        );
        let (y0, p0) = yaw_pitch(fwd);
        let (y1, p1) = yaw_pitch(*l.forward());
        assert!((y0 - y1 - 0.3).abs() < 1e-4 && (p0 - p1).abs() < 1e-4);
        let top = apply(t, &Action::Look(Vec2::new(0.0, -100_000.0)));
        assert!((yaw_pitch(*top.forward()).1 - FREE_MAX_PITCH).abs() < 1e-4);
    }

    #[test]
    fn yaw_pitch_inverts_free_rotation_under_roll() {
        for (yaw, pitch) in [(0.0, 0.0), (1.0, -0.4), (-2.5, 0.9)] {
            let q = free_rotation(yaw, pitch, 0.3);
            let (y, p) = yaw_pitch(q * Vec3::NEG_Z);
            assert!(
                (y - yaw).abs() < 1e-4 && (p - pitch).abs() < 1e-4,
                "{yaw} {pitch}"
            );
        }
    }

    #[test]
    fn pan_moves_on_the_ground_and_zoom_clamps() {
        let t = cam();
        let p = apply(t, &Action::Pan(Vec2::new(0.0, 0.1)));
        assert!(
            (p.translation.y - t.translation.y).abs() < 1e-4,
            "pan stays level"
        );
        assert!(focus(&p).z < focus(&t).z, "forward is -z here");
        let near = apply(t, &Action::Zoom(1_000.0));
        assert!(((near.translation - focus(&t)).length() - MIN_DISTANCE).abs() < 1e-2);
        let far = apply(t, &Action::Zoom(-1_000.0));
        assert!(((far.translation - focus(&t)).length() - MAX_DISTANCE).abs() < 0.5);
        assert_eq!(apply(t, &Action::Undo), t);
    }
}
