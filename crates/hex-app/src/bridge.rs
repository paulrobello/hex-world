//! wasm-bindgen exports behind window.hexWorld. Shared state is written by Bevy systems.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::math::{Mat4, Vec2, Vec3};
use hex_core::{Axial, HEX_RADIUS};

pub static READY: Mutex<bool> = Mutex::new(false);
/// Game events as JSON objects, oldest first, drained by `hex_world_events` (`hexWorld.events()`).
pub static EVENTS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
pub static STATE_JSON: Mutex<StatePublication> = Mutex::new(StatePublication::new());
pub static DIAG_JSON: Mutex<String> = Mutex::new(String::new());
/// Serialized fields are retained once. The private wasm transport drains only changed fields,
/// while native dumps assemble the complete public snapshot on demand.
pub struct StatePublication {
    state: std::collections::BTreeMap<String, String>,
    play: std::collections::BTreeMap<String, String>,
    dirty: std::collections::BTreeMap<String, String>,
    dirty_play: std::collections::BTreeMap<String, String>,
}

impl StatePublication {
    pub const fn new() -> Self {
        Self {
            state: std::collections::BTreeMap::new(),
            play: std::collections::BTreeMap::new(),
            dirty: std::collections::BTreeMap::new(),
            dirty_play: std::collections::BTreeMap::new(),
        }
    }

    pub fn set_raw(&mut self, key: &str, json: &str) {
        if self.state.get(key).map(String::as_str) != Some(json) {
            self.set(key, json.into());
        }
    }

    pub fn set(&mut self, key: &str, json: String) {
        if self.state.get(key) == Some(&json) {
            return;
        }
        if key == "play" {
            self.play.clear();
            self.dirty_play.clear();
        }
        self.state.insert(key.into(), json.clone());
        self.dirty.insert(key.into(), json);
    }

    pub fn set_play(&mut self, key: &str, json: String) {
        if self.play.get(key) == Some(&json) {
            return;
        }
        self.play.insert(key.into(), json.clone());
        self.dirty_play.insert(key.into(), json);
    }

    fn object(fields: &std::collections::BTreeMap<String, String>) -> String {
        let mut json = String::from("{");
        for (i, (key, value)) in fields.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&serde_json::to_string(key).unwrap());
            json.push(':');
            json.push_str(value);
        }
        json.push('}');
        json
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn take_patch(&mut self) -> String {
        if self.dirty.is_empty() && self.dirty_play.is_empty() {
            return String::new();
        }
        let dirty = std::mem::take(&mut self.dirty);
        let dirty_play = std::mem::take(&mut self.dirty_play);
        format!(
            "{{\"state\":{},\"play\":{}}}",
            Self::object(&dirty),
            Self::object(&dirty_play)
        )
    }

    pub fn snapshot(&self) -> String {
        let mut fields = self
            .state
            .iter()
            .map(|(k, v)| {
                let value = if k == "play" && v == "{}" {
                    Self::object(&self.play)
                } else {
                    v.clone()
                };
                format!("{}:{value}", serde_json::to_string(k).unwrap())
            })
            .collect::<Vec<_>>();
        fields.sort();
        format!("{{{}}}", fields.join(","))
    }
}

#[cfg(test)]
mod publication_tests {
    use super::StatePublication;
    use serde_json::{Value, json};

    #[test]
    fn publication_coalesces_changes_and_keeps_a_full_native_snapshot() {
        let mut state = StatePublication::new();
        assert_eq!(state.take_patch(), "");
        state.set("frames", "1".into());
        state.set("play", "{}".into());
        state.set_play("tiles", "[{\"q\":0}]".into());
        state.set_play("focus", "[0,0,0]".into());
        let first: Value = serde_json::from_str(&state.take_patch()).unwrap();
        assert_eq!(first["play"]["tiles"], json!([{ "q": 0 }]));
        assert_eq!(state.take_patch(), "");
        state.set_play("tiles", "[{\"q\":0}]".into());
        state.set_play("focus", "[1,0,0]".into());
        state.set("frames", "2".into());
        state.set("frames", "3".into());
        let patch: Value = serde_json::from_str(&state.take_patch()).unwrap();
        assert!(patch["play"].get("tiles").is_none());
        assert_eq!(patch["play"]["focus"], json!([1, 0, 0]));
        assert_eq!(patch["state"]["frames"], 3);
        let full: Value = serde_json::from_str(&state.snapshot()).unwrap();
        assert_eq!(full["play"]["tiles"], json!([{ "q": 0 }]));
        assert_eq!(full["play"]["focus"], json!([1, 0, 0]));
        assert_eq!(full["frames"], 3);
    }

    #[test]
    fn scenario_changes_replace_play_instead_of_reviving_stale_fields() {
        let mut state = StatePublication::new();
        state.set("play", "{}".into());
        state.set_play("tiles", "[]".into());
        state.take_patch();
        state.set("play", "null".into());
        let full: Value = serde_json::from_str(&state.snapshot()).unwrap();
        assert!(full["play"].is_null());
        state.set("play", "{}".into());
        state.set_play("tiles", "[]".into());
        let patch: Value = serde_json::from_str(&state.take_patch()).unwrap();
        assert_eq!(patch["play"]["tiles"], json!([]));
    }
}

/// The game camera as of the last frame, for `hexWorld.cellRect` (the UI overlay's anchors).
pub static VIEW: Mutex<Option<View>> = Mutex::new(None);

/// What projecting a world point to CSS pixels needs: the camera's clip-from-world matrix and its
/// viewport in logical pixels (the canvas fills the page, so logical pixels are CSS pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub clip_from_world: Mat4,
    pub origin: Vec2,
    pub size: Vec2,
}

/// Height of the upper corners projected for a cell: about a placed tile's cap.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const CELL_TOP: f32 = 2.0;

/// The screen rectangle `[left, top, right, bottom]` (CSS px) around `cell`'s hex at ground and
/// cap height, or `None` when any corner is behind the camera. Natively only the tests call it.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn cell_rect(view: &View, cell: Axial) -> Option<[f32; 4]> {
    let [cx, cz] = cell.to_world(HEX_RADIUS);
    let mut rect = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for i in 0..6 {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        for h in [0.0, CELL_TOP] {
            let p = Vec3::new(cx + HEX_RADIUS * a.cos(), h, cz + HEX_RADIUS * a.sin());
            let clip = view.clip_from_world * p.extend(1.0);
            if clip.w <= 1e-4 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            let sx = view.origin.x + (ndc.x + 1.0) * 0.5 * view.size.x;
            let sy = view.origin.y + (1.0 - ndc.y) * 0.5 * view.size.y;
            rect = [
                rect[0].min(sx),
                rect[1].min(sy),
                rect[2].max(sx),
                rect[3].max(sy),
            ];
        }
    }
    Some(rect)
}

#[cfg(target_arch = "wasm32")]
mod exports {
    use wasm_bindgen::prelude::*;

    use crate::{capture, control};

    #[wasm_bindgen]
    pub fn hex_world_ready() -> bool {
        *super::READY.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[wasm_bindgen]
    pub fn hex_world_state_json() -> String {
        super::STATE_JSON
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take_patch()
    }

    #[wasm_bindgen]
    pub fn hex_world_diag_json() -> String {
        let s = super::DIAG_JSON.lock().unwrap_or_else(|p| p.into_inner());
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    /// Drains the game event queue as a JSON array (`[]` when empty).
    #[wasm_bindgen]
    pub fn hex_world_events() -> String {
        let drained: Vec<String> = super::EVENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
            .collect();
        format!("[{}]", drained.join(","))
    }

    /// `[left, top, right, bottom]` in CSS pixels around cell `(q, r)`, or `null` before the
    /// camera exists or when the cell is behind it.
    #[wasm_bindgen]
    pub fn hex_world_cell_rect(q: i32, r: i32) -> String {
        let view = *super::VIEW.lock().unwrap_or_else(|p| p.into_inner());
        match view.and_then(|v| super::cell_rect(&v, hex_core::Axial::new(q, r))) {
            Some(r) => format!("[{},{},{},{}]", r[0], r[1], r[2], r[3]),
            None => "null".into(),
        }
    }

    /// Enqueues one command object or an array of them. Returns false and logs on a parse error.
    #[wasm_bindgen]
    pub fn hex_world_cmd(json: &str) -> bool {
        let parsed = if json.trim_start().starts_with('{') {
            control::parse_script(&format!("[{json}]"))
        } else {
            control::parse_script(json)
        };
        match parsed {
            Ok(cmds) => {
                control::INBOX
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .extend(cmds);
                true
            }
            Err(e) => {
                web_sys::console::error_1(&format!("hexWorld.cmd: {e}").into());
                false
            }
        }
    }

    /// Commands `hex_world_cmd` took that no frame has drained into the command queue yet.
    #[wasm_bindgen]
    pub fn hex_world_inbox_len() -> usize {
        control::INBOX
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    /// Queues a screenshot behind any pending commands; poll `hex_world_take_shot` for the result.
    #[wasm_bindgen]
    pub fn hex_world_screenshot(name: String) {
        capture::SHOTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(n, _)| *n != name);
        control::INBOX
            .lock()
            .unwrap()
            .push(control::Command::Screenshot { name });
    }

    /// Removes and returns the PNG data URL for `name` once the capture has landed.
    #[wasm_bindgen]
    pub fn hex_world_take_shot(name: String) -> Option<String> {
        let mut shots = capture::SHOTS.lock().unwrap_or_else(|p| p.into_inner());
        let i = shots.iter().position(|(n, _)| *n == name)?;
        Some(shots.remove(i).1)
    }

    /// The `.hexworld` bytes of the `save` command tagged `tag` (a `Uint8Array`), once its `saved`
    /// event has fired; `undefined` before that or after they were taken. Bytes travel as typed
    /// arrays beside the JSON command bridge, never as base64.
    #[wasm_bindgen]
    pub fn hex_world_take_save(tag: String) -> Option<Vec<u8>> {
        crate::persist::take_save(&tag)
    }

    /// The `.hexworld` bytes of the world as of the last frame (a `Uint8Array`), at once, for a
    /// page hide that cannot wait a frame for a `save` command. `volumes` is an optional
    /// `Float32Array` of master, music, sfx, ambience and ui; `undefined` before the first frame.
    #[wasm_bindgen]
    pub fn hex_world_save_now(volumes: Option<Vec<f32>>) -> Option<Vec<u8>> {
        let volumes = volumes.and_then(|v| match v[..] {
            [master, music, sfx, ambience, ui] => Some(crate::persist::Volumes {
                master,
                music,
                sfx,
                ambience,
                ui,
            }),
            _ => None,
        });
        crate::persist::save_now(volumes)
    }

    /// Validates `.hexworld` bytes (a `Uint8Array`) and queues their load behind every command
    /// already sent. Returns JSON: `{"ok":true,"tiles":n,"version":v}`, or
    /// `{"ok":false,"code":..,"reason":..,"version":..}` with a `loadRefused` event and no change.
    #[wasm_bindgen]
    pub fn hex_world_load(bytes: &[u8]) -> String {
        crate::persist::load_result_json(&crate::persist::request_load(bytes))
    }

    /// Decodes `.hexworld` bytes (a `Uint8Array`) without loading them, for the import preview:
    /// the file's version, the newest version this build reads, and its tiles, or the refusal.
    #[wasm_bindgen]
    pub fn hex_world_inspect(bytes: &[u8]) -> String {
        crate::persist::inspect_json(bytes)
    }
}

/// The bridge contract the web UI overlay is typed against (`web/ui/src/bridge-schema.json`):
/// command and event names with their fields, enum spellings, the `play` state keys, the tile
/// catalog with its socket table, and legal rotations for every frontier slot of five
/// worlds (the overlay's fit rules are checked against them). `make ui-schema` rewrites the file;
/// `web/ui/test/bridge-sync.test.ts` checks the TypeScript side against it.
#[cfg(test)]
mod schema {
    use std::collections::BTreeMap;

    use bevy::camera::CameraProjection;
    use bevy::prelude::{GlobalTransform, PerspectiveProjection, Transform, Vec2, Vec3};
    use hex_core::{Axial, EdgeStatus, HEX_RADIUS, SocketKind, TileCatalog, World};
    use serde_json::{Value, json};

    use super::{View, cell_rect};
    use crate::actions::{Action, Tool};
    use crate::control::{Command, Spin, Toggle, parse_script};
    use crate::game::{GameEvent, GameState};

    const SCHEMA_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/ui/src/bridge-schema.json"
    );
    const PARITY_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/ui/test/rules-parity.json"
    );
    const COMMITTED: &str = include_str!("../../../web/ui/src/bridge-schema.json");
    const COMMITTED_PARITY: &str = include_str!("../../../web/ui/test/rules-parity.json");

    /// The variant names serde accepts for an enum, read from its "unknown variant" error.
    fn serde_variants<T: serde::de::DeserializeOwned + std::fmt::Debug>(
        probe: &str,
    ) -> Vec<String> {
        let e = serde_json::from_str::<T>(probe).unwrap_err().to_string();
        let list = e
            .split("expected")
            .nth(1)
            .unwrap_or_else(|| panic!("no variant list in {e}"));
        let mut v: Vec<String> = list
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect();
        v.sort();
        v
    }

    /// One sample of every command (the tags must match serde's own variant list).
    const COMMAND_SAMPLES: &[&str] = &[
        r#"{"cmd":"camera","pos":[0,90,90],"look_at":[0,0,0]}"#,
        r#"{"cmd":"screenshot","name":"a"}"#,
        r#"{"cmd":"wait","frames":1}"#,
        r#"{"cmd":"scenario","name":"play"}"#,
        r#"{"cmd":"tilt","on":true,"strength":0.5}"#,
        r#"{"cmd":"dump_state","name":"s"}"#,
        r#"{"cmd":"set_tier","tier":"low"}"#,
        r#"{"cmd":"set_grade","grade":"rich-1"}"#,
        r#"{"cmd":"set_grass","density":1.2}"#,
        r#"{"cmd":"set_grass_distance","scale":2}"#,
        r#"{"cmd":"set_reduced_motion","on":true}"#,
        r#"{"cmd":"set_car_cap","cap":1}"#,
        r#"{"cmd":"set_frame_cap","fps":60}"#,
        r#"{"cmd":"exit"}"#,
        r#"{"cmd":"select_tile","id":"meadow"}"#,
        r#"{"cmd":"hover","q":1,"r":0}"#,
        r#"{"cmd":"rotate","dir":"cw"}"#,
        r#"{"cmd":"place","q":1,"r":0}"#,
        r#"{"cmd":"pin","q":1,"r":0}"#,
        r#"{"cmd":"unpin"}"#,
        r#"{"cmd":"replace","q":1,"r":0}"#,
        r#"{"cmd":"undo"}"#,
        r#"{"cmd":"redo"}"#,
        r#"{"cmd":"bulldoze","q":1,"r":0}"#,
        r#"{"cmd":"set_tool","mode":"bulldoze"}"#,
        r#"{"cmd":"toggle","what":"inventory"}"#,
        r#"{"cmd":"photo","dof":"tilt","focus":64,"aperture":2.8,"exposure":0.3,"fov":50,"roll":0,"pause_agents":true,"filter":"mono","filter_strength":1,"vignette":0.5,"vignette_softness":0.5,"grain":0.2,"aberration":0.1,"focus_at":[0.5,0.62]}"#,
        r#"{"cmd":"photo_export","scale":2,"name":"hex-world"}"#,
        r#"{"cmd":"save","tag":"autosave","path":null,"volumes":{"master":0.8,"music":0.55,"sfx":0.75,"ambience":0.7,"ui":0.5}}"#,
        r#"{"cmd":"load","path":null}"#,
        r#"{"cmd":"new_world"}"#,
        r#"{"cmd":"walk","on":true,"pointer_lock":false}"#,
        r#"{"cmd":"walk_to","x":4,"z":-2,"heading":90,"pitch":-5}"#,
        r#"{"cmd":"walk_step","forward":1,"right":0,"seconds":2,"sprint":false}"#,
        r#"{"cmd":"set_walk_speed","speed":4.2}"#,
        r#"{"cmd":"walk_look","heading":180,"pitch":0}"#,
        r#"{"cmd":"debug","lod_tint":true,"force_lod":"auto","perf":true,"grid":false,"sockets":false,"colliders":false,"scatter":false,"networks":false}"#,
    ];

    /// Exhaustive on purpose: a new command fails to compile here until it has a sample above.
    fn command_tag(c: &Command) -> &'static str {
        match c {
            Command::Camera { .. } => "camera",
            Command::Screenshot { .. } => "screenshot",
            Command::Wait { .. } => "wait",
            Command::Scenario { .. } => "scenario",
            Command::Tilt { .. } => "tilt",
            Command::DumpState { .. } => "dump_state",
            Command::SetTier { .. } => "set_tier",
            Command::SetGrade { .. } => "set_grade",
            Command::SetGrass { .. } => "set_grass",
            Command::SetGrassDistance { .. } => "set_grass_distance",
            Command::SetReducedMotion { .. } => "set_reduced_motion",
            Command::SetCarCap { .. } => "set_car_cap",
            Command::SetFrameCap { .. } => "set_frame_cap",
            Command::Exit => "exit",
            Command::SelectTile { .. } => "select_tile",
            Command::Hover { .. } => "hover",
            Command::Rotate { .. } => "rotate",
            Command::Place { .. } => "place",
            Command::Pin { .. } => "pin",
            Command::Unpin => "unpin",
            Command::Replace { .. } => "replace",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Bulldoze { .. } => "bulldoze",
            Command::SetTool { .. } => "set_tool",
            Command::Toggle { .. } => "toggle",
            Command::Photo { .. } => "photo",
            Command::PhotoExport { .. } => "photo_export",
            Command::Save { .. } => "save",
            Command::Load { .. } => "load",
            Command::NewWorld => "new_world",
            Command::Walk { .. } => "walk",
            Command::WalkTo { .. } => "walk_to",
            Command::WalkStep { .. } => "walk_step",
            Command::SetWalkSpeed { .. } => "set_walk_speed",
            Command::WalkLook { .. } => "walk_look",
            Command::Debug { .. } => "debug",
        }
    }

    fn commands() -> BTreeMap<String, Vec<String>> {
        let mut out = BTreeMap::new();
        for s in COMMAND_SAMPLES {
            let cmd = &parse_script(&format!("[{s}]")).unwrap()[0];
            let v: Value = serde_json::from_str(s).unwrap();
            let tag = v["cmd"].as_str().unwrap();
            assert_eq!(command_tag(cmd), tag, "sample {s}");
            let mut fields: Vec<String> = v
                .as_object()
                .unwrap()
                .keys()
                .filter(|k| *k != "cmd")
                .cloned()
                .collect();
            fields.sort();
            assert!(
                out.insert(tag.to_owned(), fields).is_none(),
                "duplicate sample {tag}"
            );
        }
        let tags: Vec<String> = out.keys().cloned().collect();
        assert_eq!(
            tags,
            serde_variants::<Command>(r#"{"cmd":"__none__"}"#),
            "a command lacks a sample"
        );
        out
    }

    /// One sample of every event. The exhaustive match below forces a sample for a new variant.
    fn event_samples() -> Vec<GameEvent> {
        let samples = vec![
            GameEvent::TilePlaced {
                q: 1,
                r: 0,
                tile: "meadow".into(),
                rotation: 0,
            },
            GameEvent::PlacementRefused {
                q: 1,
                r: 0,
                tile: "ocean".into(),
                rotation: 0,
                reason: "edges [3] do not fit".into(),
            },
            GameEvent::TileBulldozed {
                q: 1,
                r: 0,
                tile: "meadow".into(),
            },
            GameEvent::BulldozeRefused {
                q: 1,
                r: 0,
                reason: "no tile here".into(),
            },
            GameEvent::TileReplaced {
                q: 1,
                r: 0,
                tile: "forest".into(),
                rotation: 2,
                replaced: "meadow".into(),
            },
            GameEvent::Undone,
            GameEvent::Redone,
            GameEvent::TileSelected {
                tile: "meadow".into(),
            },
            GameEvent::GhostRotated {
                rotation: 1,
                valid: true,
            },
            GameEvent::HoverChanged {
                cell: Some(Axial::new(1, 0)),
            },
            GameEvent::PinChanged {
                cell: Some(Axial::new(1, 0)),
            },
            GameEvent::ToolChanged { mode: Tool::Place },
            GameEvent::InventoryToggled { open: true },
            GameEvent::PhotoModeToggled { on: true },
            GameEvent::ActionRefused {
                action: "undo",
                reason: "nothing to undo".into(),
            },
            GameEvent::RiseStage {
                q: 1,
                r: 0,
                tile: "meadow".into(),
                stage: "column",
                skipped: false,
            },
            GameEvent::CarPulledOut {
                id: 3,
                q: -2,
                r: 1,
                x: -41.57,
                z: 24.0,
            },
            GameEvent::CarParked {
                id: 3,
                q: 3,
                r: -1,
                x: 69.28,
                z: -24.0,
            },
            GameEvent::PhotoExported {
                name: "hex-world".into(),
                width: 2880,
                height: 1800,
            },
            GameEvent::PhotoExportFailed {
                name: "hex-world".into(),
                reason: "too large".into(),
            },
            GameEvent::Saved {
                tag: Some("autosave".into()),
                tiles: 2,
                bytes: 120,
            },
            GameEvent::Loaded {
                tiles: 2,
                version: 1,
                settings: Some(crate::persist::SettingsJson {
                    quality: "auto",
                    tilt_shift: true,
                    tilt_shift_strength: 0.5,
                    volumes: crate::persist::SavedSettings::default().volumes.into(),
                }),
            },
            GameEvent::LoadRefused {
                code: "bad_magic",
                reason: "not a .hexworld file".into(),
                version: None,
            },
            GameEvent::WalkModeToggled { on: true },
            GameEvent::TrainDeparted {
                id: 1,
                x: -27.71,
                z: -24.0,
            },
            GameEvent::TrainAtLineEnd {
                id: 1,
                x: 37.07,
                z: 0.0,
            },
            GameEvent::BoatStroke {
                id: 0,
                x: 3.5,
                z: 20.1,
            },
            GameEvent::SailFlap {
                id: 3,
                x: -40.2,
                z: 8.0,
            },
        ];
        let mut seen = [false; 28];
        for e in &samples {
            let i = match e {
                GameEvent::TilePlaced { .. } => 0,
                GameEvent::PlacementRefused { .. } => 1,
                GameEvent::TileBulldozed { .. } => 2,
                GameEvent::BulldozeRefused { .. } => 3,
                GameEvent::Undone => 4,
                GameEvent::Redone => 5,
                GameEvent::TileSelected { .. } => 6,
                GameEvent::GhostRotated { .. } => 7,
                GameEvent::HoverChanged { .. } => 8,
                GameEvent::ToolChanged { .. } => 9,
                GameEvent::InventoryToggled { .. } => 10,
                GameEvent::PhotoModeToggled { .. } => 11,
                GameEvent::ActionRefused { .. } => 12,
                GameEvent::RiseStage { .. } => 13,
                GameEvent::PhotoExported { .. } => 14,
                GameEvent::PhotoExportFailed { .. } => 15,
                GameEvent::Saved { .. } => 16,
                GameEvent::Loaded { .. } => 17,
                GameEvent::LoadRefused { .. } => 18,
                GameEvent::PinChanged { .. } => 19,
                GameEvent::CarPulledOut { .. } => 20,
                GameEvent::CarParked { .. } => 21,
                GameEvent::TileReplaced { .. } => 22,
                GameEvent::WalkModeToggled { .. } => 23,
                GameEvent::TrainDeparted { .. } => 24,
                GameEvent::TrainAtLineEnd { .. } => 25,
                GameEvent::BoatStroke { .. } => 26,
                GameEvent::SailFlap { .. } => 27,
            };
            seen[i] = true;
        }
        assert!(seen.iter().all(|s| *s), "an event variant lacks a sample");
        samples
    }

    fn keys(v: &Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().expect("an object").keys().cloned().collect();
        k.sort();
        k
    }

    fn events() -> BTreeMap<String, Vec<String>> {
        event_samples()
            .iter()
            .map(|e| {
                let v = serde_json::to_value(e).unwrap();
                let tag = v["type"].as_str().unwrap().to_owned();
                (tag, keys(&v).into_iter().filter(|k| k != "type").collect())
            })
            .collect()
    }

    fn edge_status_names() -> Vec<String> {
        let mut v: Vec<String> = [
            EdgeStatus::Compatible,
            EdgeStatus::Incompatible,
            EdgeStatus::Neutral,
        ]
        .iter()
        .map(|s| {
            serde_json::to_value(s)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
        v.sort();
        assert_eq!(v, serde_variants::<EdgeStatus>(r#""__none__""#));
        v
    }

    /// The `play` object as `publish_state` builds it (game, cars, rise) with every optional part
    /// filled: a ghost, a bulldoze target, a result, a rising tile.
    fn play_shapes() -> Value {
        let mut g = GameState::new(1);
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Place(Axial::new(1, 0)));
        g.apply(Action::Hover(Some(Axial::new(-1, 0))));
        let mut place = g.state_json();
        crate::sim::CarSim::new(&g.world, &g.catalog).extend_state(&mut place);
        let rising = vec![(Axial::new(1, 0), "meadow".to_owned(), "column", 0.25)];
        crate::control::extend_play_rise(&mut place, rising, false);
        let view = Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y);
        crate::control::extend_play_view(&mut place, Some(&view));
        crate::control::extend_play_orbit(&mut place, Some(&view));
        g.apply(Action::SetTool(Tool::Bulldoze));
        g.apply(Action::Hover(Some(Axial::new(1, 0))));
        let bulldoze = g.state_json();
        let mut photo = crate::photo::state_json(
            &crate::photo::PhotoSettings::default(),
            &crate::photo::PhotoSession::default(),
            &crate::photo::PhotoExports::default(),
            &crate::photo::PhotoFocus {
                point: Some(Vec3::ZERO),
                hit: Some("tree"),
                ..Default::default()
            },
            false,
            0,
            None,
        );
        crate::photo_fx::extend_state(&mut photo, &Default::default(), false);
        let grade = crate::control::grade_json(hex_render::Grade::default());
        let grass: Value = serde_json::from_str(&hex_play::play::grass::report(
            &Default::default(),
            Default::default(),
            Default::default(),
            "on",
            0,
        ))
        .unwrap();
        let walk = crate::walk::state_json(
            &crate::walk::WalkSession::default(),
            &hex_play::walk::WalkGround::default(),
        );
        json!({
            "frame_cap": keys(&crate::frame_cap::FrameCap(Some(60)).json()),
            "walk": keys(&walk),
            "grade": keys(&grade),
            "grass": keys(&grass),
            "grade_preset": keys(&grade["presets"][0]),
            "play": keys(&place),
            "ghost": keys(&place["ghost"]),
            "bulldoze": keys(&bulldoze["bulldoze"]),
            "last_result": keys(&place["last_result"]),
            "tile": keys(&place["tiles"][0]),
            "rising": keys(&place["rising"][0]),
            "orbit": keys(&place["orbit"]),
            "cell": keys(&place["hovered"]),
            "hovered_kind": ["empty", "slot", "tile"],
            "photo": keys(&photo),
        })
    }

    fn kind_name(k: SocketKind) -> String {
        serde_json::to_value(k)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn catalog(c: &TileCatalog) -> Value {
        let tiles: Vec<Value> = c
            .tiles()
            .iter()
            .map(|t| {
                let (stopped, rail) = c.rail_ends(t);
                json!({
                    "id": t.id,
                    "edges": t.edges.iter().map(|e| kind_name(e.kind)).collect::<Vec<_>>(),
                    "stopped": (0..6).map(|e| stopped & (1 << e) != 0).collect::<Vec<_>>(),
                    "rail": rail,
                    "land": t.edges.iter().map(|e| serde_json::to_value(e.land).unwrap()).collect::<Vec<_>>(),
                    "scatter": t.props.scatter.iter().map(|s| s.piece.0.as_str()).collect::<Vec<_>>(),
                    "period": t.orientation_period(),
                })
            })
            .collect();
        let directional: Vec<String> = SocketKind::ALL
            .iter()
            .filter(|&&k| c.sockets().directional(k))
            .map(|&k| kind_name(k))
            .collect();
        let compatible: BTreeMap<String, Vec<String>> = SocketKind::ALL
            .iter()
            .map(|&a| {
                let ok = SocketKind::ALL
                    .iter()
                    .filter(|&&b| c.sockets().kinds_compatible(a, b))
                    .map(|&b| kind_name(b))
                    .collect();
                (kind_name(a), ok)
            })
            .collect();
        json!({
            "start": hex_core::START_TILE,
            "tiles": tiles,
            "picker": c.picker(),
            "compatible": compatible,
            "stopped_end_ground": kind_name(hex_core::rail_end::STOPPED_END_GROUND),
            "directional": directional,
            "hex_radius": HEX_RADIUS,
        })
    }

    /// A coast part-built (owner O1): four meadows, then a straight beach, a headland and a bay,
    /// so the frontier asks for beach edges with the land on either side.
    const COAST_PART_BUILT: &str = r#"[
        {"cmd": "select_tile", "id": "meadow"},
        {"cmd": "place", "q": 0, "r": 1},
        {"cmd": "place", "q": 1, "r": 1},
        {"cmd": "place", "q": -1, "r": 0},
        {"cmd": "select_tile", "id": "beach"},
        {"cmd": "place", "q": 0, "r": -1},
        {"cmd": "select_tile", "id": "beach_outer"},
        {"cmd": "place", "q": 1, "r": -1},
        {"cmd": "select_tile", "id": "beach_inner"},
        {"cmd": "place", "q": 1, "r": 0}
    ]"#;

    /// Stopped rail ends (owner, 2026-10-25): a station with a meadow beyond one end, a junction
    /// off its other end and a level crossing west of the start meadow, so the frontier holds slots
    /// that stopped ends of each kind face.
    const RAIL_ENDS: &[(i32, i32, &str, u8)] = &[
        (1, 0, "rail_station", 1),
        (1, 1, "meadow", 0),
        (1, -1, "rail_junction", 0),
        (-1, 0, "rail_road_crossing", 1),
    ];

    /// [`RAIL_ENDS`] placed on a new world, each placement legal.
    fn rail_ends_world(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        for &(q, r, id, k) in RAIL_ENDS {
            w.place(c, Axial::new(q, r), id, k).unwrap();
        }
        w
    }

    /// Legal rotations of every catalog tile at every frontier slot of the world a script leaves.
    fn parity(c: &TileCatalog, script: &str) -> Value {
        let mut g = GameState::new(1);
        for cmd in parse_script(script).unwrap() {
            if let Some(a) = cmd.action() {
                g.apply(a);
            }
        }
        parity_of(c, &g.world)
    }

    /// Legal rotations of every catalog tile at every frontier slot of `w`.
    fn parity_of(c: &TileCatalog, w: &World) -> Value {
        let tiles: Vec<Value> = w
            .tiles()
            .iter()
            .map(|(cell, t)| json!([cell.q, cell.r, t.def, t.rotation]))
            .collect();
        let slots: BTreeMap<String, BTreeMap<String, Vec<u8>>> = w
            .frontier()
            .iter()
            .map(|s| {
                let per_tile = c
                    .tiles()
                    .iter()
                    .map(|t| (t.id.clone(), w.legal_rotations(c, *s, t)))
                    .collect();
                (format!("{},{}", s.q, s.r), per_tile)
            })
            .collect();
        json!({ "tiles": tiles, "slots": slots })
    }

    fn schema() -> Value {
        let c = TileCatalog::builtin().unwrap();
        let tool: Vec<String> = serde_variants::<Tool>(r#""__none__""#);
        // Exhaustive on purpose, so a new tool reaches this file.
        for t in [Tool::Place, Tool::Bulldoze] {
            match t {
                Tool::Place | Tool::Bulldoze => {}
            }
        }
        json!({
            "about": "Generated by the hex-app bridge schema test (make ui-schema). The web UI overlay's bridge types are checked against it (web/ui/test/bridge-sync.test.ts).",
            "commands": commands(),
            "events": events(),
            "enums": {
                "tool": tool,
                "spin": serde_variants::<Spin>(r#""__none__""#),
                "toggle": serde_variants::<Toggle>(r#""__none__""#),
                "dof": serde_variants::<crate::photo::Dof>(r#""__none__""#),
                "photo_filter": serde_variants::<crate::photo_fx::PhotoFilter>(r#""__none__""#),
                "edge_status": edge_status_names(),
                "grade": hex_render::Grade::ALL.iter().map(|g| g.name()).collect::<Vec<_>>(),
                "grass": hex_play::play::grass::Density::ALIASES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                "grass_distance": hex_play::play::grass::GrassDistance::ALIASES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            },
            "state": play_shapes(),
            "catalog": catalog(&c),
        })
    }

    fn parity_worlds() -> Value {
        let c = TileCatalog::builtin().unwrap();
        json!({
            "about": "Legal rotations of every tile at every frontier slot of five worlds, from hex-core (make ui-schema). web/ui/test/rules.test.ts checks the overlay's fit rules against it.",
            "worlds": [
                parity(&c, include_str!("../../../docs/checkpoints/play/play.json")),
                parity(&c, include_str!("../../../docs/checkpoints/water-play/water-play.json")),
                parity(&c, COAST_PART_BUILT),
                parity(&c, include_str!("../../../docs/checkpoints/coast/coast.json")),
                parity_of(&c, &rail_ends_world(&c)),
            ],
        })
    }

    #[test]
    fn schema_matches_web_ui() {
        let pretty = |v: &Value| format!("{}\n", serde_json::to_string_pretty(v).unwrap());
        let (schema, parity) = (pretty(&schema()), pretty(&parity_worlds()));
        if std::env::var_os("HEXW_UPDATE_SCHEMA").is_some() {
            std::fs::write(SCHEMA_PATH, &schema).unwrap();
            std::fs::write(PARITY_PATH, &parity).unwrap();
            return;
        }
        let stale =
            "is stale: run `make ui-schema`, then update web/ui/src until `make ui-test` passes";
        assert!(COMMITTED == schema, "web/ui/src/bridge-schema.json {stale}");
        assert!(
            COMMITTED_PARITY == parity,
            "web/ui/test/rules-parity.json {stale}"
        );
    }

    #[test]
    fn cell_rect_projects_the_origin_cell_around_the_view_centre() {
        let proj = PerspectiveProjection {
            aspect_ratio: 1.6,
            ..Default::default()
        };
        let t = Transform::from_xyz(0.0, 80.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y);
        let gt = GlobalTransform::from(t);
        let view = View {
            clip_from_world: proj.get_clip_from_view() * gt.to_matrix().inverse(),
            origin: Vec2::ZERO,
            size: Vec2::new(1280.0, 800.0),
        };
        let [l, t, r, b] = cell_rect(&view, Axial::ORIGIN).unwrap();
        assert!(
            l < 640.0 && r > 640.0 && t < 400.0 && b > 400.0,
            "{l} {t} {r} {b}"
        );
        // East is to the right on screen from a camera south of the world.
        let [el, ..] = cell_rect(&view, Axial::new(1, 0)).unwrap();
        assert!(el > l);
        // Behind the camera.
        assert!(cell_rect(&view, Axial::new(0, 40)).is_none());
    }
}
