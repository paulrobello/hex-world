//! The input-action layer (PRD §4.5): keyboard and mouse map to [`Action`]s, script and bridge
//! commands enqueue the same actions, and only actions change the camera or the game. Touch (P7)
//! and a first-person controller (P8) add bindings here without touching the game.
use std::collections::VecDeque;

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use hex_core::{Axial, RotateDir};
use serde::{Deserialize, Serialize};

/// The player's tool: place tiles or bulldoze them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Place,
    Bulldoze,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Orbit around the focus point by a screen-space drag in pixels.
    Orbit(Vec2),
    /// Pan in fractions of the view distance: x to the right, y forward along the ground.
    Pan(Vec2),
    /// Wheel notches, positive zooms in.
    Zoom(f32),
    /// Photo mode's free camera: metres along the view's horizontal right (x), world up (y) and
    /// the view direction (z).
    Fly(Vec3),
    /// Photo mode's mouse look by a drag in pixels (x turns right, y tilts down).
    Look(Vec2),
    /// Left-click: apply the current tool (place or bulldoze) at the hovered cell.
    Primary,
    /// Shift+click: place the selected tile on the hovered slot at once, at the ghost's rotation
    /// for that slot, with no pin step. A pin on another slot stays. Anywhere else, in bulldoze
    /// mode or with nothing hovered it is a plain [`Action::Primary`].
    QuickPlace,
    /// Ctrl or Cmd+click: [`Action::Replace`] at the hovered cell.
    QuickReplace,
    /// Swap the placed tile at a cell for the selected tile, at the ghost's rotation when that
    /// fits, else the first rotation that fits. On a free slot it places as [`Action::QuickPlace`].
    Replace(Axial),
    /// Place the selected tile at a cell. A cell other than the hovered one is hovered first.
    /// Clears any pinned slot first, so the rotation is the one legal at this cell.
    Place(Axial),
    /// Pin a free slot: the ghost and the inventory stay on it while the pointer moves.
    Pin(Axial),
    /// Clear the pinned slot.
    Unpin,
    /// Esc outside photo mode: unpins a pinned slot, else leaves photo mode (nothing otherwise).
    Escape,
    Bulldoze(Axial),
    /// Pointer or script hover; `None` hovers nothing.
    Hover(Option<Axial>),
    /// Cycle the ghost rotation: `Forward` is clockwise viewed from above (R), `Back` is Shift+R.
    Rotate(RotateDir),
    Undo,
    Redo,
    ToggleBulldoze,
    SetTool(Tool),
    ToggleInventory,
    TogglePhoto,
    /// Esc in photo mode: leaves it; does nothing outside it.
    ExitPhoto,
    SelectTile(String),
    /// Select the n-th inventory tile (catalog order), from the number-key binding.
    SelectNth(usize),
    /// F: enter or leave first-person walk mode.
    ToggleWalk,
    /// Enter (`true`) or leave walk mode; the `walk` command and the HUD button.
    SetWalk(bool),
    /// Esc in walk mode: leaves it; does nothing outside it.
    ExitWalk,
    /// Walk mode: ground offset in metres, x to the walker's right and y forward.
    Stride(Vec2),
    /// Walk mode's mouse look in pixels (x turns right, y tilts down).
    WalkLook(Vec2),
    /// Walk mode: Space hops.
    Hop,
}

impl Action {
    /// Camera actions move the view; every other action goes to the game.
    pub fn is_camera(&self) -> bool {
        matches!(
            self,
            Action::Orbit(_) | Action::Pan(_) | Action::Zoom(_) | Action::Fly(_) | Action::Look(_)
        )
    }

    /// Walk actions move the walker (`walk::sync`); they never reach the game or the orbit camera.
    pub fn is_walk(&self) -> bool {
        matches!(self, Action::Stride(_) | Action::WalkLook(_) | Action::Hop)
    }
}

/// Actions waiting for this frame's `apply_actions`, from input, picking and commands.
#[derive(Resource, Default)]
pub struct ActionQueue(pub VecDeque<Action>);

/// One frame of raw input, decoupled from Bevy so the binding map is unit-testable.
#[derive(Clone, Debug, Default)]
pub struct InputFrame {
    pub just_pressed: Vec<KeyCode>,
    pub held: Vec<KeyCode>,
    pub mouse_just_pressed: Vec<MouseButton>,
    pub mouse_held: Vec<MouseButton>,
    /// Pointer motion in pixels this frame.
    pub motion: Vec2,
    /// Vertical wheel travel this frame in notches (lines), positive away from the user.
    pub wheel: f32,
    pub dt: f32,
}

impl InputFrame {
    fn held_any(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.held.contains(k))
    }

    fn pressed(&self, k: KeyCode) -> bool {
        self.just_pressed.contains(&k)
    }

    /// Ctrl on every platform, Cmd on macOS.
    fn ctrl_or_cmd(&self) -> bool {
        self.held_any(&[
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ])
    }

    fn shift(&self) -> bool {
        self.held_any(&[KeyCode::ShiftLeft, KeyCode::ShiftRight])
    }
}

/// Mouse orbit is in pixels; pan converts pixels to view-distance fractions.
const DRAG_PAN_PER_PX: f32 = 0.0012;
/// Keyboard pan speed in view distances per second.
const KEY_PAN_SPEED: f32 = 0.8;
/// Pixel-unit scroll (trackpads, browsers) per wheel notch.
pub const PIXELS_PER_NOTCH: f32 = 40.0;
/// Photo free-camera speed in metres per second (WASD, Q/E); Shift flies four times faster.
pub const FLY_SPEED: f32 = 12.0;
/// Photo free-camera dolly per wheel notch, in metres.
const FLY_PER_NOTCH: f32 = 2.0;

/// Number keys 1..9, 0, -, = select inventory tiles 1..12 (a provisional binding, not in §4.5).
const SELECT_KEYS: [KeyCode; 12] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::Digit0,
    KeyCode::Minus,
    KeyCode::Equal,
];

/// Photo mode's bindings (§4.5): P or Esc leaves, WASD flies, Q/E move down and up, a left or right
/// drag looks around and the wheel dollies. Nothing places, rotates or opens a panel.
pub fn map_photo_input(f: &InputFrame) -> Vec<Action> {
    let mut out = vec![];
    if f.pressed(KeyCode::KeyP) && !f.ctrl_or_cmd() {
        out.push(Action::TogglePhoto);
    } else if f.pressed(KeyCode::Escape) {
        out.push(Action::ExitPhoto);
    }
    if f.motion != Vec2::ZERO
        && (f.mouse_held.contains(&MouseButton::Left) || f.mouse_held.contains(&MouseButton::Right))
    {
        out.push(Action::Look(f.motion));
    }
    if !f.ctrl_or_cmd() {
        let axis = |pos: &[KeyCode], neg: &[KeyCode]| {
            f.held_any(pos) as i32 as f32 - f.held_any(neg) as i32 as f32
        };
        let dir = Vec3::new(
            axis(
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            ),
            axis(&[KeyCode::KeyE], &[KeyCode::KeyQ]),
            axis(
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            ),
        );
        let speed = FLY_SPEED * if f.shift() { 4.0 } else { 1.0 };
        let mut step = dir.normalize_or_zero() * speed * f.dt;
        step.z += f.wheel * FLY_PER_NOTCH;
        if step != Vec3::ZERO {
            out.push(Action::Fly(step));
        }
    }
    out
}

/// Walk mode's bindings (PRD P8): F or Esc leaves, WASD or the arrows walk at the `speed` setting
/// (Shift sprints at twice it), Space hops, and the mouse looks while a button is held or the page
/// holds the pointer lock. Nothing places, rotates or opens a panel.
pub fn map_walk_input(
    f: &InputFrame,
    pointer_lock: bool,
    speed: crate::walk::WalkSpeed,
) -> Vec<Action> {
    let mut out = vec![];
    let ctrl = f.ctrl_or_cmd();
    if f.pressed(KeyCode::KeyF) && !ctrl {
        out.push(Action::ToggleWalk);
    } else if f.pressed(KeyCode::Escape) {
        out.push(Action::ExitWalk);
    }
    if f.pressed(KeyCode::Space) {
        out.push(Action::Hop);
    }
    if f.motion != Vec2::ZERO
        && (pointer_lock
            || f.mouse_held.contains(&MouseButton::Left)
            || f.mouse_held.contains(&MouseButton::Right))
    {
        out.push(Action::WalkLook(f.motion));
    }
    if !ctrl {
        let axis = |pos: &[KeyCode], neg: &[KeyCode]| {
            f.held_any(pos) as i32 as f32 - f.held_any(neg) as i32 as f32
        };
        let dir = Vec2::new(
            axis(
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            ),
            axis(
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            ),
        );
        if dir != Vec2::ZERO {
            out.push(Action::Stride(dir.normalize() * speed.of(f.shift()) * f.dt));
        }
    }
    out
}

/// The §4.5 bindings. `rotate_wheel` carries fractional Ctrl+wheel travel between frames, so one
/// notch is one rotation step and a trackpad flick rotates at most once per frame.
pub fn map_input(f: &InputFrame, rotate_wheel: &mut f32) -> Vec<Action> {
    let mut out = vec![];
    let ctrl = f.ctrl_or_cmd();
    let shift = f.shift();

    if ctrl && f.pressed(KeyCode::KeyZ) {
        out.push(if shift { Action::Redo } else { Action::Undo });
    }
    if !ctrl {
        if f.pressed(KeyCode::KeyR) {
            out.push(Action::Rotate(if shift {
                RotateDir::Back
            } else {
                RotateDir::Forward
            }));
        }
        if f.pressed(KeyCode::KeyB) {
            out.push(Action::ToggleBulldoze);
        }
        if f.pressed(KeyCode::KeyP) {
            out.push(Action::TogglePhoto);
        }
        if f.pressed(KeyCode::KeyF) {
            out.push(Action::ToggleWalk);
        }
        for (i, k) in SELECT_KEYS.iter().enumerate() {
            if f.pressed(*k) {
                out.push(Action::SelectNth(i));
            }
        }
    }
    if f.pressed(KeyCode::Tab) {
        out.push(Action::ToggleInventory);
    }
    if f.pressed(KeyCode::Escape) {
        out.push(Action::Escape);
    }

    if f.mouse_just_pressed.contains(&MouseButton::Left) {
        out.push(if ctrl {
            Action::QuickReplace
        } else if shift {
            Action::QuickPlace
        } else {
            Action::Primary
        });
    }
    if f.motion != Vec2::ZERO {
        if f.mouse_held.contains(&MouseButton::Right) {
            out.push(Action::Orbit(f.motion));
        } else if f.mouse_held.contains(&MouseButton::Middle) {
            out.push(Action::Pan(
                Vec2::new(-f.motion.x, f.motion.y) * DRAG_PAN_PER_PX,
            ));
        }
    }
    if !ctrl {
        let axis = |pos: &[KeyCode], neg: &[KeyCode]| {
            f.held_any(pos) as i32 as f32 - f.held_any(neg) as i32 as f32
        };
        let dir = Vec2::new(
            axis(
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            ),
            axis(
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            ),
        );
        if dir != Vec2::ZERO {
            out.push(Action::Pan(dir.normalize() * KEY_PAN_SPEED * f.dt));
        }
    }

    if ctrl {
        *rotate_wheel += f.wheel;
        if rotate_wheel.abs() >= 1.0 {
            out.push(Action::Rotate(if *rotate_wheel > 0.0 {
                RotateDir::Forward
            } else {
                RotateDir::Back
            }));
            *rotate_wheel = 0.0;
        }
    } else {
        *rotate_wheel = 0.0;
        if f.wheel != 0.0 {
            out.push(Action::Zoom(f.wheel));
        }
    }
    out
}

/// Reads this frame's keyboard and mouse into the action queue.
#[allow(clippy::too_many_arguments)]
pub fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    game: Res<crate::game::GameState>,
    walk: Res<crate::walk::WalkSession>,
    mut rotate_wheel: Local<f32>,
    mut queue: ResMut<ActionQueue>,
) {
    let wheel = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_NOTCH,
    };
    let frame = InputFrame {
        just_pressed: keys.get_just_pressed().copied().collect(),
        held: keys.get_pressed().copied().collect(),
        mouse_just_pressed: mouse.get_just_pressed().copied().collect(),
        mouse_held: mouse.get_pressed().copied().collect(),
        motion: motion.delta,
        wheel,
        dt: time.delta_secs(),
    };
    if game.photo_mode {
        *rotate_wheel = 0.0;
        queue.0.extend(map_photo_input(&frame));
    } else if game.walk_mode {
        *rotate_wheel = 0.0;
        queue
            .0
            .extend(map_walk_input(&frame, walk.pointer_lock, walk.speed));
    } else {
        queue.0.extend(map_input(&frame, &mut rotate_wheel));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(keys: &[KeyCode], held: &[KeyCode]) -> Vec<Action> {
        let f = InputFrame {
            just_pressed: keys.to_vec(),
            held: held.iter().chain(keys).copied().collect(),
            ..default()
        };
        map_input(&f, &mut 0.0)
    }

    #[test]
    fn single_keys_map_to_the_table() {
        assert_eq!(
            press(&[KeyCode::KeyR], &[]),
            [Action::Rotate(RotateDir::Forward)]
        );
        assert_eq!(press(&[KeyCode::KeyB], &[]), [Action::ToggleBulldoze]);
        assert_eq!(press(&[KeyCode::Tab], &[]), [Action::ToggleInventory]);
        assert_eq!(press(&[KeyCode::Escape], &[]), [Action::Escape]);
        assert_eq!(press(&[KeyCode::KeyP], &[]), [Action::TogglePhoto]);
        assert_eq!(press(&[KeyCode::Digit3], &[]), [Action::SelectNth(2)]);
        assert_eq!(press(&[KeyCode::Equal], &[]), [Action::SelectNth(11)]);
        assert_eq!(press(&[KeyCode::KeyF], &[]), [Action::ToggleWalk]);
    }

    #[test]
    fn walk_mode_walks_sprints_hops_looks_and_leaves() {
        use crate::walk::WalkSpeed;
        let map_walk_input =
            |f: &InputFrame, lock: bool| map_walk_input(f, lock, WalkSpeed::default());
        let f = InputFrame {
            held: vec![KeyCode::KeyW],
            dt: 0.5,
            ..default()
        };
        assert_eq!(
            map_walk_input(&f, false),
            [Action::Stride(Vec2::new(0.0, 5.0 * 0.5))],
            "W walks at the default 5.0 m/s"
        );
        let sprint = InputFrame {
            held: vec![KeyCode::KeyD, KeyCode::ShiftLeft],
            dt: 1.0,
            ..default()
        };
        assert_eq!(
            map_walk_input(&sprint, false),
            [Action::Stride(Vec2::new(10.0, 0.0))],
            "Shift sprints at twice the walk speed"
        );
        let set = WalkSpeed::new(6.0).unwrap();
        assert_eq!(
            super::map_walk_input(&f, false, set),
            [Action::Stride(Vec2::new(0.0, 3.0))],
            "the setting changes the walk"
        );
        assert_eq!(
            super::map_walk_input(&sprint, false, set),
            [Action::Stride(Vec2::new(12.0, 0.0))],
            "and the sprint stays twice it"
        );
        let press = |k| InputFrame {
            just_pressed: vec![k],
            held: vec![k],
            ..default()
        };
        assert_eq!(map_walk_input(&press(KeyCode::Space), false), [Action::Hop]);
        assert_eq!(
            map_walk_input(&press(KeyCode::KeyF), false),
            [Action::ToggleWalk]
        );
        assert_eq!(
            map_walk_input(&press(KeyCode::Escape), false),
            [Action::ExitWalk]
        );
        for k in [
            KeyCode::KeyR,
            KeyCode::KeyB,
            KeyCode::Tab,
            KeyCode::KeyP,
            KeyCode::Digit1,
        ] {
            assert!(
                map_walk_input(&press(k), false).is_empty(),
                "{k:?} is inert in walk mode"
            );
        }
        let hover = InputFrame {
            motion: Vec2::new(6.0, 2.0),
            ..default()
        };
        assert!(
            map_walk_input(&hover, false).is_empty(),
            "no look without a button"
        );
        assert_eq!(
            map_walk_input(&hover, true),
            [Action::WalkLook(Vec2::new(6.0, 2.0))],
            "the pointer lock looks with no button"
        );
        let drag = InputFrame {
            mouse_just_pressed: vec![MouseButton::Left],
            mouse_held: vec![MouseButton::Left],
            motion: Vec2::new(-3.0, 1.0),
            ..default()
        };
        assert_eq!(
            map_walk_input(&drag, false),
            [Action::WalkLook(Vec2::new(-3.0, 1.0))],
            "a click looks, never places"
        );
        assert!(Action::Hop.is_walk() && !Action::Hop.is_camera());
    }

    #[test]
    fn modifiers_pick_the_variant() {
        assert_eq!(
            press(&[KeyCode::KeyR], &[KeyCode::ShiftLeft]),
            [Action::Rotate(RotateDir::Back)]
        );
        for ctrl in [KeyCode::ControlLeft, KeyCode::SuperRight] {
            assert_eq!(press(&[KeyCode::KeyZ], &[ctrl]), [Action::Undo]);
            assert_eq!(
                press(&[KeyCode::KeyZ], &[ctrl, KeyCode::ShiftRight]),
                [Action::Redo]
            );
            // Ctrl/Cmd+R is the browser's reload, not a rotation.
            assert!(press(&[KeyCode::KeyR], &[ctrl]).is_empty());
        }
        assert!(press(&[KeyCode::KeyZ], &[]).is_empty(), "bare Z is unbound");
    }

    #[test]
    fn mouse_buttons_and_drags() {
        let click = InputFrame {
            mouse_just_pressed: vec![MouseButton::Left],
            mouse_held: vec![MouseButton::Left],
            ..default()
        };
        assert_eq!(map_input(&click, &mut 0.0), [Action::Primary]);

        let orbit = InputFrame {
            mouse_held: vec![MouseButton::Right],
            motion: Vec2::new(10.0, -4.0),
            ..default()
        };
        assert_eq!(
            map_input(&orbit, &mut 0.0),
            [Action::Orbit(Vec2::new(10.0, -4.0))]
        );

        let pan = InputFrame {
            mouse_held: vec![MouseButton::Middle],
            motion: Vec2::new(100.0, 50.0),
            ..default()
        };
        let [Action::Pan(d)] = map_input(&pan, &mut 0.0)[..] else {
            panic!("middle-drag must pan");
        };
        assert!(d.x < 0.0 && d.y > 0.0, "grab semantics: {d}");

        let hover_only = InputFrame {
            motion: Vec2::new(5.0, 5.0),
            ..default()
        };
        assert!(map_input(&hover_only, &mut 0.0).is_empty());
    }

    fn click_with(held: &[KeyCode]) -> Vec<Action> {
        let f = InputFrame {
            held: held.to_vec(),
            mouse_just_pressed: vec![MouseButton::Left],
            mouse_held: vec![MouseButton::Left],
            ..default()
        };
        map_input(&f, &mut 0.0)
    }

    #[test]
    fn modifier_clicks_place_and_replace() {
        assert_eq!(click_with(&[]), [Action::Primary], "a plain click pins");
        for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            assert_eq!(click_with(&[shift]), [Action::QuickPlace]);
        }
        for m in [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ] {
            assert_eq!(click_with(&[m]), [Action::QuickReplace], "{m:?}");
            assert_eq!(
                click_with(&[m, KeyCode::ShiftLeft]),
                [Action::QuickReplace],
                "Ctrl or Cmd wins over Shift"
            );
        }
        let held_only = InputFrame {
            held: vec![KeyCode::ShiftLeft],
            mouse_held: vec![MouseButton::Left],
            ..default()
        };
        assert!(
            map_input(&held_only, &mut 0.0).is_empty(),
            "only the press acts, not a held button"
        );
    }

    #[test]
    fn wasd_and_arrows_pan() {
        let f = InputFrame {
            held: vec![KeyCode::KeyW, KeyCode::ArrowRight],
            dt: 0.5,
            ..default()
        };
        let [Action::Pan(d)] = map_input(&f, &mut 0.0)[..] else {
            panic!("expected one pan");
        };
        assert!(d.x > 0.0 && d.y > 0.0);
        assert!((d.length() - KEY_PAN_SPEED * 0.5).abs() < 1e-5);
    }

    #[test]
    fn wheel_zooms_and_ctrl_wheel_rotates_once_per_notch() {
        let zoom = InputFrame {
            wheel: 2.0,
            ..default()
        };
        assert_eq!(map_input(&zoom, &mut 0.0), [Action::Zoom(2.0)]);

        let mut acc = 0.0;
        let half = InputFrame {
            held: vec![KeyCode::ControlLeft],
            wheel: 0.5,
            ..default()
        };
        assert!(map_input(&half, &mut acc).is_empty(), "half a notch");
        assert_eq!(
            map_input(&half, &mut acc),
            [Action::Rotate(RotateDir::Forward)]
        );
        let flick = InputFrame {
            held: vec![KeyCode::ControlLeft],
            wheel: -7.0,
            ..default()
        };
        assert_eq!(
            map_input(&flick, &mut acc),
            [Action::Rotate(RotateDir::Back)],
            "a flick rotates once, never zooms"
        );
    }

    #[test]
    fn photo_mode_flies_looks_and_leaves() {
        let fly = InputFrame {
            held: vec![KeyCode::KeyW, KeyCode::KeyE],
            dt: 0.5,
            ..default()
        };
        let [Action::Fly(d)] = map_photo_input(&fly)[..] else {
            panic!("WASD/QE fly in photo mode");
        };
        assert!(d.z > 0.0 && d.y > 0.0 && d.x == 0.0, "{d}");
        assert!((d.length() - FLY_SPEED * 0.5).abs() < 1e-4);
        let down = InputFrame {
            held: vec![KeyCode::KeyQ],
            dt: 1.0,
            ..default()
        };
        assert_eq!(
            map_photo_input(&down),
            [Action::Fly(Vec3::new(0.0, -FLY_SPEED, 0.0))]
        );

        let look = InputFrame {
            mouse_just_pressed: vec![MouseButton::Left],
            mouse_held: vec![MouseButton::Left],
            motion: Vec2::new(8.0, -3.0),
            ..default()
        };
        assert_eq!(
            map_photo_input(&look),
            [Action::Look(Vec2::new(8.0, -3.0))],
            "a click looks, never places"
        );
        let press = |k| InputFrame {
            just_pressed: vec![k],
            held: vec![k],
            ..default()
        };
        assert_eq!(
            map_photo_input(&press(KeyCode::KeyP)),
            [Action::TogglePhoto]
        );
        assert_eq!(
            map_photo_input(&press(KeyCode::Escape)),
            [Action::ExitPhoto]
        );
        for k in [KeyCode::KeyR, KeyCode::KeyB, KeyCode::Tab, KeyCode::Digit1] {
            assert!(
                map_photo_input(&press(k)).is_empty(),
                "{k:?} is inert in photo mode"
            );
        }
        assert!(Action::Fly(Vec3::X).is_camera() && Action::Look(Vec2::X).is_camera());
    }

    #[test]
    fn camera_actions_are_classified() {
        assert!(Action::Zoom(1.0).is_camera());
        assert!(Action::Pan(Vec2::X).is_camera());
        assert!(!Action::Primary.is_camera());
        assert!(!Action::Undo.is_camera());
    }
}
