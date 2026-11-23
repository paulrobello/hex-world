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

    /// Whether slot `slot` is one of [`PieceTable::not_ground`].
    pub fn is_not_ground(&self, slot: &str) -> bool {
        self.not_ground.iter().any(|s| s == slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r#"(id: "x", label: Some(Rock), rise: Tree, walk: Some(Obstacle))"#;

    fn table(entry: &str) -> Result<PieceTable, ron::error::SpannedError> {
        PieceTable::parse(&format!("(pieces: [{entry}], parts: [], not_ground: [])"))
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
