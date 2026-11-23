//! Kit-piece runtime metadata (ENH-001): everything the runtime knows about a kit piece beyond
//! drawing it, from `data/pieces.ron` (embedded, so the wasm build needs no filesystem and the
//! lookups stay pure functions). The manifest keeps the draw data; this table keeps what play,
//! walk, scatter and the grass read per piece. Every prop, tree and hero piece of the manifest has
//! one entry (`kit_gate`), and every field is spelled out: an entry with a missing field does not
//! parse.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// `data/pieces.ron`.
pub const BUILTIN_PIECES: &str = include_str!("../../../data/pieces.ron");

/// What a prop is to the player. [`PropLabel::as_str`] is the string walk mode's blocked reason
/// and photo mode's click to focus report, which the bridge and `docs/agent-operability.md`
/// publish.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum PropLabel {
    Building,
    Tree,
    Hay,
    Animal,
    Log,
    Fence,
    Lever,
    BufferStop,
    Boat,
    Stage,
    Well,
    Rock,
}

impl PropLabel {
    pub const fn as_str(self) -> &'static str {
        match self {
            PropLabel::Building => "building",
            PropLabel::Tree => "tree",
            PropLabel::Hay => "hay",
            PropLabel::Animal => "animal",
            PropLabel::Log => "log",
            PropLabel::Fence => "fence",
            PropLabel::Lever => "lever",
            PropLabel::BufferStop => "buffer_stop",
            PropLabel::Boat => "boat",
            PropLabel::Stage => "stage",
            PropLabel::Well => "well",
            PropLabel::Rock => "rock",
        }
    }
}

/// How a hero prop grows when its tile is placed: a tree scales up, a building rises a floor at a
/// time (roof included).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Rise {
    Tree,
    Building { floors: u8 },
}

/// What a prop is to the walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum WalkClass {
    /// Its collider blocks the walker.
    Obstacle,
    /// The walker deliberately passes through it (low flora, reeds, pebbles).
    WalkThrough,
    /// A hero that is ground: the walker stands on it and scatter keeps off it.
    Ground,
}

/// One kit piece's runtime metadata.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    pub id: String,
    /// The click and blocked label; `None` for a piece nothing can hit or pick.
    #[serde(deserialize_with = "Option::deserialize")]
    pub label: Option<PropLabel>,
    pub rise: Rise,
    /// `None` for a piece the catalog never places as a static prop (traffic, agents, bores).
    #[serde(deserialize_with = "Option::deserialize")]
    pub walk: Option<WalkClass>,
    /// The scatter variants a rule naming this piece draws among, after the piece itself.
    pub variants: Vec<String>,
    /// Pick weights of the piece then its `variants` in order; `None` draws each equally.
    #[serde(deserialize_with = "Option::deserialize")]
    pub variant_weights: Option<Vec<u32>>,
    /// Pick weights that replace `variant_weights` on a road or rail verge tile.
    #[serde(deserialize_with = "Option::deserialize")]
    pub verge_variant_weights: Option<Vec<u32>>,
    /// The variants a hero slot authored as this piece may draw instead (`crate::variant`).
    pub hero_variants: Vec<String>,
    /// Its horizontal reach; `None` takes [`Footprint::DEFAULT`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub footprint: Option<Footprint>,
    /// Its collider's bounds when the collider is a rectangle.
    #[serde(deserialize_with = "Option::deserialize")]
    pub ground_box: Option<GroundBox>,
    /// Its fence line and posts, for a fence piece.
    #[serde(deserialize_with = "Option::deserialize")]
    pub fence: Option<FenceDef>,
    /// Parts standing outside its collider that block the walker as the piece, each a square's
    /// corners in the piece frame (x, z).
    pub posts: Vec<[[f32; 2]; 4]>,
    /// Convex parts in the piece frame (`[x min, x max, z min, z max, top]`) that replace the GLB
    /// collider's one hull for the walker.
    #[serde(deserialize_with = "Option::deserialize")]
    pub declared_parts: Option<Vec<[f32; 5]>>,
}

/// Horizontal reach of a piece in metres: `ground` is its collider's (what it stands on and what
/// others must not overlap) and `crown` its LOD0 canopy's (0 for low props).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    pub ground: f32,
    pub crown: f32,
}

impl Footprint {
    /// The reach of a piece with `footprint: None` (and of any id without an entry).
    pub const DEFAULT: Footprint = Footprint {
        ground: 1.0,
        crown: 0.0,
    };
}

/// A rectangular piece's collider bounds in its own frame (x, z), before its yaw.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundBox {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// A fence's layout in its own frame (x, z): the fence line (runs between consecutive points; the
/// gap between the last and first point is the gate opening) and every post's centre and half
/// width.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceDef {
    pub line: Vec<[f32; 2]>,
    pub posts: Vec<([f32; 2], f32)>,
}

/// A centre piece's part that blocks the walker: the centre, the part's name and its footprint
/// corners in the tile frame (x, z).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentreObstacle {
    pub centre: String,
    pub part: String,
    pub corners: [[f32; 2]; 4],
}

/// A tile's standing rock that blocks the walker: the tile, the part's name and the convex parts
/// of its footprint in the tile frame (x, z).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintObstacle {
    pub tile: String,
    pub part: String,
    pub parts: Vec<Vec<[f32; 2]>>,
}

/// A collider part that is not a kit piece of its own (a centre's lever, a viaduct pier, a
/// landform's rock foot), with the label walk and photo mode report for it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartDef {
    pub id: String,
    pub label: PropLabel,
}

/// `data/pieces.ron` parsed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceTable {
    pub pieces: Vec<PieceDef>,
    pub parts: Vec<PartDef>,
    /// Slots of ground pieces that stand on the ground rather than being it: nothing stands on
    /// them and the walker walks through, or under, them.
    pub not_ground: Vec<String>,
    /// Ground slots a car's wheels rest on: the road, and a level crossing's deck and rails.
    pub car_ground: Vec<String>,
    pub centre_obstacles: Vec<CentreObstacle>,
    pub footprint_obstacles: Vec<FootprintObstacle>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl PieceTable {
    /// Parses a pieces file. Duplicate ids are a data error the validator reports
    /// (`data_validator`); the lookups take the first.
    pub fn parse(text: &str) -> Result<Self, ron::error::SpannedError> {
        let mut table: Self = ron::from_str(text)?;
        for (i, p) in table.pieces.iter().enumerate() {
            table.index.entry(p.id.clone()).or_insert(i);
        }
        Ok(table)
    }

    /// The embedded `data/pieces.ron`, parsed once.
    pub fn builtin() -> &'static PieceTable {
        static TABLE: LazyLock<PieceTable> = LazyLock::new(|| {
            PieceTable::parse(BUILTIN_PIECES).unwrap_or_else(|e| panic!("data/pieces.ron: {e}"))
        });
        &TABLE
    }

    /// The entry of kit piece `id`.
    pub fn get(&self, id: &str) -> Option<&PieceDef> {
        self.index.get(id).map(|&i| &self.pieces[i])
    }

    /// The label of piece or part `id`.
    pub fn label(&self, id: &str) -> Option<PropLabel> {
        self.get(id)
            .and_then(|p| p.label)
            .or_else(|| self.parts.iter().find(|p| p.id == id).map(|p| p.label))
    }

    /// Whether piece `id` is of walk class `walk`.
    pub fn is_walk(&self, id: &str, walk: WalkClass) -> bool {
        self.get(id).is_some_and(|p| p.walk == Some(walk))
    }

    /// The pieces of walk class `walk`, in table order.
    pub fn with_walk(&self, walk: WalkClass) -> impl Iterator<Item = &str> {
        self.pieces
            .iter()
            .filter(move |p| p.walk == Some(walk))
            .map(|p| p.id.as_str())
    }

    /// The piece whose scatter `variants` list `piece`, if any.
    pub fn family(&self, piece: &str) -> Option<&str> {
        self.pieces
            .iter()
            .find(|p| p.variants.iter().any(|v| v == piece))
            .map(|p| p.id.as_str())
    }

    /// The reach of piece `id`: its `footprint`, else [`Footprint::DEFAULT`].
    pub fn footprint(&self, id: &str) -> Footprint {
        self.get(id)
            .and_then(|p| p.footprint)
            .unwrap_or(Footprint::DEFAULT)
    }

    /// Whether slot `slot` is one of [`PieceTable::car_ground`].
    pub fn is_car_ground(&self, slot: &str) -> bool {
        self.car_ground.iter().any(|s| s == slot)
    }

    /// Whether slot `slot` is one of [`PieceTable::not_ground`].
    pub fn is_not_ground(&self, slot: &str) -> bool {
        self.not_ground.iter().any(|s| s == slot)
    }
}

/// `&'static` views of the builtin table's id lists, so lookups can return slices of `&str`.
struct Views {
    variants: HashMap<&'static str, Vec<&'static str>>,
    hero_variants: HashMap<&'static str, Vec<&'static str>>,
}

fn views() -> &'static Views {
    static VIEWS: LazyLock<Views> = LazyLock::new(|| {
        let view = |list: fn(&'static PieceDef) -> &'static Vec<String>| {
            PieceTable::builtin()
                .pieces
                .iter()
                .map(|p| (p.id.as_str(), list(p).iter().map(String::as_str).collect()))
                .collect()
        };
        Views {
            variants: view(|p| &p.variants),
            hero_variants: view(|p| &p.hero_variants),
        }
    });
    &VIEWS
}

/// The builtin scatter variants of `piece` (its `variants`), empty for a piece without any.
pub fn scatter_variants(piece: &str) -> &'static [&'static str] {
    views().variants.get(piece).map_or(&[], |v| v)
}

/// The builtin hero variants of `piece` (its `hero_variants`), empty for a piece without any.
pub fn builtin_hero_variants(piece: &str) -> &'static [&'static str] {
    views().hero_variants.get(piece).map_or(&[], |v| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r#"(id: "x", label: Some(Rock), rise: Tree, walk: Some(Obstacle), variants: [], variant_weights: None, verge_variant_weights: None, hero_variants: [], footprint: Some((ground: 1.5, crown: 0.0)), ground_box: None, fence: None, posts: [], declared_parts: None)"#;

    fn table(entry: &str) -> Result<PieceTable, ron::error::SpannedError> {
        PieceTable::parse(&format!(
            "(pieces: [{entry}], parts: [], not_ground: [], car_ground: [], centre_obstacles: [], footprint_obstacles: [])"
        ))
    }

    #[test]
    fn the_builtin_table_parses() {
        assert!(!PieceTable::builtin().pieces.is_empty());
    }

    /// Every field is required, `Option` ones included, and an unknown field is an error: a new
    /// entry cannot silently take a default.
    #[test]
    fn an_entry_with_a_missing_or_unknown_field_does_not_parse() {
        assert!(table(ENTRY).is_ok());
        for field in [
            "label: Some(Rock), ",
            "walk: Some(Obstacle)",
            "rise: Tree, ",
        ] {
            let entry = ENTRY.replace(field, "");
            assert!(table(&entry).is_err(), "parsed without {field}: {entry}");
        }
        let extra = ENTRY.replace("rise: Tree", "rise: Tree, colour: 3");
        assert!(table(&extra).is_err(), "{extra}");
    }
}
