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
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            layout: Layout::Entities,
            post: PostMode::Full,
            shadows: true,
            chunk: 7,
            shadow_casters: ShadowCasters::All,
            cascades: 4,
            shadow_dist: 2_000,
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
