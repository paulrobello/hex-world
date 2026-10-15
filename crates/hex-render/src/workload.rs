//! The engine-gate workload: the PRD tile shape (§6, §10) at 2,000 tiles plus moving agents (§7.2).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateWorkload {
    pub tiles: usize,
    pub pieces_per_tile: usize,
    pub materials: usize,
    pub moving: usize,
    pub tris_lod0_per_tile: usize,
}

pub const GATE: GateWorkload = GateWorkload {
    tiles: 2000,
    pieces_per_tile: 20,
    materials: 40,
    moving: 300,
    tris_lod0_per_tile: 1000 + 6 * 600 + 1500 + 12 * 800,
};

impl GateWorkload {
    /// Logical pieces plus movers. Each piece renders as three LOD entities, so the
    /// `Mesh3d` entity count is about three times this.
    pub fn entities(&self) -> usize {
        self.tiles * self.pieces_per_tile + self.moving
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GateView {
    #[default]
    Strategic,
    Closeup,
}

impl GateView {
    pub fn parse(s: &str) -> Option<GateView> {
        match s {
            "strategic" => Some(GateView::Strategic),
            "closeup" => Some(GateView::Closeup),
            _ => None,
        }
    }
}

/// Material index of piece `pi` on tile `ti`: a synthetic scatter over all `GATE.materials`.
pub fn piece_material(ti: usize, pi: usize) -> usize {
    (ti * 7 + pi) % GATE.materials
}

/// How the gate's static pieces become entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layout {
    /// One entity per piece per LOD (the Task 6 scene).
    #[default]
    Entities,
    /// One merged mesh per chunk, LOD and material.
    Chunks,
    /// Shared kit meshes drawn from one instance buffer: per view, one draw per (piece mesh,
    /// LOD) over the chunks that view sees (PRD §6 "per-chunk GPU instance buffers").
    Instanced,
}

impl Layout {
    pub fn parse(s: &str) -> Option<Layout> {
        match s {
            "entities" => Some(Layout::Entities),
            "chunks" => Some(Layout::Chunks),
            "instanced" => Some(Layout::Instanced),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layout::Entities => "entities",
            Layout::Chunks => "chunks",
            Layout::Instanced => "instanced",
        }
    }
}

/// Camera post stack: `Full` is TAA, SSAO, bloom and DOF; `None` turns all four off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PostMode {
    #[default]
    Full,
    None,
}

impl PostMode {
    pub fn parse(s: &str) -> Option<PostMode> {
        match s {
            "full" => Some(PostMode::Full),
            "none" => Some(PostMode::None),
            _ => None,
        }
    }
}

pub fn parse_shadows(s: &str) -> Option<bool> {
    match s {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// `grass=off|sparse|normal|lush` (`on` is normal): grass blade density in play where the tier
/// allows them (`play::grass`).
pub fn parse_grass(s: &str) -> Option<crate::play::grass::Density> {
    crate::play::grass::Density::parse(s)
}

/// `ibl=on|off`: the noon environment map on the kit camera (D5).
pub fn parse_ibl(s: &str) -> Option<bool> {
    parse_shadows(s)
}

/// D4 candidates. The default is the default grade's (`rich-3`, owner 2026-10-14): ACES fitted, with
/// a shadow lift before the curve because fitted ACES outputs pure black below about 0.0033 linear,
/// where the shaded soil walls sit. AgX desaturated the meadow greens against the approved
/// references (2026-10-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tonemap {
    AgX,
    TonyMcMapface,
    #[default]
    AcesFitted,
}

impl Tonemap {
    pub fn parse(s: &str) -> Option<Tonemap> {
        match s {
            "agx" => Some(Tonemap::AgX),
            "tony" => Some(Tonemap::TonyMcMapface),
            "aces" => Some(Tonemap::AcesFitted),
            _ => None,
        }
    }
}

/// `ev100=<f32>`: the kit camera's exposure (EV100). Stored in hundredths so `GateConfig` stays `Eq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ev100(i32);

impl Ev100 {
    /// Finite values in [0, 20] (Bevy's range of sensible camera EV100s), rounded to 0.01.
    pub fn parse(s: &str) -> Option<Ev100> {
        let v: f32 = s.parse().ok()?;
        (v.is_finite() && (0.0..=20.0).contains(&v)).then(|| Ev100::from_f32(v))
    }

    pub fn from_f32(v: f32) -> Ev100 {
        Ev100((v * 100.0).round() as i32)
    }

    pub fn get(self) -> f32 {
        self.0 as f32 / 100.0
    }
}

/// `tilt=`: the tilt-shift pass. `Auto` follows the quality tier (off on Low) and runs in the kit
/// scenario only, so the P0 perf baselines are unchanged; `On` and `Off` force it in any scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TiltMode {
    #[default]
    Auto,
    On,
    Off,
}

impl TiltMode {
    pub fn parse(s: &str) -> Option<TiltMode> {
        match s {
            "auto" => Some(TiltMode::Auto),
            "on" => Some(TiltMode::On),
            "off" => Some(TiltMode::Off),
            _ => None,
        }
    }
}

/// `tilt_strength=<f32>`: the tilt-shift strength slider, 0..=1. Stored in hundredths so
/// `GateConfig` stays `Eq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TiltStrength(i32);

impl TiltStrength {
    pub fn parse(s: &str) -> Option<TiltStrength> {
        let v: f32 = s.parse().ok()?;
        (v.is_finite() && (0.0..=1.0).contains(&v))
            .then(|| TiltStrength((v * 100.0).round() as i32))
    }

    pub fn get(self) -> f32 {
        self.0 as f32 / 100.0
    }
}

/// Hexes per chunk: 7 (radius 1) or 19 (radius 2).
pub fn parse_chunk(s: &str) -> Option<u32> {
    match s {
        "7" => Some(7),
        "19" => Some(19),
        _ => None,
    }
}

/// Which static pieces cast shadows: `All`, or `Large` (no props and no LOD2 pieces).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShadowCasters {
    #[default]
    All,
    Large,
}

impl ShadowCasters {
    pub fn parse(s: &str) -> Option<ShadowCasters> {
        match s {
            "all" => Some(ShadowCasters::All),
            "large" => Some(ShadowCasters::Large),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShadowCasters::All => "all",
            ShadowCasters::Large => "large",
        }
    }
}

/// Directional-light shadow cascades: 2, 3 or 4.
pub fn parse_cascades(s: &str) -> Option<u32> {
    match s {
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        _ => None,
    }
}

/// End of the first cascade; `shadow_dist` must lie beyond it.
pub const FIRST_CASCADE_FAR_BOUND: f32 = 120.0;

/// Cascade maximum distance in whole metres, beyond the first cascade's far bound.
pub fn parse_shadow_dist(s: &str) -> Option<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|&m| m as f32 > FIRST_CASCADE_FAR_BOUND && m <= 100_000)
}

/// `kit_hooks=`: drives the §9.4 hooks for verification shots (default none = noon, the MVP);
/// `crate::hooks` maps each variant to its uniforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KitHooks {
    #[default]
    None,
    Snow,
    Wet,
    Night,
    /// The time-of-day block at `hooks::DUSK`.
    Dusk,
    /// Water freeze 1.
    Freeze,
    /// Foliage instances tagged with autumn season indices.
    Autumn,
}

impl KitHooks {
    pub fn parse(s: &str) -> Option<KitHooks> {
        match s {
            "none" => Some(KitHooks::None),
            "snow" => Some(KitHooks::Snow),
            "wet" => Some(KitHooks::Wet),
            "night" => Some(KitHooks::Night),
            "dusk" => Some(KitHooks::Dusk),
            "freeze" => Some(KitHooks::Freeze),
            "autumn" => Some(KitHooks::Autumn),
            _ => None,
        }
    }
}

/// `kit_src=`: what the kit scenario lays out. `Runtime` is `assets/kit/manifest.json` with the P1
/// family layout; `Fixture` is the kit gate fixture, for checking the runtime path before any
/// real piece ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KitSource {
    #[default]
    Runtime,
    Fixture,
    /// The gate fixture with its cap bound to a probe tileable set (constant maps under
    /// `assets/kit/p1/fixture_tile/`), so the tileable forward and prepass branches run.
    FixtureTiled,
}

impl KitSource {
    pub fn parse(s: &str) -> Option<KitSource> {
        match s {
            "runtime" => Some(KitSource::Runtime),
            "fixture" => Some(KitSource::Fixture),
            "fixture_tiled" => Some(KitSource::FixtureTiled),
            _ => None,
        }
    }
}

/// Gate scene variant, read by `setup` on every (re)build.
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateConfig {
    pub layout: Layout,
    pub post: PostMode,
    pub shadows: bool,
    pub chunk: u32,
    pub shadow_casters: ShadowCasters,
    pub cascades: u32,
    /// Cascade maximum distance in metres.
    pub shadow_dist: u32,
    /// Kit scenario only.
    pub kit_hooks: KitHooks,
    /// Kit scenario only.
    pub kit_src: KitSource,
    /// Kit scenario only.
    pub tonemap: Tonemap,
    /// Kit scenario only: the noon IBL replaces the flat ambient light.
    pub ibl: bool,
    /// Kit scenario only: camera exposure (default: the default grade's).
    pub ev100: Ev100,
    /// Kit scenario only: the look preset (`grade`); it sets `tonemap` and `ev100` unless their own
    /// keys override them, and supplies the colour grading and backdrop.
    pub grade: crate::grade::Grade,
    pub tilt: TiltMode,
    pub tilt_strength: TiltStrength,
    /// Quality tier (PRD §9.2): the camera post stack, and the water variant.
    pub tier: crate::post::QualityTier,
    /// Water scenario only: advance water time by a fixed 1/30 s per frame (motion captures).
    pub water_fixed: bool,
    /// `grass=off|sparse|normal|lush`: grass blade density near the camera in play, on the High
    /// and Ultra tiers (`play::grass`); `set_grass` changes it live.
    pub grass: crate::play::grass::Density,
}

impl Default for GateConfig {
    fn default() -> Self {
        let grade = crate::grade::Grade::default();
        Self {
            layout: Layout::Entities,
            post: PostMode::Full,
            shadows: true,
            chunk: 7,
            shadow_casters: ShadowCasters::All,
            cascades: 4,
            shadow_dist: 2_000,
            kit_hooks: KitHooks::None,
            kit_src: KitSource::Runtime,
            tonemap: grade.tonemap(),
            ibl: true,
            ev100: Ev100::from_f32(grade.ev100()),
            grade,
            tilt: TiltMode::Off,
            tilt_strength: TiltStrength(100),
            tier: crate::post::QualityTier::High,
            water_fixed: false,
            grass: crate::play::grass::Density::Normal,
        }
    }
}

/// Deterministic per-entity jitter (splitmix64), mapped to `[0, 1)`.
pub fn unit_hash(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_matches_prd_tile_shape() {
        assert_eq!(GATE.tiles, 2000);
        assert_eq!(GATE.pieces_per_tile, 1 + 6 + 1 + 12);
        assert_eq!(GATE.tris_lod0_per_tile, 15_700);
        assert_eq!(GATE.entities(), 40_300);
    }

    #[test]
    fn views_parse() {
        assert_eq!(GateView::parse("closeup"), Some(GateView::Closeup));
        assert_eq!(GateView::parse("x"), None);
    }

    #[test]
    fn ev100_parses_in_range_only() {
        assert_eq!(Ev100::parse("12.5").map(Ev100::get), Some(12.5));
        assert_eq!(Ev100::parse("13.3").map(Ev100::get), Some(13.3));
        assert_eq!(Ev100::parse("0"), Some(Ev100::from_f32(0.0)));
        assert_eq!(Ev100::parse("21"), None);
        assert_eq!(Ev100::parse("-1"), None);
        assert_eq!(Ev100::parse("NaN"), None);
        assert_eq!(Ev100::parse("bright"), None);
        assert_eq!(
            GateConfig::default().ev100.get(),
            crate::grade::Grade::default().ev100()
        );
    }

    #[test]
    fn gate_config_values_parse() {
        assert_eq!(Layout::parse("entities"), Some(Layout::Entities));
        assert_eq!(Layout::parse("chunks"), Some(Layout::Chunks));
        assert_eq!(Layout::parse("chunk"), None);
        assert_eq!(PostMode::parse("full"), Some(PostMode::Full));
        assert_eq!(PostMode::parse("none"), Some(PostMode::None));
        assert_eq!(PostMode::parse("off"), None);
        assert_eq!(parse_shadows("on"), Some(true));
        assert_eq!(parse_shadows("off"), Some(false));
        assert_eq!(parse_shadows("1"), None);
        assert_eq!(parse_chunk("7"), Some(7));
        assert_eq!(parse_chunk("19"), Some(19));
        assert_eq!(parse_chunk("8"), None);
        assert_eq!(parse_chunk("x"), None);
        assert_eq!(Layout::parse("instanced"), Some(Layout::Instanced));
        assert_eq!(Layout::Instanced.name(), "instanced");
        assert_eq!(ShadowCasters::parse("all"), Some(ShadowCasters::All));
        assert_eq!(ShadowCasters::parse("large"), Some(ShadowCasters::Large));
        assert_eq!(ShadowCasters::parse("some"), None);
        assert_eq!(ShadowCasters::Large.name(), "large");
        assert_eq!(parse_cascades("2"), Some(2));
        assert_eq!(parse_cascades("3"), Some(3));
        assert_eq!(parse_cascades("4"), Some(4));
        assert_eq!(parse_cascades("1"), None);
        assert_eq!(parse_cascades("5"), None);
        assert_eq!(parse_shadow_dist("600"), Some(600));
        assert_eq!(parse_shadow_dist("2000"), Some(2000));
        // The first cascade ends at 120 m, so the maximum must lie beyond it.
        assert_eq!(parse_shadow_dist("120"), None);
        assert_eq!(parse_shadow_dist("-5"), None);
        assert_eq!(parse_shadow_dist("x"), None);
        let d = GateConfig::default();
        assert_eq!(
            (d.shadow_casters, d.cascades, d.shadow_dist),
            (ShadowCasters::All, 4, 2000)
        );
        assert_eq!(
            (d.layout, d.post, d.shadows, d.chunk),
            (Layout::Entities, PostMode::Full, true, 7)
        );
        assert_eq!(
            (d.kit_hooks, d.kit_src),
            (KitHooks::None, KitSource::Runtime)
        );
    }

    #[test]
    fn kit_keys_parse() {
        for (s, h) in [
            ("none", KitHooks::None),
            ("snow", KitHooks::Snow),
            ("wet", KitHooks::Wet),
            ("night", KitHooks::Night),
            ("dusk", KitHooks::Dusk),
            ("freeze", KitHooks::Freeze),
            ("autumn", KitHooks::Autumn),
        ] {
            assert_eq!(KitHooks::parse(s), Some(h));
        }
        assert_eq!(KitHooks::parse("hail"), None);
        assert_eq!(KitSource::parse("runtime"), Some(KitSource::Runtime));
        assert_eq!(KitSource::parse("fixture"), Some(KitSource::Fixture));
        assert_eq!(
            KitSource::parse("fixture_tiled"),
            Some(KitSource::FixtureTiled)
        );
        assert_eq!(KitSource::parse("p0"), None);
    }

    #[test]
    fn lookdev_keys_parse() {
        assert_eq!(Tonemap::parse("agx"), Some(Tonemap::AgX));
        assert_eq!(Tonemap::parse("tony"), Some(Tonemap::TonyMcMapface));
        assert_eq!(Tonemap::parse("aces"), Some(Tonemap::AcesFitted));
        assert_eq!(Tonemap::parse("filmic"), None);
        assert_eq!(parse_ibl("on"), Some(true));
        assert_eq!(parse_ibl("off"), Some(false));
        assert_eq!(parse_ibl("1"), None);
        let d = GateConfig::default();
        assert_eq!((d.tonemap, d.ibl), (Tonemap::AcesFitted, true));
        assert_eq!(Tonemap::default(), d.tonemap);
    }

    #[test]
    fn tilt_keys_parse() {
        assert_eq!(TiltMode::parse("auto"), Some(TiltMode::Auto));
        assert_eq!(TiltMode::parse("on"), Some(TiltMode::On));
        assert_eq!(TiltMode::parse("off"), Some(TiltMode::Off));
        assert_eq!(TiltMode::parse("1"), None);
        assert_eq!(TiltStrength::parse("0.5").map(TiltStrength::get), Some(0.5));
        assert_eq!(TiltStrength::parse("1.5"), None);
        assert_eq!(TiltStrength::parse("-0.1"), None);
        assert_eq!(TiltStrength::parse("NaN"), None);
        let d = GateConfig::default();
        assert_eq!((d.tilt, d.tilt_strength.get()), (TiltMode::Off, 1.0));
    }

    #[test]
    fn unit_hash_in_range_and_deterministic() {
        for s in 0..1000 {
            let v = unit_hash(s);
            assert!((0.0..1.0).contains(&v));
            assert_eq!(v, unit_hash(s));
        }
    }
}
