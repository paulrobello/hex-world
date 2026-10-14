//! wasm-bindgen exports behind window.hexWorld. Shared state is written by Bevy systems.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::math::{Mat4, Vec2, Vec3};
use hex_core::{Axial, HEX_RADIUS};

pub static READY: Mutex<bool> = Mutex::new(false);
/// Game events as JSON objects, oldest first, drained by `hex_world_events` (`hexWorld.events()`).
pub static EVENTS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
pub static STATE_JSON: Mutex<String> = Mutex::new(String::new());
pub static DIAG_JSON: Mutex<String> = Mutex::new(String::new());
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
        *super::READY.lock().unwrap()
    }

    #[wasm_bindgen]
    pub fn hex_world_state_json() -> String {
        let s = super::STATE_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    #[wasm_bindgen]
    pub fn hex_world_diag_json() -> String {
        let s = super::DIAG_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    /// Drains the game event queue as a JSON array (`[]` when empty).
    #[wasm_bindgen]
    pub fn hex_world_events() -> String {
        let drained: Vec<String> = super::EVENTS.lock().unwrap().drain(..).collect();
        format!("[{}]", drained.join(","))
    }

    /// `[left, top, right, bottom]` in CSS pixels around cell `(q, r)`, or `null` before the
    /// camera exists or when the cell is behind it.
    #[wasm_bindgen]
    pub fn hex_world_cell_rect(q: i32, r: i32) -> String {
        let view = *super::VIEW.lock().unwrap();
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
                control::INBOX.lock().unwrap().extend(cmds);
                true
            }
            Err(e) => {
                web_sys::console::error_1(&format!("hexWorld.cmd: {e}").into());
                false
            }
        }
    }

    /// Queues a screenshot behind any pending commands; poll `hex_world_take_shot` for the result.
    #[wasm_bindgen]
    pub fn hex_world_screenshot(name: String) {
        capture::SHOTS.lock().unwrap().retain(|(n, _)| *n != name);
        control::INBOX
            .lock()
            .unwrap()
            .push(control::Command::Screenshot { name });
    }

    /// Removes and returns the PNG data URL for `name` once the capture has landed.
    #[wasm_bindgen]
    pub fn hex_world_take_shot(name: String) -> Option<String> {
        let mut shots = capture::SHOTS.lock().unwrap();
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
}

/// The bridge contract the web UI overlay is typed against (`web/ui/src/bridge-schema.json`):
/// command and event names with their fields, enum spellings, the `play` state keys, the tile
/// catalog with its socket table, and legal rotations for every frontier slot of four scripted
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
        r#"{"cmd":"set_reduced_motion","on":true}"#,
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
        r#"{"cmd":"photo","dof":"tilt","focus":64,"aperture":2.8,"exposure":0.3,"fov":50,"roll":0,"pause_agents":true}"#,
        r#"{"cmd":"photo_export","scale":2,"name":"hex-world"}"#,
        r#"{"cmd":"save","tag":"autosave","path":null,"volumes":{"master":0.8,"music":0.55,"sfx":0.75,"ambience":0.7,"ui":0.5}}"#,
        r#"{"cmd":"load","path":null}"#,
        r#"{"cmd":"new_world"}"#,
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
            Command::SetReducedMotion { .. } => "set_reduced_motion",
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
        ];
        let mut seen = [false; 21];
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
                GameEvent::TileReplaced { .. } => 20,
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
        g.apply(Action::SetTool(Tool::Bulldoze));
        g.apply(Action::Hover(Some(Axial::new(1, 0))));
        let bulldoze = g.state_json();
        let photo = crate::photo::state_json(
            &crate::photo::PhotoSettings::default(),
            &crate::photo::PhotoSession::default(),
            &crate::photo::PhotoExports::default(),
            false,
            0,
            None,
        );
        let grade = crate::control::grade_json(hex_render::Grade::default());
        json!({
            "grade": keys(&grade),
            "grade_preset": keys(&grade["presets"][0]),
            "play": keys(&place),
            "ghost": keys(&place["ghost"]),
            "bulldoze": keys(&bulldoze["bulldoze"]),
            "last_result": keys(&place["last_result"]),
            "tile": keys(&place["tiles"][0]),
            "rising": keys(&place["rising"][0]),
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
                json!({
                    "id": t.id,
                    "edges": t.edges.iter().map(|e| kind_name(e.kind)).collect::<Vec<_>>(),
                    "land": t.edges.iter().map(|e| serde_json::to_value(e.land).unwrap()).collect::<Vec<_>>(),
                    "scatter": t.props.scatter.iter().map(|s| s.piece.0.as_str()).collect::<Vec<_>>(),
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
            "compatible": compatible,
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

    /// Legal rotations of every catalog tile at every frontier slot of the world a script leaves.
    fn parity(c: &TileCatalog, script: &str) -> Value {
        let mut g = GameState::new(1);
        for cmd in parse_script(script).unwrap() {
            if let Some(a) = cmd.action() {
                g.apply(a);
            }
        }
        let w: &World = &g.world;
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
                "edge_status": edge_status_names(),
                "grade": hex_render::Grade::ALL.iter().map(|g| g.name()).collect::<Vec<_>>(),
            },
            "state": play_shapes(),
            "catalog": catalog(&c),
        })
    }

    fn parity_worlds() -> Value {
        let c = TileCatalog::builtin().unwrap();
        json!({
            "about": "Legal rotations of every tile at every frontier slot of four scripted worlds, from hex-core (make ui-schema). web/ui/test/rules.test.ts checks the overlay's fit rules against it.",
            "worlds": [
                parity(&c, include_str!("../../../docs/checkpoints/play/play.json")),
                parity(&c, include_str!("../../../docs/checkpoints/water-play/water-play.json")),
                parity(&c, COAST_PART_BUILT),
                parity(&c, include_str!("../../../docs/checkpoints/coast/coast.json")),
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
