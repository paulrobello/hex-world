//! The engine-gate benchmark workload (`gate-tools`): the PRD tile shape (§6, §10) at 2,000 tiles
//! plus moving agents (§7.2), and the gate scene's own knobs. Production settings are
//! `config::RenderConfig`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateShape {
    pub tiles: usize,
    pub pieces_per_tile: usize,
    pub materials: usize,
    pub moving: usize,
    pub tris_lod0_per_tile: usize,
}

pub const GATE: GateShape = GateShape {
    tiles: 2000,
    pieces_per_tile: 20,
    materials: 40,
    moving: 300,
    tris_lod0_per_tile: 1000 + 6 * 600 + 1500 + 12 * 800,
};

impl GateShape {
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

/// The gate and kit-showcase scene variant, read by `setup` on every (re)build. Production render
/// settings are `config::RenderConfig`.
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateWorkload {
    pub layout: Layout,
    pub chunk: u32,
    pub shadow_casters: ShadowCasters,
    /// Kit scenario only.
    pub kit_src: KitSource,
}

impl Default for GateWorkload {
    fn default() -> Self {
        Self {
            layout: Layout::Entities,
            chunk: 7,
            shadow_casters: ShadowCasters::All,
            kit_src: KitSource::Runtime,
        }
    }
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
    fn gate_workload_values_parse() {
        assert_eq!(Layout::parse("entities"), Some(Layout::Entities));
        assert_eq!(Layout::parse("chunks"), Some(Layout::Chunks));
        assert_eq!(Layout::parse("chunk"), None);
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
        let d = GateWorkload::default();
        assert_eq!(
            (d.layout, d.shadow_casters, d.kit_src),
            (Layout::Entities, ShadowCasters::All, KitSource::Runtime)
        );
        assert_eq!(d.chunk, 7);
    }

    #[test]
    fn kit_source_parses() {
        assert_eq!(KitSource::parse("runtime"), Some(KitSource::Runtime));
        assert_eq!(KitSource::parse("fixture"), Some(KitSource::Fixture));
        assert_eq!(
            KitSource::parse("fixture_tiled"),
            Some(KitSource::FixtureTiled)
        );
        assert_eq!(KitSource::parse("p0"), None);
    }
}
