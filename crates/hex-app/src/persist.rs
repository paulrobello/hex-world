//! Saves and loads (PRD §12.2, D9): the `save`, `load` and `new_world` commands, the `.hexworld`
//! bytes behind `hexWorld.takeSave(tag)` and `hexWorld.load(bytes)` on the web, and `--save` and
//! `--load` natively. The byte format is hex-core's (`docs/save-format.md`); this module maps it to
//! the running game: the world and its history, the orbit camera and the settings.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;
use hex_core::{
    AudioVolumes, CameraState, GameSettings, QualitySetting, SAVE_FORMAT_VERSION, SaveData,
    SaveError, TileCatalog, World,
};
use hex_render::post::QualityTier;
use hex_render::tilt_shift::{TiltPinned, TiltShiftSettings};
use serde::{Deserialize, Serialize};

use crate::RunFlags;
use crate::camera::{self, MAX_DISTANCE, MAX_PITCH, MIN_DISTANCE, MIN_PITCH};
use crate::control::{self, Command};
use crate::game::{self, GameEvent, GameState};

/// Audio bus gains as the bridge spells them (the save's `sfx` and `ui` are the web audio layer's
/// `effects` and `interface` buses).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Volumes {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
    pub ui: f32,
}

impl From<AudioVolumes> for Volumes {
    fn from(v: AudioVolumes) -> Self {
        Self {
            master: v.master,
            music: v.music,
            sfx: v.sfx,
            ambience: v.ambience,
            ui: v.ui,
        }
    }
}

impl From<Volumes> for AudioVolumes {
    fn from(v: Volumes) -> Self {
        let g = |x: f32| {
            if x.is_finite() {
                x.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        Self {
            master: g(v.master),
            music: g(v.music),
            sfx: g(v.sfx),
            ambience: g(v.ambience),
            ui: g(v.ui),
        }
    }
}

/// The settings a `loaded` event hands the overlay to apply.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsJson {
    pub quality: &'static str,
    pub tilt_shift: bool,
    pub tilt_shift_strength: f32,
    pub volumes: Volumes,
}

/// The `set_tier` spelling of a quality setting (`auto` or a tier name).
pub fn quality_setting(name: &str) -> Option<QualitySetting> {
    Some(match name {
        "auto" => QualitySetting::Auto,
        "low" => QualitySetting::Low,
        "medium" => QualitySetting::Medium,
        "high" => QualitySetting::High,
        "ultra" => QualitySetting::Ultra,
        _ => return None,
    })
}

/// The tier a quality setting asks for; `None` is Auto (detection's answer).
pub fn quality_tier(q: QualitySetting) -> Option<QualityTier> {
    match q {
        QualitySetting::Auto => None,
        QualitySetting::Low => Some(QualityTier::Low),
        QualitySetting::Medium => Some(QualityTier::Medium),
        QualitySetting::High => Some(QualityTier::High),
        QualitySetting::Ultra => Some(QualityTier::Ultra),
    }
}

fn quality_name(q: QualitySetting) -> &'static str {
    match q {
        QualitySetting::Auto => "auto",
        QualitySetting::Low => "low",
        QualitySetting::Medium => "medium",
        QualitySetting::High => "high",
        QualitySetting::Ultra => "ultra",
    }
}

/// Saved settings with no live owner in the engine: the quality choice (stored, not applied until
/// a quality setter exists) and the audio volumes the overlay sends with each `save`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SavedSettings {
    pub quality: QualitySetting,
    pub volumes: AudioVolumes,
}

impl Default for SavedSettings {
    /// The web audio layer's defaults (`web/ui/src/audio.ts`).
    fn default() -> Self {
        Self {
            quality: QualitySetting::Auto,
            volumes: AudioVolumes {
                master: 0.8,
                music: 0.55,
                sfx: 0.75,
                ambience: 0.7,
                ui: 0.5,
            },
        }
    }
}

/// Save, load and new-world commands in queue order, handed over by `control::run_commands`.
#[derive(Resource, Default)]
pub struct PersistQueue(pub VecDeque<Command>);

/// Saves validated by `hexWorld.load(bytes)` and waiting for their queued `load` command, with
/// their file's format version.
static PENDING_LOADS: Mutex<VecDeque<(SaveData, u16)>> = Mutex::new(VecDeque::new());

/// Encoded saves by tag, taken by `hexWorld.takeSave(tag)`. A tag's newer save replaces its older
/// one, and the oldest drop past the cap, so untaken saves cannot pile up.
static SAVED: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());
const SAVED_CAP: usize = 8;

/// The running game as a save, kept current every frame so a page hide can encode it at once
/// (`hexWorld.saveNow`), without the frame a queued `save` command waits for. The world is copied
/// only when `GameState::world_rev` moves.
struct Latest {
    world_rev: u64,
    save: Option<SaveData>,
}
static LATEST: Mutex<Latest> = Mutex::new(Latest {
    world_rev: 0,
    save: None,
});

/// Why a `.hexworld` file was refused, as the `loadRefused` event carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub reason: String,
    pub version: Option<u16>,
}

impl Refusal {
    fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            version: None,
        }
    }

    pub fn event(&self) -> GameEvent {
        GameEvent::LoadRefused {
            code: self.code,
            reason: self.reason.clone(),
            version: self.version,
        }
    }
}

impl From<&SaveError> for Refusal {
    fn from(e: &SaveError) -> Self {
        let code = match e {
            SaveError::TooLarge(_) => "too_large",
            SaveError::BadMagic => "bad_magic",
            SaveError::UnsupportedVersion(v) if *v > SAVE_FORMAT_VERSION => "newer_version",
            SaveError::UnsupportedVersion(_) => "unsupported_version",
            SaveError::Truncated { .. } => "truncated",
            SaveError::TrailingBytes(_) => "trailing_bytes",
            SaveError::ChecksumMismatch { .. } => "checksum",
            SaveError::Decode(_) => "decode",
            SaveError::InvalidValue(_) => "invalid_value",
            SaveError::InvalidWorld(_) => "invalid_world",
            SaveError::InvalidHistory(_) => "invalid_history",
        };
        let version = match e {
            SaveError::UnsupportedVersion(v) => Some(*v),
            _ => None,
        };
        Self {
            code,
            reason: e.to_string(),
            version,
        }
    }
}

/// Validates `.hexworld` bytes against the built-in catalog and migrates them. Returns the save
/// and the file's format version.
pub fn decode(bytes: &[u8]) -> Result<(SaveData, u16), Refusal> {
    let catalog = TileCatalog::builtin().expect("the embedded catalog is validated by hex-core");
    let save = SaveData::decode(bytes, &catalog).map_err(|e| Refusal::from(&e))?;
    // A decoded file has a full header, so the version field is there.
    Ok((save, u16::from_le_bytes([bytes[8], bytes[9]])))
}

/// The orbit pose of a camera transform.
pub fn camera_state(t: &Transform) -> CameraState {
    let focus = camera::focus(t);
    let (yaw, pitch, distance) = camera::spherical(t.translation - focus);
    CameraState {
        focus: focus.to_array(),
        yaw,
        pitch,
        distance,
    }
}

/// The camera transform for a saved orbit pose, inside the rig's pitch and zoom limits.
pub fn camera_transform(c: &CameraState) -> Transform {
    camera::place(
        Vec3::from_array(c.focus),
        c.yaw,
        c.pitch.clamp(MIN_PITCH, MAX_PITCH),
        c.distance.clamp(MIN_DISTANCE, MAX_DISTANCE),
    )
}

/// The default orbit when there is no camera to read (a headless run).
fn default_camera() -> CameraState {
    CameraState {
        focus: [0.0, 0.0, 0.0],
        yaw: 0.0,
        pitch: 0.8,
        distance: 120.0,
    }
}

/// Everything the save holds, read from the running game.
pub fn snapshot(
    game: &GameState,
    camera: Option<&Transform>,
    tilt: &TiltShiftSettings,
    saved: &SavedSettings,
) -> SaveData {
    SaveData {
        world: game.world.clone(),
        unlocks: Default::default(),
        camera: camera.map(camera_state).unwrap_or_else(default_camera),
        settings: GameSettings {
            quality: saved.quality,
            tilt_shift: tilt.enabled,
            tilt_shift_strength: tilt.strength,
            volumes: saved.volumes,
        },
    }
}

/// Puts a decoded save into the running game and returns its `loaded` event. Agents re-derive
/// from the new world (`sim::mirror_world` follows the game resource).
pub fn apply(
    save: SaveData,
    version: u16,
    game: &mut GameState,
    camera: Option<&mut Transform>,
    tilt: &mut TiltShiftSettings,
    saved: &mut SavedSettings,
) -> GameEvent {
    if let Some(t) = camera {
        *t = camera_transform(&save.camera);
    }
    let s = save.settings;
    tilt.enabled = s.tilt_shift;
    tilt.strength = s.tilt_shift_strength.clamp(0.0, 1.0);
    saved.quality = s.quality;
    saved.volumes = s.volumes;
    let event = GameEvent::Loaded {
        tiles: tile_count(&save.world),
        version,
        settings: Some(SettingsJson {
            quality: quality_name(s.quality),
            tilt_shift: s.tilt_shift,
            tilt_shift_strength: s.tilt_shift_strength,
            volumes: s.volumes.into(),
        }),
    };
    game.replace_world(save.world, event.clone());
    event
}

/// Resets to the start world. The camera keeps its angle and zoom but moves back over the origin.
pub fn new_world(game: &mut GameState, camera: Option<&mut Transform>, seed: u64) -> GameEvent {
    if let Some(t) = camera {
        let f = camera::focus(t);
        t.translation -= Vec3::new(f.x, 0.0, f.z);
    }
    let event = GameEvent::Loaded {
        tiles: 1,
        version: SAVE_FORMAT_VERSION,
        settings: None,
    };
    game.replace_world(World::new(seed), event.clone());
    event
}

fn tile_count(world: &World) -> u32 {
    u32::try_from(world.tiles().len()).unwrap_or(u32::MAX)
}

/// Keeps `bytes` for `hexWorld.takeSave(tag)`.
pub fn store_save(tag: &str, bytes: Vec<u8>) {
    let mut saved = SAVED.lock().unwrap();
    saved.retain(|(t, _)| t != tag);
    saved.push((tag.to_owned(), bytes));
    while saved.len() > SAVED_CAP {
        saved.remove(0);
    }
}

/// Removes and returns the bytes saved under `tag`, once its `saved` event has fired.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn take_save(tag: &str) -> Option<Vec<u8>> {
    let mut saved = SAVED.lock().unwrap();
    let i = saved.iter().position(|(t, _)| t == tag)?;
    Some(saved.remove(i).1)
}

/// The web's `hexWorld.load(bytes)`: validates at once, so a refused file is answered before the
/// call returns, then queues a `load` command behind every command already sent, so loads and
/// play commands keep their order. A refusal is also published as a `loadRefused` event.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn request_load(bytes: &[u8]) -> Result<(u32, u16), Refusal> {
    match decode(bytes) {
        Ok((save, version)) => {
            let tiles = tile_count(&save.world);
            PENDING_LOADS.lock().unwrap().push_back((save, version));
            control::INBOX
                .lock()
                .unwrap()
                .push(Command::Load { path: None });
            Ok((tiles, version))
        }
        Err(r) => {
            warn!("load refused ({}): {}", r.code, r.reason);
            game::publish_event(&r.event());
            Err(r)
        }
    }
}

/// [`request_load`]'s answer as the JSON `hexWorld.load` returns.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn load_result_json(r: &Result<(u32, u16), Refusal>) -> String {
    match r {
        Ok((tiles, version)) => {
            serde_json::json!({ "ok": true, "tiles": tiles, "version": version }).to_string()
        }
        Err(e) => serde_json::json!({
            "ok": false, "code": e.code, "reason": e.reason, "version": e.version,
        })
        .to_string(),
    }
}

/// Where a `load` command finds its save.
fn load_source(path: Option<&str>) -> Result<(SaveData, u16), Refusal> {
    match path {
        #[cfg(not(target_arch = "wasm32"))]
        Some(p) => {
            let bytes = std::fs::read(p)
                .map_err(|e| Refusal::new("io", format!("cannot read {p}: {e}")))?;
            decode(&bytes)
        }
        #[cfg(target_arch = "wasm32")]
        Some(p) => Err(Refusal::new(
            "io",
            format!("the web build has no file system ({p}); pass the bytes to hexWorld.load"),
        )),
        None => PENDING_LOADS
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Refusal::new("no_file", "no file was given to load")),
    }
}

/// Encodes the running game; natively writes `path` when given.
fn save_bytes(
    game: &GameState,
    camera: Option<&Transform>,
    tilt: &TiltShiftSettings,
    saved: &SavedSettings,
    path: Option<&str>,
) -> Result<Vec<u8>, String> {
    let bytes = snapshot(game, camera, tilt, saved).encode();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = path {
        std::fs::write(p, &bytes).map_err(|e| format!("cannot write {p}: {e}"))?;
        info!("wrote {p} ({} bytes)", bytes.len());
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(p) = path {
        warn!("save: the web build has no file system, {p} ignored");
    }
    Ok(bytes)
}

/// One save, load or new-world command against the running game, returning its event.
pub fn handle(
    cmd: Command,
    game: &mut GameState,
    camera: Option<&mut Transform>,
    tilt: &mut TiltShiftSettings,
    saved: &mut SavedSettings,
    seed: u64,
) -> GameEvent {
    match cmd {
        Command::Save { tag, path, volumes } => {
            if let Some(v) = volumes {
                saved.volumes = v.into();
            }
            match save_bytes(game, camera.as_deref(), tilt, saved, path.as_deref()) {
                Ok(bytes) => {
                    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                    store_save(tag.as_deref().unwrap_or(""), bytes);
                    GameEvent::Saved {
                        tag,
                        tiles: tile_count(&game.world),
                        bytes: len,
                    }
                }
                Err(reason) => {
                    warn!("save failed: {reason}");
                    GameEvent::ActionRefused {
                        action: "save",
                        reason,
                    }
                }
            }
        }
        Command::Load { path } => match load_source(path.as_deref()) {
            Ok((save, version)) => apply(save, version, game, camera, tilt, saved),
            Err(r) => {
                warn!("load refused ({}): {}", r.code, r.reason);
                r.event()
            }
        },
        Command::NewWorld => new_world(game, camera, seed),
        other => unreachable!("not a persistence command: {other:?}"),
    }
}

/// Runs the queued persistence commands after the frame's game actions (see `main`'s chain). A
/// loaded save's quality is applied (a `tier` key still wins) and its tilt-shift counts as the
/// player's choice, so the tier rebuild that follows keeps it.
pub fn run(
    mut commands: Commands,
    mut queue: ResMut<PersistQueue>,
    mut game: ResMut<GameState>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
    (mut tilt, mut pinned): (ResMut<TiltShiftSettings>, ResMut<TiltPinned>),
    mut saved: ResMut<SavedSettings>,
    flags: Res<RunFlags>,
) {
    while let Some(cmd) = queue.0.pop_front() {
        let mut cam = cameras.iter_mut().next();
        let event = handle(
            cmd,
            &mut game,
            cam.as_deref_mut(),
            &mut tilt,
            &mut saved,
            flags.0.seed,
        );
        if let GameEvent::Loaded {
            settings: Some(_), ..
        } = &event
        {
            pinned.0 = true;
            hex_render::tier::request(
                &mut commands,
                quality_tier(saved.quality),
                "loaded save".into(),
                true,
            );
        }
        game::publish_event(&event);
    }
}

/// Keeps [`LATEST`] current, after the frame's game actions and persistence commands.
pub fn keep_latest(
    game: Res<GameState>,
    cameras: Query<&Transform, With<Camera3d>>,
    tilt: Res<TiltShiftSettings>,
    saved: Res<SavedSettings>,
) {
    let camera = cameras
        .iter()
        .next()
        .map(camera_state)
        .unwrap_or_else(default_camera);
    let settings = GameSettings {
        quality: saved.quality,
        tilt_shift: tilt.enabled,
        tilt_shift_strength: tilt.strength,
        volumes: saved.volumes,
    };
    let mut latest = LATEST.lock().unwrap();
    let fresh = latest.world_rev == game.world_rev;
    match latest.save.as_mut() {
        Some(s) if fresh => {
            s.camera = camera;
            s.settings = settings;
        }
        _ => {
            latest.world_rev = game.world_rev;
            latest.save = Some(SaveData {
                world: game.world.clone(),
                unlocks: Default::default(),
                camera,
                settings,
            });
        }
    }
}

/// The `.hexworld` bytes of the world as of the last frame, with the overlay's `volumes`; `None`
/// before the first frame. For a page hide, which cannot wait for a queued `save` command.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn save_now(volumes: Option<Volumes>) -> Option<Vec<u8>> {
    let mut latest = LATEST.lock().unwrap();
    let save = latest.save.as_mut()?;
    if let Some(v) = volumes {
        save.settings.volumes = v.into();
    }
    Some(save.encode())
}

/// `--save <path>`: writes the world on every exit path, beside `--dump-state`.
pub fn save_on_exit(
    flags: Res<RunFlags>,
    mut exits: MessageReader<AppExit>,
    game: Res<GameState>,
    cameras: Query<&Transform, With<Camera3d>>,
    tilt: Res<TiltShiftSettings>,
    saved: Res<SavedSettings>,
) {
    if exits.read().next().is_none() {
        return;
    }
    if let Some(path) = &flags.0.save
        && let Err(e) = save_bytes(&game, cameras.iter().next(), &tilt, &saved, Some(path))
    {
        warn!("--save: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAVED and PENDING_LOADS are process-global and tests run in parallel: the cap test's 11 saves
    // evict other tests' entries, so every test that touches them holds this lock.
    static OUTBOX: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn outbox() -> std::sync::MutexGuard<'static, ()> {
        OUTBOX.lock().unwrap_or_else(|e| e.into_inner())
    }
    use crate::actions::Action;
    use hex_core::Axial;

    fn ax(q: i32, r: i32) -> Axial {
        Axial::new(q, r)
    }

    /// A world with placements, a bulldoze, an undo (so both stacks hold actions) and a camera.
    fn played() -> (GameState, Transform) {
        let mut g = GameState::new(7);
        for (id, cell) in [
            ("meadow", ax(1, 0)),
            ("forest", ax(-1, 0)),
            ("road_straight", ax(0, 1)),
            ("meadow", ax(2, 0)),
        ] {
            g.apply(Action::SelectTile(id.into()));
            g.apply(Action::Place(cell));
        }
        g.apply(Action::Bulldoze(ax(2, 0)));
        g.apply(Action::Undo);
        assert!(g.world.can_undo() && g.world.can_redo());
        let cam = camera::place(Vec3::new(30.0, 0.0, -12.0), 0.7, 0.9, 140.0);
        (g, cam)
    }

    fn save_cmd(tag: &str, volumes: Option<Volumes>) -> Command {
        Command::Save {
            tag: Some(tag.into()),
            path: None,
            volumes,
        }
    }

    #[test]
    fn save_then_load_through_the_commands_keeps_world_history_camera_and_settings() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = TiltShiftSettings {
            enabled: false,
            strength: 0.35,
            ..Default::default()
        };
        let mut saved = SavedSettings::default();
        let volumes = Volumes {
            master: 0.6,
            music: 0.1,
            sfx: 0.9,
            ambience: 0.4,
            ui: 0.2,
        };
        let e = handle(
            save_cmd("rt-a", Some(volumes)),
            &mut g,
            Some(&mut cam),
            &mut tilt,
            &mut saved,
            7,
        );
        assert!(matches!(&e, GameEvent::Saved { tag: Some(t), tiles: 5, .. } if t == "rt-a"));
        let bytes = take_save("rt-a").expect("the save waits under its tag");
        assert!(take_save("rt-a").is_none(), "taking removes it");

        // Load into a fresh game, camera and settings, the way a page reload does.
        let mut g2 = GameState::new(7);
        let mut cam2 = Transform::from_xyz(0.0, 90.0, 90.0).looking_at(Vec3::ZERO, Vec3::Y);
        let mut tilt2 = TiltShiftSettings::default();
        let mut saved2 = SavedSettings::default();
        let (save, version) = decode(&bytes).unwrap();
        let e = apply(
            save,
            version,
            &mut g2,
            Some(&mut cam2),
            &mut tilt2,
            &mut saved2,
        );
        let GameEvent::Loaded {
            tiles,
            version,
            settings: Some(s),
        } = &e
        else {
            panic!("{e:?}")
        };
        assert_eq!((*tiles, *version), (5, SAVE_FORMAT_VERSION));
        assert_eq!(s.volumes, volumes);
        assert_eq!(
            g2.world, g.world,
            "tiles, rotations, seeds and both history stacks"
        );
        assert_eq!(g2.recent.back(), Some(&e));
        assert!(
            cam2.translation.distance(cam.translation) < 1e-3,
            "camera position"
        );
        assert!(
            cam2.forward().dot(*cam.forward()) > 0.9999,
            "camera direction"
        );
        assert_eq!((tilt2.enabled, tilt2.strength), (false, 0.35));
        assert_eq!(saved2.volumes, AudioVolumes::from(volumes));

        // The loaded history replays: undo and redo act exactly as they would have before.
        let (mut a, mut b) = (g.world.clone(), g2.world.clone());
        assert!(a.redo() && b.redo());
        assert_eq!(a, b);
        for _ in 0..5 {
            assert_eq!(a.undo(), b.undo());
            assert_eq!(a, b);
        }
    }

    #[test]
    fn bad_files_are_refused_with_typed_reasons_and_leave_the_world_alone() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = TiltShiftSettings::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("bad-src", None),
            &mut g,
            Some(&mut cam),
            &mut tilt,
            &mut saved,
            7,
        );
        let good = take_save("bad-src").unwrap();
        let mut magic = good.clone();
        magic[0] = b'X';
        let mut newer = good.clone();
        newer[8..10].copy_from_slice(&(SAVE_FORMAT_VERSION + 1).to_le_bytes());
        let mut trailing = good.clone();
        trailing.push(0);
        let mut flipped = good.clone();
        let mid = flipped.len() / 2;
        flipped[mid] ^= 0x10;
        let cases: [(&str, Vec<u8>, &str); 6] = [
            ("bad magic", magic, "bad_magic"),
            ("a zip", b"PK\x03\x04 not a world".to_vec(), "bad_magic"),
            ("truncated", good[..good.len() - 3].to_vec(), "truncated"),
            ("newer version", newer, "newer_version"),
            ("trailing bytes", trailing, "trailing_bytes"),
            ("bit flip", flipped, "checksum"),
        ];
        for (what, bytes, code) in cases {
            let r = decode(&bytes).expect_err(what);
            assert_eq!(r.code, code, "{what}: {}", r.reason);
            assert!(!r.reason.is_empty());
            if code == "newer_version" {
                assert_eq!(r.version, Some(SAVE_FORMAT_VERSION + 1));
                assert!(r.reason.contains("not supported"), "{}", r.reason);
            }
            let json: serde_json::Value = serde_json::from_str(&load_result_json(&Err(r))).unwrap();
            assert_eq!(json["ok"], false);
            assert_eq!(json["code"], code);
        }
    }

    /// The web path: `hexWorld.load(bytes)` validates at once, then queues a `load` command that
    /// applies the pending save in order. The only test touching the pending queue and the inbox.
    #[test]
    fn web_load_validates_at_once_and_applies_in_queue_order() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = TiltShiftSettings::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("web-src", None),
            &mut g,
            Some(&mut cam),
            &mut tilt,
            &mut saved,
            7,
        );
        let good = take_save("web-src").unwrap();

        let r = request_load(&good[..20]);
        assert!(
            matches!(
                &r,
                Err(Refusal {
                    code: "truncated",
                    ..
                })
            ),
            "{r:?}"
        );
        assert!(
            PENDING_LOADS.lock().unwrap().is_empty(),
            "a refused file queues nothing"
        );

        let r = request_load(&good);
        assert_eq!(r, Ok((5, SAVE_FORMAT_VERSION)));
        let json: serde_json::Value = serde_json::from_str(&load_result_json(&r)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "ok": true, "tiles": 5, "version": 1 })
        );
        let queued: Vec<Command> = control::INBOX.lock().unwrap().drain(..).collect();
        assert_eq!(queued, [Command::Load { path: None }]);

        let mut g2 = GameState::new(7);
        let load = || Command::Load { path: None };
        let e = handle(load(), &mut g2, None, &mut tilt, &mut saved, 7);
        assert!(matches!(e, GameEvent::Loaded { tiles: 5, .. }), "{e:?}");
        assert_eq!(g2.world, g.world);
        // Nothing left pending: a second load is refused and the world is untouched.
        let e = handle(load(), &mut g2, None, &mut tilt, &mut saved, 7);
        assert!(
            matches!(
                e,
                GameEvent::LoadRefused {
                    code: "no_file",
                    ..
                }
            ),
            "{e:?}"
        );
        assert_eq!(g2.world, g.world);
    }

    #[test]
    fn load_and_save_files_natively() {
        let dir = std::env::temp_dir().join(format!("hexw-persist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.hexworld").to_string_lossy().into_owned();
        let (mut g, mut cam) = played();
        let mut tilt = TiltShiftSettings::default();
        let mut saved = SavedSettings::default();
        let cmd = Command::Save {
            tag: None,
            path: Some(path.clone()),
            volumes: None,
        };
        let e = handle(cmd, &mut g, Some(&mut cam), &mut tilt, &mut saved, 7);
        assert!(matches!(e, GameEvent::Saved { tiles: 5, .. }), "{e:?}");
        let mut g2 = GameState::new(7);
        let mut cam2 = Transform::default();
        let e = handle(
            Command::Load {
                path: Some(path.clone()),
            },
            &mut g2,
            Some(&mut cam2),
            &mut tilt,
            &mut saved,
            7,
        );
        assert!(matches!(e, GameEvent::Loaded { tiles: 5, .. }), "{e:?}");
        assert_eq!(g2.world, g.world);
        let missing = dir.join("missing.hexworld").to_string_lossy().into_owned();
        let e = handle(
            Command::Load {
                path: Some(missing),
            },
            &mut g2,
            None,
            &mut tilt,
            &mut saved,
            7,
        );
        assert!(
            matches!(e, GameEvent::LoadRefused { code: "io", .. }),
            "{e:?}"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn new_world_resets_to_the_start_tile_and_recentres_the_camera() {
        let (mut g, mut cam) = played();
        let e = new_world(&mut g, Some(&mut cam), 7);
        assert!(matches!(
            e,
            GameEvent::Loaded {
                tiles: 1,
                settings: None,
                ..
            }
        ));
        assert_eq!(g.world, World::new(7));
        assert!(!g.world.can_undo() && !g.world.can_redo());
        let f = camera::focus(&cam);
        assert!(f.x.abs() < 1e-3 && f.z.abs() < 1e-3, "{f}");
    }

    /// The page-hide path: the latest world, camera and settings encode at once, and a change to
    /// the world shows in the next frame's copy. The only test touching `LATEST`.
    #[test]
    fn save_now_encodes_the_latest_frame_without_a_queued_command() {
        use bevy::ecs::system::RunSystemOnce;
        let (game, cam) = played();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(game);
        world.insert_resource(TiltShiftSettings::default());
        world.insert_resource(SavedSettings::default());
        world.spawn((Camera3d::default(), cam));
        world.run_system_once(keep_latest).unwrap();
        let volumes = Volumes {
            master: 0.3,
            music: 0.2,
            sfx: 0.1,
            ambience: 0.4,
            ui: 0.5,
        };
        let (save, _) = decode(&save_now(Some(volumes)).expect("a frame ran")).unwrap();
        assert_eq!(save.world, world.resource::<GameState>().world);
        assert!(
            camera_transform(&save.camera)
                .translation
                .distance(cam.translation)
                < 1e-2
        );
        assert_eq!(save.settings.volumes, AudioVolumes::from(volumes));

        world
            .resource_mut::<GameState>()
            .apply(Action::Bulldoze(ax(-1, 0)));
        world.run_system_once(keep_latest).unwrap();
        let (save, _) = decode(&save_now(None).unwrap()).unwrap();
        assert!(
            save.world.tile(ax(-1, 0)).is_none(),
            "the next frame copies the changed world"
        );
        assert_eq!(save.world, world.resource::<GameState>().world);
    }

    #[test]
    fn a_replaced_world_moves_the_view_epoch_and_a_placement_does_not() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let before = g.view().epoch;
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Place(ax(1, 1)));
        assert_eq!(g.view().epoch, before, "a fresh placement rises as usual");
        let mut tilt = TiltShiftSettings::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("epoch", None),
            &mut g,
            Some(&mut cam),
            &mut tilt,
            &mut saved,
            7,
        );
        let (save, v) = decode(&take_save("epoch").unwrap()).unwrap();
        apply(save, v, &mut g, Some(&mut cam), &mut tilt, &mut saved);
        assert_eq!(
            g.view().epoch,
            before + 1,
            "a load redraws the world at rest"
        );
        new_world(&mut g, Some(&mut cam), 7);
        assert_eq!(g.view().epoch, before + 2, "so does a new world");
    }

    #[test]
    fn camera_state_round_trips_and_clamps_to_the_rig() {
        let t = camera::place(Vec3::new(-40.0, 0.0, 25.0), -2.1, 0.6, 333.0);
        let back = camera_transform(&camera_state(&t));
        assert!(back.translation.distance(t.translation) < 1e-2);
        let wild = CameraState {
            focus: [0.0; 3],
            yaw: 0.0,
            pitch: 3.0,
            distance: 1e9,
        };
        let c = camera_state(&camera_transform(&wild));
        assert!(c.pitch <= MAX_PITCH + 1e-3 && c.distance <= MAX_DISTANCE + 1e-1);
    }

    #[test]
    fn set_tier_spellings_map_to_the_saved_quality_and_back() {
        for name in ["auto", "low", "medium", "high", "ultra"] {
            let q = quality_setting(name).expect(name);
            assert_eq!(quality_name(q), name);
            assert_eq!(
                quality_tier(q).map(QualityTier::name),
                (name != "auto").then_some(name)
            );
        }
        assert_eq!(quality_setting("max"), None);
    }

    #[test]
    fn saved_bytes_are_kept_per_tag_and_capped() {
        let _outbox = outbox();
        for i in 0..SAVED_CAP + 3 {
            store_save(&format!("cap-{i}"), vec![i as u8]);
        }
        store_save("cap-x", vec![1]);
        store_save("cap-x", vec![2]);
        assert_eq!(
            take_save("cap-x"),
            Some(vec![2]),
            "a newer save replaces its tag's"
        );
        assert!(SAVED.lock().unwrap().len() <= SAVED_CAP);
    }
}
