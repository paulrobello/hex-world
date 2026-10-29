//! Saves and loads (PRD §12.2, D9): the `save`, `load` and `new_world` commands, the `.hexworld`
//! bytes behind `hexWorld.takeSave(tag)` and `hexWorld.load(bytes)` on the web, and `--save` and
//! `--load` natively. The byte format is hex-core's (`docs/save-format.md`); this module maps it to
//! the running game: the world and its history, the orbit camera and the settings.
use std::collections::VecDeque;
use std::sync::Mutex;

use bevy::prelude::*;
use hex_core::{GameSettings, SAVE_FORMAT_VERSION, SaveData, SaveError, TileCatalog, World};
use hex_render::tilt_shift::{TiltPinned, TiltShiftSettings, TiltTierDefault};

use crate::RunFlags;
use crate::camera;
use crate::control::{self, Command};
use crate::game::{self, GameEvent, GameState};
use crate::photo::PhotoSession;

mod settings;
pub use settings::{
    SavedSettings, SettingsJson, Volumes, camera_state, camera_transform, quality_setting,
    quality_tier,
};
use settings::{default_camera, quality_name};

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

/// The running game's tilt-shift: the settings, whether the player chose them (`TiltPinned`),
/// and the tier's default.
pub struct TiltLive<'a> {
    pub settings: &'a mut TiltShiftSettings,
    pub chosen: &'a mut bool,
    pub tier_default: Option<TiltShiftSettings>,
}

/// Everything the save holds, read from the running game. `tilt_chosen` is whether the player
/// toggled tilt-shift themselves.
pub fn snapshot(
    game: &GameState,
    camera: Option<&Transform>,
    tilt: &TiltShiftSettings,
    tilt_chosen: bool,
    saved: &SavedSettings,
) -> SaveData {
    SaveData {
        world: game.world.clone(),
        unlocks: Default::default(),
        camera: camera.map(camera_state).unwrap_or_else(default_camera),
        settings: GameSettings {
            quality: saved.quality,
            tilt_shift: tilt.enabled,
            tilt_shift_chosen: tilt_chosen,
            tilt_shift_strength: tilt.strength,
            volumes: saved.volumes,
        },
    }
}

/// Puts a decoded save into the running game and returns its `loaded` event. Agents re-derive
/// from the new world (`sim::mirror_world` follows the game resource). A save whose tilt-shift
/// the player chose restores it and the choice; any other save puts tilt-shift back to the
/// tier's default and leaves it unchosen, so tier changes keep driving it.
pub fn apply(
    save: SaveData,
    version: u16,
    game: &mut GameState,
    camera: Option<&mut Transform>,
    tilt: TiltLive,
    saved: &mut SavedSettings,
) -> GameEvent {
    if let Some(t) = camera {
        *t = camera_transform(&save.camera);
    }
    let s = save.settings;
    *tilt.chosen = s.tilt_shift_chosen;
    if s.tilt_shift_chosen {
        tilt.settings.enabled = s.tilt_shift;
        tilt.settings.strength = s.tilt_shift_strength.clamp(0.0, 1.0);
    } else if let Some(d) = tilt.tier_default {
        tilt.settings.enabled = d.enabled;
        tilt.settings.strength = d.strength;
    }
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

/// The web's `hexWorld.inspect(bytes)`: decodes `.hexworld` bytes as a load would, but changes
/// nothing (no load is queued and no `loadRefused` event fires), for the import preview and the
/// refusal checks. Returns JSON: `{"ok":true,"version":v,"supported":s,"tiles":[{q,r,tile,rotation,level}]}`
/// or `{"ok":false,"code":..,"reason":..,"version":..,"supported":s}`, where `supported` is the
/// newest format version this build reads.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn inspect_json(bytes: &[u8]) -> String {
    match decode(bytes) {
        Ok((save, version)) => {
            let tiles: Vec<serde_json::Value> = save
                .world
                .tiles()
                .iter()
                .map(|(c, t)| {
                    serde_json::json!({ "q": c.q, "r": c.r, "tile": t.def, "rotation": t.rotation, "level": t.level })
                })
                .collect();
            serde_json::json!({
                "ok": true, "version": version, "supported": SAVE_FORMAT_VERSION, "tiles": tiles,
            })
            .to_string()
        }
        Err(e) => serde_json::json!({
            "ok": false, "code": e.code, "reason": e.reason, "version": e.version,
            "supported": SAVE_FORMAT_VERSION,
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
    (tilt, tilt_chosen): (&TiltShiftSettings, bool),
    saved: &SavedSettings,
    path: Option<&str>,
) -> Result<Vec<u8>, String> {
    let bytes = snapshot(game, camera, tilt, tilt_chosen, saved).encode();
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
    tilt: TiltLive,
    saved: &mut SavedSettings,
    seed: u64,
) -> GameEvent {
    match cmd {
        Command::Save { tag, path, volumes } => {
            if let Some(v) = volumes {
                saved.volumes = v.into();
            }
            let t = (&*tilt.settings, *tilt.chosen);
            match save_bytes(game, camera.as_deref(), t, saved, path.as_deref()) {
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
/// loaded save's quality is applied (a `tier` key still wins). Its tilt-shift counts as the
/// player's choice only when the save says the player toggled it, so an autosave resume leaves
/// tier changes driving tilt-shift ([`apply`]).
///
/// Loading a world or starting a new one exits photo mode. `photo::sync` runs before this system
/// and restores the snapshot's transform on exit, so the snapshot takes the new world's camera.
#[allow(clippy::too_many_arguments)]
pub fn run(
    mut commands: Commands,
    mut queue: ResMut<PersistQueue>,
    mut game: ResMut<GameState>,
    mut cameras: Query<&mut Transform, hex_render::post::GameCamera>,
    (mut tilt, mut pinned, tier_default): (
        ResMut<TiltShiftSettings>,
        ResMut<TiltPinned>,
        Option<Res<TiltTierDefault>>,
    ),
    mut saved: ResMut<SavedSettings>,
    mut photo: ResMut<PhotoSession>,
    flags: Res<RunFlags>,
    mut walk: ResMut<crate::walk::WalkSession>,
) {
    while let Some(cmd) = queue.0.pop_front() {
        let mut cam = cameras.iter_mut().next();
        let in_photo = game.photo_mode;
        // Walking, saves take the orbit camera, never the walker's eye.
        let in_walk = game.walk_mode;
        let mut orbit = walk.orbit();
        // A new world keeps the orbit camera's angle and zoom, not the free camera's.
        if let (true, Some(snap), Some(t)) = (
            matches!(cmd, Command::NewWorld),
            photo
                .saved
                .map(|s| s.transform)
                .filter(|_| in_photo)
                .or(orbit),
            cam.as_deref_mut(),
        ) {
            *t = snap;
        }
        let save_cam = match cmd {
            Command::Save { .. } if orbit.is_some() => orbit.as_mut(),
            _ => cam.as_deref_mut(),
        };
        let event = handle(
            cmd,
            &mut game,
            save_cam,
            TiltLive {
                settings: &mut tilt,
                chosen: &mut pinned.0,
                tier_default: tier_default.as_deref().map(|d| d.0),
            },
            &mut saved,
            flags.0.seed,
        );
        let loaded = matches!(event, GameEvent::Loaded { .. });
        if let GameEvent::Loaded {
            settings: Some(_), ..
        } = &event
        {
            hex_play::tier::request(
                &mut commands,
                quality_tier(saved.quality),
                "loaded save".into(),
                true,
            );
        }
        game::publish_event(&event);
        if in_photo && loaded {
            if let (Some(snap), Some(t)) = (photo.saved.as_mut(), cam.as_deref()) {
                snap.transform = *t;
            }
            for e in game.apply(crate::actions::Action::ExitPhoto) {
                game::publish_event(&e);
            }
        }
        // Loading a world or starting a new one leaves walk mode at the loaded orbit camera.
        if in_walk && loaded {
            if let (Some(snap), Some(t)) = (walk.saved.as_mut(), cam.as_deref()) {
                snap.transform = *t;
            }
            for e in game.apply(crate::actions::Action::SetWalk(false)) {
                game::publish_event(&e);
            }
        }
    }
}

/// Keeps [`LATEST`] current, after the frame's game actions and persistence commands.
pub fn keep_latest(
    game: Res<GameState>,
    cameras: Query<&Transform, hex_render::post::GameCamera>,
    (tilt, chosen): (Res<TiltShiftSettings>, Res<TiltPinned>),
    saved: Res<SavedSettings>,
    walk: Res<crate::walk::WalkSession>,
) {
    let orbit = walk.orbit();
    let camera = orbit
        .as_ref()
        .or(cameras.iter().next())
        .map(camera_state)
        .unwrap_or_else(default_camera);
    let settings = GameSettings {
        quality: saved.quality,
        tilt_shift: tilt.enabled,
        tilt_shift_chosen: chosen.0,
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
    cameras: Query<&Transform, hex_render::post::GameCamera>,
    (tilt, chosen): (Res<TiltShiftSettings>, Res<TiltPinned>),
    saved: Res<SavedSettings>,
    walk: Res<crate::walk::WalkSession>,
) {
    if exits.read().next().is_none() {
        return;
    }
    let orbit = walk.orbit();
    if let Some(path) = &flags.0.save
        && let Err(e) = save_bytes(
            &game,
            orbit.as_ref().or(cameras.iter().next()),
            (&tilt, chosen.0),
            &saved,
            Some(path),
        )
    {
        warn!("--save: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{MAX_DISTANCE, MAX_PITCH};
    use hex_core::{AudioVolumes, CameraState};
    use hex_render::post::QualityTier;

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

    /// The tilt-shift a test game runs with: settings, the player's-choice flag, and the tier's
    /// default (on at 0.5, as High builds it in play).
    #[derive(Default)]
    struct Tilt {
        s: TiltShiftSettings,
        chosen: bool,
    }

    const TIER_DEFAULT: TiltShiftSettings = TiltShiftSettings {
        enabled: true,
        strength: 0.5,
        taps: 16,
    };

    impl Tilt {
        fn live(&mut self) -> TiltLive<'_> {
            TiltLive {
                settings: &mut self.s,
                chosen: &mut self.chosen,
                tier_default: Some(TIER_DEFAULT),
            }
        }
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

    /// `SavedSettings::default` mirrors the web audio layer's `DEFAULTS` (`web/ui/src/audio.ts`),
    /// whose `effects` and `interface` buses the save spells `sfx` and `ui`.
    #[test]
    fn default_volumes_match_the_web_audio_defaults() {
        const AUDIO_TS: &str = include_str!("../../../web/ui/src/audio.ts");
        let line = AUDIO_TS
            .lines()
            .find(|l| l.starts_with("const DEFAULTS: Volumes = {"))
            .expect("audio.ts declares DEFAULTS on one line");
        let web = |bus: &str| -> f32 {
            let at = line
                .find(&format!("{bus}: "))
                .unwrap_or_else(|| panic!("no {bus}"));
            let rest = &line[at + bus.len() + 2..];
            let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
            rest[..end].parse().unwrap_or_else(|e| panic!("{bus}: {e}"))
        };
        let v = SavedSettings::default().volumes;
        assert_eq!(
            [v.master, v.music, v.sfx, v.ambience, v.ui],
            [
                web("master"),
                web("music"),
                web("effects"),
                web("ambience"),
                web("interface")
            ]
        );
    }

    #[test]
    fn save_then_load_through_the_commands_keeps_world_history_camera_and_settings() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = Tilt {
            s: TiltShiftSettings {
                enabled: false,
                strength: 0.35,
                ..Default::default()
            },
            chosen: true,
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
            tilt.live(),
            &mut saved,
            7,
        );
        assert!(matches!(&e, GameEvent::Saved { tag: Some(t), tiles: 5, .. } if t == "rt-a"));
        let bytes = take_save("rt-a").expect("the save waits under its tag");
        assert!(take_save("rt-a").is_none(), "taking removes it");

        // Load into a fresh game, camera and settings, the way a page reload does.
        let mut g2 = GameState::new(7);
        let mut cam2 = Transform::from_xyz(0.0, 90.0, 90.0).looking_at(Vec3::ZERO, Vec3::Y);
        let mut tilt2 = Tilt::default();
        let mut saved2 = SavedSettings::default();
        let (save, version) = decode(&bytes).unwrap();
        let e = apply(
            save,
            version,
            &mut g2,
            Some(&mut cam2),
            tilt2.live(),
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
        assert_eq!((tilt2.s.enabled, tilt2.s.strength), (false, 0.35));
        assert!(tilt2.chosen, "the player's toggle travels with the save");
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
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("bad-src", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
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

    /// `hexWorld.inspect(bytes)`: the import preview's decode reports the tiles and versions of a
    /// good file and the typed refusal of a bad one, and queues nothing.
    #[test]
    fn inspect_reports_tiles_or_refusal_and_queues_nothing() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("inspect-src", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
            &mut saved,
            7,
        );
        let good = take_save("inspect-src").unwrap();
        let loads = PENDING_LOADS.lock().unwrap().len();
        let ok: serde_json::Value = serde_json::from_str(&inspect_json(&good)).unwrap();
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["version"], SAVE_FORMAT_VERSION);
        assert_eq!(ok["supported"], SAVE_FORMAT_VERSION);
        let tiles = ok["tiles"].as_array().unwrap();
        assert_eq!(tiles.len(), g.world.tiles().len());
        assert!(tiles[0]["tile"].is_string() && tiles[0]["rotation"].is_u64());
        let mut newer = good.clone();
        newer[8..10].copy_from_slice(&(SAVE_FORMAT_VERSION + 1).to_le_bytes());
        let bad: serde_json::Value = serde_json::from_str(&inspect_json(&newer)).unwrap();
        assert_eq!(bad["ok"], false);
        assert_eq!(bad["code"], "newer_version");
        assert_eq!(bad["version"], SAVE_FORMAT_VERSION + 1);
        assert_eq!(bad["supported"], SAVE_FORMAT_VERSION);
        assert_eq!(PENDING_LOADS.lock().unwrap().len(), loads);
    }

    /// The web path: `hexWorld.load(bytes)` validates at once, then queues a `load` command that
    /// applies the pending save in order. The only test touching the pending queue and the inbox.
    #[test]
    fn web_load_validates_at_once_and_applies_in_queue_order() {
        let _outbox = outbox();
        let (mut g, mut cam) = played();
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("web-src", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
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
            serde_json::json!({ "ok": true, "tiles": 5, "version": SAVE_FORMAT_VERSION })
        );
        let queued: Vec<Command> = control::INBOX.lock().unwrap().drain(..).collect();
        assert_eq!(queued, [Command::Load { path: None }]);

        let mut g2 = GameState::new(7);
        let load = || Command::Load { path: None };
        let e = handle(load(), &mut g2, None, tilt.live(), &mut saved, 7);
        assert!(matches!(e, GameEvent::Loaded { tiles: 5, .. }), "{e:?}");
        assert_eq!(g2.world, g.world);
        // Nothing left pending: a second load is refused and the world is untouched.
        let e = handle(load(), &mut g2, None, tilt.live(), &mut saved, 7);
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
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        let cmd = Command::Save {
            tag: None,
            path: Some(path.clone()),
            volumes: None,
        };
        let e = handle(cmd, &mut g, Some(&mut cam), tilt.live(), &mut saved, 7);
        assert!(matches!(e, GameEvent::Saved { tiles: 5, .. }), "{e:?}");
        let mut g2 = GameState::new(7);
        let mut cam2 = Transform::default();
        let e = handle(
            Command::Load {
                path: Some(path.clone()),
            },
            &mut g2,
            Some(&mut cam2),
            tilt.live(),
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
            tilt.live(),
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
        world.insert_resource(TiltPinned::default());
        world.insert_resource(SavedSettings::default());
        world.insert_resource(crate::walk::WalkSession::default());
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
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("epoch", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
            &mut saved,
            7,
        );
        let (save, v) = decode(&take_save("epoch").unwrap()).unwrap();
        apply(save, v, &mut g, Some(&mut cam), tilt.live(), &mut saved);
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

    /// Owner decision A21 (2026-10-14): a save the player never toggled tilt-shift in (the
    /// autosave resume) does not count as their choice, so the tier keeps driving tilt-shift.
    #[test]
    fn an_unchosen_save_leaves_the_tier_driving_tilt_shift() {
        let (mut g, mut cam) = played();
        let mut tilt = Tilt {
            s: TiltShiftSettings {
                enabled: false,
                strength: 0.2,
                taps: 16,
            },
            chosen: false,
        };
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("unchosen", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
            &mut saved,
            7,
        );
        let (save, v) = decode(&take_save("unchosen").unwrap()).unwrap();
        assert!(!save.settings.tilt_shift_chosen);

        // Resume into a session where the player had toggled tilt-shift off.
        let mut g2 = GameState::new(7);
        let mut tilt2 = Tilt {
            s: TiltShiftSettings {
                enabled: false,
                strength: 0.9,
                taps: 16,
            },
            chosen: true,
        };
        apply(save, v, &mut g2, None, tilt2.live(), &mut saved);
        assert!(!tilt2.chosen, "the resume is not the player's choice");
        assert_eq!(
            (tilt2.s.enabled, tilt2.s.strength),
            (TIER_DEFAULT.enabled, TIER_DEFAULT.strength),
            "tilt-shift goes back to the tier's default"
        );
        let medium = TiltShiftSettings {
            enabled: false,
            strength: 0.5,
            taps: 8,
        };
        assert_eq!(
            hex_render::tilt_shift::kept_tilt(tilt2.s, medium, tilt2.chosen),
            medium,
            "a tier change after the resume still drives tilt-shift"
        );
    }

    /// The player's own toggle is kept through a save, a reload and a tier change.
    #[test]
    fn a_chosen_toggle_survives_save_reload_and_a_tier_change() {
        let (mut g, mut cam) = played();
        let mut tilt = Tilt {
            s: TiltShiftSettings {
                enabled: false,
                strength: 0.7,
                taps: 16,
            },
            chosen: true,
        };
        let mut saved = SavedSettings::default();
        handle(
            save_cmd("chosen", None),
            &mut g,
            Some(&mut cam),
            tilt.live(),
            &mut saved,
            7,
        );
        let (save, v) = decode(&take_save("chosen").unwrap()).unwrap();
        assert!(save.settings.tilt_shift_chosen);
        let mut g2 = GameState::new(7);
        let mut tilt2 = Tilt::default();
        apply(save, v, &mut g2, None, tilt2.live(), &mut saved);
        assert!(tilt2.chosen);
        assert_eq!((tilt2.s.enabled, tilt2.s.strength), (false, 0.7));
        let low = TiltShiftSettings {
            enabled: false,
            strength: 0.5,
            taps: 4,
        };
        let kept = hex_render::tilt_shift::kept_tilt(tilt2.s, low, tilt2.chosen);
        assert_eq!((kept.enabled, kept.strength, kept.taps), (false, 0.7, 4));
    }

    /// Owner decision A16 (2026-10-14): loading a world exits photo mode. The orbit camera comes
    /// back at the save's pose with its FOV, exposure and lens, and agents run again.
    #[test]
    fn loading_a_world_exits_photo_mode_at_the_saved_camera() {
        use bevy::camera::Exposure;
        use bevy::ecs::system::RunSystemOnce;
        use hex_render::{AgentsPaused, tilt_shift::TiltOverride};

        use crate::photo::{PhotoExports, PhotoSession, PhotoSettings};

        let (mut g, cam) = played();
        let mut tilt = Tilt::default();
        let mut saved = SavedSettings::default();
        let path = std::env::temp_dir()
            .join(format!("hexw-photo-load-{}.hexworld", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let save = Command::Save {
            tag: None,
            path: Some(path.clone()),
            volumes: None,
        };
        handle(
            save,
            &mut g,
            Some(&mut cam.clone()),
            tilt.live(),
            &mut saved,
            7,
        );

        let mut app = App::new();
        app.insert_resource(GameState::new(7))
            .init_resource::<PhotoSession>()
            .init_resource::<crate::walk::WalkSession>()
            .init_resource::<PhotoSettings>()
            .init_resource::<PhotoExports>()
            .init_resource::<AgentsPaused>()
            .init_resource::<PersistQueue>()
            .init_resource::<SavedSettings>()
            .init_resource::<TiltShiftSettings>()
            .init_resource::<TiltPinned>()
            .init_resource::<hex_play::tier::TierChoice>()
            .insert_resource(crate::RunFlags(Default::default()));
        let orbit = Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y);
        let fov = PerspectiveProjection::default().fov;
        app.world_mut().spawn((
            Camera3d::default(),
            orbit,
            Projection::Perspective(PerspectiveProjection::default()),
            Exposure { ev100: 12.0 },
        ));
        let sync = |app: &mut App| app.world_mut().run_system_once(crate::photo::sync).unwrap();
        app.world_mut()
            .resource_mut::<GameState>()
            .apply(Action::TogglePhoto);
        sync(&mut app);
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&crate::photo::PhotoPatch {
                fov: Some(80.0),
                roll: Some(12.0),
                exposure: Some(1.0),
                ..Default::default()
            });
        sync(&mut app);
        assert!(app.world().resource::<AgentsPaused>().0);
        assert!(app.world().get_resource::<TiltOverride>().is_some());

        app.world_mut()
            .resource_mut::<PersistQueue>()
            .0
            .push_back(Command::Load {
                path: Some(path.clone()),
            });
        app.world_mut().run_system_once(run).unwrap();
        assert!(
            !app.world().resource::<GameState>().photo_mode,
            "a load exits photo mode"
        );
        assert!(
            app.world()
                .resource::<GameState>()
                .recent
                .iter()
                .any(|e| matches!(e, GameEvent::PhotoModeToggled { on: false })),
            "the exit is published like any other"
        );
        sync(&mut app);
        let mut q = app
            .world_mut()
            .query::<(&Transform, &Projection, &Exposure)>();
        let (t, p, e) = q.single(app.world()).unwrap();
        let Projection::Perspective(p) = p else {
            unreachable!()
        };
        assert!(
            t.translation.distance(cam.translation) < 1e-2
                && t.forward().dot(*cam.forward()) > 0.9999,
            "the camera is the save's orbit pose, not the free camera or the pre-photo one"
        );
        assert!((p.fov - fov).abs() < 1e-6, "FOV restored");
        assert_eq!(e.ev100, 12.0, "exposure restored");
        assert!(app.world().resource::<PhotoSession>().saved.is_none());
        assert!(app.world().get_resource::<TiltOverride>().is_none());
        assert!(
            !app.world().resource::<AgentsPaused>().0,
            "agents run again"
        );
        std::fs::remove_file(path).ok();
    }

    /// Start over (the Settings sheet's World tab) is `new_world` on the world in place, so it
    /// follows the load rule: photo mode exits, and the orbit camera, not the free one, comes back
    /// over the origin at its pre-photo angle and zoom. The world is the start tile with no history.
    #[test]
    fn a_new_world_exits_photo_mode_with_the_orbit_camera_over_the_origin() {
        use bevy::camera::Exposure;
        use bevy::ecs::system::RunSystemOnce;
        use hex_render::AgentsPaused;

        use crate::photo::{PhotoExports, PhotoSession, PhotoSettings};

        let (g, orbit) = played();
        let mut app = App::new();
        app.insert_resource(g)
            .init_resource::<PhotoSession>()
            .init_resource::<crate::walk::WalkSession>()
            .init_resource::<PhotoSettings>()
            .init_resource::<PhotoExports>()
            .init_resource::<AgentsPaused>()
            .init_resource::<PersistQueue>()
            .init_resource::<SavedSettings>()
            .init_resource::<TiltShiftSettings>()
            .init_resource::<TiltPinned>()
            .init_resource::<hex_play::tier::TierChoice>()
            .insert_resource(crate::RunFlags(Default::default()));
        app.world_mut().spawn((
            Camera3d::default(),
            orbit,
            Projection::Perspective(PerspectiveProjection::default()),
            Exposure { ev100: 12.0 },
        ));
        let sync = |app: &mut App| app.world_mut().run_system_once(crate::photo::sync).unwrap();
        app.world_mut()
            .resource_mut::<GameState>()
            .apply(Action::TogglePhoto);
        sync(&mut app);
        let mut cams = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Camera3d>>();
        *cams.single_mut(app.world_mut()).unwrap() =
            Transform::from_xyz(-50.0, 5.0, 20.0).looking_at(Vec3::new(-50.0, 0.0, 0.0), Vec3::Y);

        app.world_mut()
            .resource_mut::<PersistQueue>()
            .0
            .push_back(Command::NewWorld);
        app.world_mut().run_system_once(run).unwrap();
        let seed = app.world().resource::<crate::RunFlags>().0.seed;
        let game = app.world().resource::<GameState>();
        assert!(!game.photo_mode, "a new world exits photo mode");
        assert_eq!(game.world, World::new(seed), "only the start tile is left");
        assert!(!game.world.can_undo() && !game.world.can_redo());
        sync(&mut app);
        let t = *cams.single(app.world()).unwrap();
        let f = camera::focus(&t);
        assert!(
            f.x.abs() < 1e-3 && f.z.abs() < 1e-3,
            "recentred over the origin ({f})"
        );
        assert!(
            t.forward().dot(*orbit.forward()) > 0.9999,
            "the orbit camera's angle, not the free camera's"
        );
        let zoom = |t: &Transform| (t.translation - camera::focus(t)).length();
        assert!((zoom(&t) - zoom(&orbit)).abs() < 1e-2, "and its zoom");
        assert!(
            !app.world().resource::<AgentsPaused>().0,
            "agents run again"
        );
    }
}
