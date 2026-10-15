//! `assets/kit/manifest.json`: kit pieces, their material slots and sidecar maps (R6).
use crate::{TextureFamily, family_suffix};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const GLB_MAX_BYTES: u64 = 2_097_152; // PRD §10
/// KTX2 bytes per texture family (BC7 and ASTC counted separately), by budget class. Owner ruling
/// 2026-10-13. Mirrors `KTX_BUDGETS` in `scripts/assets/kit-lib.mjs`; change both together (S33).
pub const KTX_BUDGETS: [(BudgetClass, u64); 3] = [
    (BudgetClass::Piece, 1_500_000),
    (BudgetClass::Tileable, 3_600_000),
    (BudgetClass::Hero, 11_500_000),
];
/// The pack loaded before the first interaction (PRD §10): the start meadow, the road family and
/// their sets. A piece or set without a `pack` is core.
pub const CORE_PACK: &str = "core";
/// Every pack id, `core` first; the other packs stream after the first interaction in this order
/// (PRD §10 "Kit content streams in by biome pack"). A pack id outside this list is a manifest
/// error, so a typo cannot start a pack of its own.
pub const PACKS: [&str; 6] = [CORE_PACK, "props", "forest", "hero", "village", "water"];
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
    /// Streaming pack (one of [`PACKS`]); `None` is [`CORE_PACK`].
    #[serde(default)]
    pub pack: Option<String>,
}

impl TileSet {
    pub fn pack(&self) -> &str {
        self.pack.as_deref().unwrap_or(CORE_PACK)
    }
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
    /// A plant that is neither alpha-tested nor part of a tree (grass tufts, reeds, hedges): it
    /// takes the per-instance seasonal tint like the other foliage (PRD §9.4).
    #[serde(default)]
    pub foliage: bool,
    /// Takes the live swash wet band at the waterline (`docs/kit/water-contract.md` §3): sand
    /// darkens and glosses where the swash runs up, as on the look-dev's placeholder bed.
    #[serde(default)]
    pub wet_band: bool,
    /// Sways in the shared wind (PRD §9.1 foliage sway): the amplitude in metres per unit of wind
    /// push, read with the mesh's TEXCOORD_1 (`wind`: U sway weight, V phase). A tree piece leans
    /// from its trunk base by height and flexes its branches by U; any other piece bends by U².
    /// Every slot of one plant takes the same value so its parts move together.
    #[serde(default)]
    pub wind: Option<f32>,
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
    /// Streaming pack (one of [`PACKS`]); `None` is [`CORE_PACK`].
    #[serde(default)]
    pub pack: Option<String>,
}

impl PieceEntry {
    pub fn pack(&self) -> &str {
        self.pack.as_deref().unwrap_or(CORE_PACK)
    }

    /// Tileable sets the piece's slots sample.
    pub fn tileable_sets(&self) -> BTreeSet<&str> {
        self.slots
            .values()
            .filter_map(|s| s.tileable.as_deref())
            .collect()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitManifest {
    pub version: u32,
    pub materials: BTreeMap<String, TileSet>,
    pub pieces: Vec<PieceEntry>,
}

impl KitManifest {
    /// Packs that hold at least one piece or set, in [`PACKS`] order.
    pub fn packs(&self) -> Vec<&'static str> {
        PACKS
            .into_iter()
            .filter(|k| {
                self.pieces.iter().any(|p| p.pack() == *k)
                    || self.materials.values().any(|s| s.pack() == *k)
            })
            .collect()
    }

    /// The pieces in `pack`.
    pub fn pack_pieces<'a>(&'a self, pack: &'a str) -> impl Iterator<Item = &'a PieceEntry> {
        self.pieces.iter().filter(move |p| p.pack() == pack)
    }

    /// Tileable sets a pack loads: the sets it owns and every set its pieces sample, whichever
    /// pack owns it (bark for the hero oak, gravel for the ocean seabed).
    pub fn pack_sets(&self, pack: &str) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self
            .materials
            .iter()
            .filter(|(_, s)| s.pack() == pack)
            .map(|(id, _)| id.clone())
            .collect();
        for p in self.pack_pieces(pack) {
            out.extend(p.tileable_sets().into_iter().map(str::to_owned));
        }
        out
    }

    /// The pack of piece `id`, if the manifest has it.
    pub fn piece_pack(&self, id: &str) -> Option<&str> {
        self.pieces
            .iter()
            .find(|p| p.id == id)
            .map(PieceEntry::pack)
    }
}

pub fn parse_manifest(json: &str) -> Result<KitManifest, String> {
    let m: KitManifest = serde_json::from_str(json).map_err(|e| format!("manifest: {e}"))?;
    let known = |owner: &str, pack: &str| {
        if PACKS.contains(&pack) {
            Ok(())
        } else {
            Err(format!("{owner}: unknown pack {pack} (known: {PACKS:?})"))
        }
    };
    for (id, s) in &m.materials {
        known(id, s.pack())?;
    }
    for p in &m.pieces {
        known(&p.id, p.pack())?;
        // Core must be complete on its own: it is everything before the first interaction.
        if p.pack() == CORE_PACK
            && let Some(set) = p
                .tileable_sets()
                .into_iter()
                .find(|s| m.materials.get(*s).is_some_and(|t| t.pack() != CORE_PACK))
        {
            return Err(format!(
                "{}: core piece samples set {set} from pack {}; core sets must be core",
                p.id,
                m.materials[set].pack()
            ));
        }
    }
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
    fn packs_default_to_core_and_must_be_known() {
        let m = parse_manifest(OK).unwrap();
        assert_eq!(m.pieces[0].pack(), CORE_PACK);
        assert_eq!(m.materials["grass"].pack(), CORE_PACK);
        assert_eq!(m.packs(), vec![CORE_PACK]);
        let typo = OK.replace(r#""kind":"base""#, r#""kind":"base","pack":"forrest""#);
        assert!(
            parse_manifest(&typo)
                .unwrap_err()
                .contains("unknown pack forrest")
        );
    }

    #[test]
    fn core_pieces_need_core_sets_and_packs_load_their_pieces_sets() {
        let streamed_set = OK.replace(r#""tileM":2.0,"#, r#""tileM":2.0,"pack":"forest","#);
        let e = parse_manifest(&streamed_set).unwrap_err();
        assert!(
            e.contains("core piece samples set grass from pack forest"),
            "{e}"
        );
        let both = streamed_set.replace(r#""kind":"base""#, r#""kind":"base","pack":"hero""#);
        let m = parse_manifest(&both).unwrap();
        assert_eq!(m.packs(), vec!["forest", "hero"]);
        assert_eq!(m.piece_pack("meadow_base"), Some("hero"));
        // The hero pack loads the forest-owned set its piece samples; forest owns it.
        assert!(m.pack_sets("hero").contains("grass"));
        assert!(m.pack_sets("forest").contains("grass"));
        assert_eq!(m.pack_pieces("forest").count(), 0);
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
