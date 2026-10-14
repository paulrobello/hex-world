//! `assets/kit/manifest.json`: kit pieces, their material slots and sidecar maps (R6).
use crate::{TextureFamily, family_suffix};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const GLB_MAX_BYTES: u64 = 2_097_152; // PRD §10
/// KTX2 bytes per texture family (BC7 and ASTC counted separately), by budget class. Owner ruling
/// 2026-10-13. Mirrors `KTX_BUDGETS` in `scripts/assets/kit-lib.mjs`; change both together (S33).
pub const KTX_BUDGETS: [(BudgetClass, u64); 3] = [
    (BudgetClass::Piece, 1_500_000),
    (BudgetClass::Tileable, 3_600_000),
    (BudgetClass::Hero, 11_500_000),
];
pub const LOD1_MAX_RATIO: f32 = 0.5;
pub const LOD2_MAX_RATIO: f32 = 0.15;
pub const APOTHEM: f32 = hex_core_apothem();

const fn hex_core_apothem() -> f32 {
    16.0 * 0.866_025_4
}

/// KTX2 budget class, recorded by `kit-compress` as `budgets.class` in `compress-report.json`.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum BudgetClass {
    /// A piece's unique maps (the default).
    Piece,
    /// One shared world-space material set.
    Tileable,
    /// Full-detail hero pieces (streamed biome packs).
    Hero,
}

impl BudgetClass {
    pub fn ktx_max(self) -> u64 {
        KTX_BUDGETS
            .iter()
            .find(|(c, _)| *c == self)
            .map_or(0, |(_, b)| *b)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BudgetClass::Piece => "piece",
            BudgetClass::Tileable => "tileable",
            BudgetClass::Hero => "hero",
        }
    }

    pub fn parse(s: &str) -> Option<BudgetClass> {
        KTX_BUDGETS
            .iter()
            .map(|(c, _)| *c)
            .find(|c| c.as_str() == s)
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Albedo,
    Normal,
    Orm,
    Emissive,
    Masks,
}

impl Channel {
    /// R6: base colour and emissive are sRGB; normal, ORM and masks are linear. The PNG fallback
    /// carries no color space, so it must load with this `is_srgb` too.
    pub fn is_srgb(self) -> bool {
        matches!(self, Channel::Albedo | Channel::Emissive)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Albedo => "albedo",
            Channel::Normal => "normal",
            Channel::Orm => "orm",
            Channel::Emissive => "emissive",
            Channel::Masks => "masks",
        }
    }

    pub fn parse(s: &str) -> Option<Channel> {
        [
            Channel::Albedo,
            Channel::Normal,
            Channel::Orm,
            Channel::Emissive,
            Channel::Masks,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PieceKind {
    Base,
    Edge,
    Center,
    Prop,
    Tree,
    Hero,
}

impl PieceKind {
    pub fn lod0_budget(self) -> usize {
        match self {
            PieceKind::Base | PieceKind::Edge | PieceKind::Center => 4_000,
            PieceKind::Hero => 12_000,
            PieceKind::Prop => 1_500,
            PieceKind::Tree => 3_000,
        }
    }

    /// Pieces whose geometry reaches the hex border and must keep the border ring in every LOD.
    pub fn touches_border(self) -> bool {
        matches!(self, PieceKind::Base | PieceKind::Edge | PieceKind::Center)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileSet {
    /// World meters covered by one repeat of the tile (D2: 1024 px over 2 m = 512 px/m).
    pub tile_m: f32,
    pub maps: BTreeMap<Channel, String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    /// World-space tileable set id (bases, road surfaces).
    #[serde(default)]
    pub tileable: Option<String>,
    /// Use the piece's own albedo/normal/orm/emissive on UV0 (props, trees).
    #[serde(default)]
    pub unique: bool,
    /// Bind the piece's `masks` map (§9.4 hooks + AO in A).
    #[serde(default)]
    pub masks: bool,
    /// Linear RGB base colour factor (default white). The GLB's own material factors are ignored.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// Perceptual roughness factor for untextured slots (default 1.0).
    #[serde(default)]
    pub roughness: Option<f32>,
    /// Alpha-test cutoff for leaf cards (AlphaMode::Mask); implies double-sided, no culling.
    #[serde(default)]
    pub alpha_cutoff: Option<f32>,
    /// Map the `tileable` set on UV0 instead of world space: StandardMaterial's `uv_transform`
    /// scales UV0 by this factor (tile repeats per UV unit) while `masks` keeps raw UV0. For
    /// surfaces world-space tiling cannot map, such as tree bark.
    #[serde(default)]
    pub tile_uv_scale: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PieceEntry {
    pub id: String,
    pub kind: PieceKind,
    /// Path under `assets/kit/` (the `kit://` root).
    pub glb: String,
    /// Repo-relative provenance path. Its directory also holds `compress-report.json`.
    pub provenance: String,
    /// Keyed by the glTF material name exported from Blender.
    pub slots: BTreeMap<String, Slot>,
    /// Map stems under `assets/kit/`, without the family suffix.
    #[serde(default)]
    pub maps: BTreeMap<Channel, String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitManifest {
    pub version: u32,
    pub materials: BTreeMap<String, TileSet>,
    pub pieces: Vec<PieceEntry>,
}

pub fn parse_manifest(json: &str) -> Result<KitManifest, String> {
    let m: KitManifest = serde_json::from_str(json).map_err(|e| format!("manifest: {e}"))?;
    for p in &m.pieces {
        for (name, slot) in &p.slots {
            if let Some(set) = &slot.tileable
                && !m.materials.contains_key(set)
            {
                return Err(format!(
                    "{}: slot {name} uses unknown tileable set {set}",
                    p.id
                ));
            }
            if let Some(k) = slot.tile_uv_scale
                && (slot.tileable.is_none() || !(k.is_finite() && k > 0.0))
            {
                return Err(format!(
                    "{}: slot {name} tileUvScale {k} needs a tileable set and a positive factor",
                    p.id
                ));
            }
            if slot.masks && !p.maps.contains_key(&Channel::Masks) {
                return Err(format!(
                    "{}: slot {name} wants masks but the piece has none",
                    p.id
                ));
            }
        }
    }
    Ok(m)
}

pub fn map_file(stem: &str, family: TextureFamily) -> String {
    format!("{stem}.{}", family_suffix(family))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"{"version":1,
      "materials":{"grass":{"tileM":2.0,"maps":{"albedo":"p1/materials/grass/grass.albedo"}}},
      "pieces":[{"id":"meadow_base","kind":"base","glb":"p1/meadow_base/meadow_base.glb",
        "provenance":"assets-source/kit/meadow_base/provenance.json",
        "slots":{"meadow_base_cap":{"tileable":"grass","masks":true}},
        "maps":{"masks":"p1/meadow_base/meadow_base.masks"}}]}"#;

    #[test]
    fn parses_and_resolves() {
        let m = parse_manifest(OK).unwrap();
        assert_eq!(m.pieces[0].kind, PieceKind::Base);
        assert_eq!(m.materials["grass"].tile_m, 2.0);
        assert_eq!(
            map_file(&m.pieces[0].maps[&Channel::Masks], TextureFamily::Astc),
            "p1/meadow_base/meadow_base.masks.astc.ktx2"
        );
    }

    #[test]
    fn unknown_set_or_missing_masks_is_an_error() {
        assert!(
            parse_manifest(&OK.replace(r#""tileable":"grass""#, r#""tileable":"sand""#)).is_err()
        );
        assert!(
            parse_manifest(&OK.replace(
                r#""masks":"p1/meadow_base/meadow_base.masks""#,
                r#""orm":"x""#
            ))
            .is_err()
        );
    }

    #[test]
    fn tile_uv_scale_needs_a_tileable_set_and_a_positive_factor() {
        let uv = OK.replace(r#""masks":true}"#, r#""masks":true,"tileUvScale":16.0}"#);
        let m = parse_manifest(&uv).unwrap();
        assert_eq!(
            m.pieces[0].slots["meadow_base_cap"].tile_uv_scale,
            Some(16.0)
        );
        let no_set = uv.replace(r#""tileable":"grass","#, "");
        assert!(parse_manifest(&no_set).unwrap_err().contains("tileUvScale"));
        let zero = uv.replace("16.0", "0.0");
        assert!(parse_manifest(&zero).unwrap_err().contains("tileUvScale"));
    }

    #[test]
    fn misspelled_piece_key_is_an_error() {
        assert!(
            parse_manifest(&OK.replace(r#""kind":"base""#, r#""kind":"base","slts":{}"#)).is_err()
        );
    }

    #[test]
    fn channel_color_spaces() {
        assert!(Channel::Albedo.is_srgb() && Channel::Emissive.is_srgb());
        assert!(!Channel::Normal.is_srgb() && !Channel::Orm.is_srgb() && !Channel::Masks.is_srgb());
        assert_eq!(Channel::parse("orm"), Some(Channel::Orm));
        assert_eq!(Channel::parse("rgb"), None);
    }

    #[test]
    fn budgets() {
        assert_eq!(PieceKind::Base.lod0_budget(), 4_000);
        assert_eq!(PieceKind::Tree.lod0_budget(), 3_000);
        assert!((APOTHEM - 13.856_406).abs() < 1e-4);
    }

    #[test]
    fn budget_classes_match_the_owner_ruling() {
        assert_eq!(BudgetClass::Piece.ktx_max(), 1_500_000);
        assert_eq!(BudgetClass::Tileable.ktx_max(), 3_600_000);
        assert_eq!(BudgetClass::Hero.ktx_max(), 11_500_000);
        assert_eq!(BudgetClass::parse("hero"), Some(BudgetClass::Hero));
        assert_eq!(BudgetClass::parse("huge"), None);
    }
}
