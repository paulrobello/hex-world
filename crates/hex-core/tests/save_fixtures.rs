//! Every committed `.hexworld` fixture still loads (PRD §12.2, docs/save-format.md). Fixtures are
//! frozen: a format change bumps the version and adds a new fixture, it never rewrites one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hex_core::{
    AudioVolumes, Axial, CameraState, GameSettings, QualitySetting, SAVE_FORMAT_VERSION, SaveData,
    TileCatalog, World, cell_seed,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/save")
}

fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
}

/// `v<N>-<name>.hexworld` files by path, with `N` parsed from the name.
fn fixtures() -> BTreeMap<PathBuf, u16> {
    std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "hexworld"))
        .map(|p| {
            let version = name_version(&p);
            (p, version)
        })
        .collect()
}

fn name_version(path: &Path) -> u16 {
    let name = path.file_name().unwrap().to_string_lossy();
    name.strip_prefix('v')
        .and_then(|rest| rest.split_once('-'))
        .and_then(|(v, _)| v.parse().ok())
        .unwrap_or_else(|| panic!("{name} is not named v<N>-<name>.hexworld"))
}

const EAST: Axial = Axial { q: 1, r: 0 };
const WEST: Axial = Axial { q: -1, r: 0 };
const SOUTH: Axial = Axial { q: 0, r: 1 };
const NORTH: Axial = Axial { q: 0, r: -1 };

/// The save in `v1-small-world.hexworld`: four placements, a bulldoze and two undos, so both
/// history stacks are non-empty.
fn small_world_v1(c: &TileCatalog) -> SaveData {
    let mut w = World::new(2026);
    w.place(c, EAST, "road_straight", 1).unwrap();
    w.place(c, WEST, "forest", 3).unwrap();
    w.place(c, SOUTH, "meadow", 0).unwrap();
    w.place(c, NORTH, "beach", 4).unwrap();
    w.bulldoze(WEST).unwrap();
    assert!(w.undo() && w.undo());
    SaveData {
        world: w,
        unlocks: BTreeSet::new(),
        camera: CameraState {
            focus: [12.5, 0.0, -3.25],
            yaw: 0.75,
            pitch: 0.9,
            distance: 180.0,
        },
        settings: GameSettings {
            quality: QualitySetting::High,
            tilt_shift: true,
            // v1 has no such field: a v1 file loads with tilt-shift driven by the tier.
            tilt_shift_chosen: false,
            tilt_shift_strength: 0.6,
            volumes: AudioVolumes {
                master: 0.8,
                music: 0.5,
                sfx: 1.0,
                ambience: 0.7,
                ui: 0.9,
            },
        },
    }
}

#[test]
fn every_fixture_loads_and_every_version_has_one() {
    let c = catalog();
    let all = fixtures();
    assert!(
        !all.is_empty(),
        "no fixtures in {}",
        fixture_dir().display()
    );
    for (path, version) in &all {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            bytes.get(8..10),
            Some(&version.to_le_bytes()[..]),
            "{} header version differs from its name",
            path.display()
        );
        let save = SaveData::decode(&bytes, &c)
            .unwrap_or_else(|e| panic!("{} does not load: {e}", path.display()));
        if *version == SAVE_FORMAT_VERSION {
            assert_eq!(
                save.encode(),
                bytes,
                "{} no longer re-encodes byte for byte: the current wire format drifted",
                path.display()
            );
        }
    }
    let versions: BTreeSet<u16> = all.values().copied().collect();
    for v in 1..=SAVE_FORMAT_VERSION {
        assert!(versions.contains(&v), "no fixture for format version {v}");
    }
}

#[test]
fn v1_small_world_restores_tiles_history_camera_and_settings() {
    let c = catalog();
    let bytes = std::fs::read(fixture_dir().join("v1-small-world.hexworld")).unwrap();
    let save = SaveData::decode(&bytes, &c).unwrap();

    let mut w = save.world.clone();
    assert_eq!(w.seed(), 2026);
    let cells: Vec<Axial> = w.tiles().keys().copied().collect();
    assert_eq!(cells, [WEST, Axial::ORIGIN, SOUTH, EAST]);
    let road = w.tile(EAST).unwrap();
    assert_eq!(
        (road.def.as_str(), road.rotation, road.seed),
        ("road_straight", 1, cell_seed(2026, EAST))
    );
    assert_eq!(w.tile(WEST).unwrap().rotation, 3);
    // Saved before the pine and oak split (U3): its forest loads as the pine forest, no oak.
    assert_eq!(w.tile(WEST).unwrap().def, "forest");
    let pine = c.get("forest").unwrap();
    assert!(pine.props.hero.is_empty() && pine.agents.is_empty());
    assert!(save.unlocks.is_empty());
    assert_eq!(save.camera.distance, 180.0);
    assert_eq!(save.settings.quality, QualitySetting::High);
    assert_eq!(save.settings.volumes.ui, 0.9);
    assert!(save.settings.tilt_shift, "the v1 toggle value is kept");
    assert!(
        !save.settings.tilt_shift_chosen,
        "a v1 file cannot say the player chose tilt-shift, so the tier drives it"
    );

    // Redo replays the beach, then the bulldoze; undo walks back to the start meadow.
    assert!(w.can_undo() && w.can_redo());
    assert!(w.redo());
    assert_eq!(w.tile(NORTH).unwrap().def, "beach");
    assert!(w.redo());
    assert!(w.tile(WEST).is_none());
    assert!(!w.redo());
    while w.undo() {}
    assert_eq!(w.tiles(), World::new(2026).tiles());

    // While v1 is the current version, the fixture equals the world built by play.
    if SAVE_FORMAT_VERSION == 1 {
        assert_eq!(save, small_world_v1(&c));
    }
}

/// The save in `v2-tilt-chosen.hexworld`: the v1 small world with the player's own tilt-shift
/// toggle (off) recorded.
fn tilt_chosen_v2(c: &TileCatalog) -> SaveData {
    let mut save = small_world_v1(c);
    save.settings.tilt_shift = false;
    save.settings.tilt_shift_chosen = true;
    save
}

#[test]
fn v2_tilt_chosen_keeps_the_players_toggle() {
    let c = catalog();
    let bytes = std::fs::read(fixture_dir().join("v2-tilt-chosen.hexworld")).unwrap();
    let save = SaveData::decode(&bytes, &c).unwrap();
    assert!(save.settings.tilt_shift_chosen);
    assert!(!save.settings.tilt_shift);
    if SAVE_FORMAT_VERSION == 2 {
        assert_eq!(save, tilt_chosen_v2(&c));
    }
}

/// The save in `v3-replaced.hexworld`: the v2 world with the road at EAST replaced by a pine forest
/// (the newest undo record), then the meadow at SOUTH replaced by an oak and that replace undone
/// (the next redo).
fn replaced_v3(c: &TileCatalog) -> SaveData {
    let mut save = tilt_chosen_v2(c);
    let w = &mut save.world;
    w.replace(c, EAST, "forest", 0).unwrap();
    w.replace(c, SOUTH, "oak", 2).unwrap();
    assert!(w.undo());
    save
}

#[test]
fn v3_replaced_keeps_replace_records_on_both_stacks() {
    let c = catalog();
    let bytes = std::fs::read(fixture_dir().join("v3-replaced.hexworld")).unwrap();
    let save = SaveData::decode(&bytes, &c).unwrap();
    assert!(
        save.settings.tilt_shift_chosen && !save.settings.tilt_shift,
        "v3 keeps v2's tilt_shift_chosen"
    );
    let mut w = save.world.clone();
    assert_eq!(w.tile(EAST).unwrap().def, "forest");
    assert_eq!(w.tile(SOUTH).unwrap().def, "meadow");
    assert!(w.redo(), "redo swaps the oak in");
    assert_eq!(w.tile(SOUTH).unwrap().def, "oak");
    assert!(
        w.undo() && w.undo(),
        "undo swaps the oak and then the forest out"
    );
    let road = w.tile(EAST).unwrap();
    assert_eq!((road.def.as_str(), road.rotation), ("road_straight", 1));
    if SAVE_FORMAT_VERSION == 3 {
        assert_eq!(save, replaced_v3(&c));
    }
}

#[test]
#[ignore = "writes the v3 fixture once; fixtures are frozen"]
fn write_v3_fixture() {
    let path = fixture_dir().join("v3-replaced.hexworld");
    assert!(
        !path.exists(),
        "{} exists and is frozen; add a new version instead",
        path.display()
    );
    assert_eq!(SAVE_FORMAT_VERSION, 3, "this writer only produces v3");
    std::fs::write(&path, replaced_v3(&catalog()).encode()).unwrap();
}

#[test]
#[ignore = "writes the v2 fixture once; fixtures are frozen"]
fn write_v2_fixture() {
    let path = fixture_dir().join("v2-tilt-chosen.hexworld");
    assert!(
        !path.exists(),
        "{} exists and is frozen; add a new version instead",
        path.display()
    );
    assert_eq!(SAVE_FORMAT_VERSION, 2, "this writer only produces v2");
    std::fs::write(&path, tilt_chosen_v2(&catalog()).encode()).unwrap();
}

#[test]
#[ignore = "writes the v1 fixture once; fixtures are frozen"]
fn write_v1_fixture() {
    let path = fixture_dir().join("v1-small-world.hexworld");
    assert!(
        !path.exists(),
        "{} exists and is frozen; add a new version instead",
        path.display()
    );
    assert_eq!(SAVE_FORMAT_VERSION, 1, "this writer only produces v1");
    std::fs::create_dir_all(fixture_dir()).unwrap();
    std::fs::write(&path, small_world_v1(&catalog()).encode()).unwrap();
}
