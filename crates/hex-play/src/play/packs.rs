//! Kit streaming by pack (PRD §10: "Initial download before first interaction ≤30 MB", "Kit content
//! streams in by biome pack after the first interaction").
//!
//! Each manifest piece and tileable set names a pack (`hex_assets::PACKS`). The play scenario loads
//! `core` on entry. A pack is requested on demand the moment a tile that draws one of its pieces is
//! selected (`PackRequests::selected`) or in the view (placed, or previewed as the ghost), and
//! after the first interaction (a change of selection, or the first change to the view after the
//! start world) the remaining packs stream one at a time in `PACKS` order. A pack's request fetches its pieces' GLBs and every map their slots sample
//! (sets owned by other packs included) plus the sets it owns, and it is ready only once all of
//! them have loaded, so a tile hidden while it waits appears straight in the textured kit look.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use bevy::{gltf::Gltf, prelude::*};
use hex_assets::{CORE_PACK, Channel, KitManifest, TextureFamily, map_file};
use hex_core::{Axial, TileCatalog, TileDef};

use super::PlayView;
use crate::kit_material::{MapRequest, load_map, slot_requests};

/// Native only: frames a streamed (non-core) pack waits after its request before its files are
/// fetched, so a shot can catch a tile while it waits (`docs/checkpoints/streaming/`). Disk
/// loads otherwise land within a few frames, inside the rise.
pub const HOLD_ENV: &str = "HEXW_PACK_HOLD_FRAMES";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackState {
    /// Not requested yet.
    Idle,
    /// Requested: held (`HOLD_ENV`) or fetching.
    Loading,
    /// Every file loaded (or failed: its pieces draw as placeholders).
    Ready,
}

impl PackState {
    pub fn as_str(self) -> &'static str {
        match self {
            PackState::Idle => "idle",
            PackState::Loading => "loading",
            PackState::Ready => "ready",
        }
    }
}

struct Pack {
    state: PackState,
    /// Frames left before the files are fetched.
    hold: u32,
    fetched: bool,
    glbs: Vec<(String, Handle<Gltf>)>,
    images: Vec<Handle<Image>>,
}

/// Requests from `hex-app`: `selected` is the selected tile (its packs load at once; a change of
/// selection is an interaction), and `all` loads every pack at once (the thumbnail run).
#[derive(Resource, Default)]
pub struct PackRequests {
    pub selected: Option<String>,
    pub all: bool,
}

/// The play kit's packs: their load state, the first-interaction trigger, and the placed tiles
/// still hidden for a pack that has not arrived.
#[derive(Resource)]
pub struct PlayPacks {
    pub(super) manifest: KitManifest,
    pub(super) family: TextureFamily,
    packs: BTreeMap<&'static str, Pack>,
    /// Pack ids in `PACKS` order: the background stream order.
    order: Vec<&'static str>,
    /// The first non-empty view; any later change to it is the first interaction.
    baseline: Option<PlayView>,
    /// The selection first seen; selecting another tile is an interaction too.
    first_selected: Option<String>,
    interacted: bool,
    hold_frames: u32,
    waiting: Vec<(Axial, String, Vec<&'static str>)>,
    /// Pieces whose pack arrived without them (their GLB failed for good): they draw placeholders
    /// for the session, and a tile's ground is built without them (`scatter::ground_tris`).
    lost: BTreeSet<String>,
}

fn hold_frames() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(n) = std::env::var(HOLD_ENV).ok().and_then(|v| v.parse().ok()) {
        return n;
    }
    0
}

/// Kit pieces a placed tile draws: base, centre, edge pieces and hero props. Scatter props are left
/// out: they do not make a tile wait, and `scatter` draws them once their pack lands. A hero slot
/// counts every variant it may draw (`TileDef::hero_options`), whichever its cell picks.
pub fn drawn_pieces<'a>(def: &'a TileDef, catalog: &'a TileCatalog) -> BTreeSet<&'a str> {
    let scatter: BTreeSet<&str> = def.props.scatter.iter().map(|s| s.piece.as_str()).collect();
    def.kit_refs(catalog.sockets())
        .into_iter()
        .filter(|r| !scatter.contains(r))
        .collect()
}

impl PlayPacks {
    pub fn new(manifest: KitManifest, family: TextureFamily) -> Self {
        let order = manifest.packs();
        let packs = order
            .iter()
            .copied()
            .map(|k| {
                let p = Pack {
                    state: PackState::Idle,
                    hold: 0,
                    fetched: false,
                    glbs: vec![],
                    images: vec![],
                };
                (k, p)
            })
            .collect();
        Self {
            manifest,
            family,
            packs,
            order,
            baseline: None,
            first_selected: None,
            interacted: false,
            hold_frames: hold_frames(),
            waiting: vec![],
            lost: BTreeSet::new(),
        }
    }

    /// Records a piece of an arrived pack that failed to build.
    pub fn mark_lost(&mut self, id: &str) {
        self.lost.insert(id.to_owned());
    }

    /// The piece's pack arrived without it.
    pub fn lost(&self, id: &str) -> bool {
        self.lost.contains(id)
    }

    pub fn state(&self, pack: &str) -> Option<PackState> {
        self.packs.get(pack).map(|p| p.state)
    }

    /// Core has loaded, or the manifest has no core pack.
    pub fn core_ready(&self) -> bool {
        self.state(CORE_PACK).is_none_or(|s| s == PackState::Ready)
    }

    pub fn all_ready(&self) -> bool {
        self.packs.values().all(|p| p.state == PackState::Ready)
    }

    /// Requests `pack` if it is idle. Core is fetched at once; a streamed pack after the hold.
    pub fn request(&mut self, pack: &str, server: &AssetServer) {
        let hold = if pack == CORE_PACK {
            0
        } else {
            self.hold_frames
        };
        let Some(p) = self.packs.get_mut(pack) else {
            return;
        };
        if p.state != PackState::Idle {
            return;
        }
        p.state = PackState::Loading;
        p.hold = hold;
        info!("play: pack {pack} requested");
        if hold == 0 {
            self.fetch(pack, server);
        }
    }

    fn fetch(&mut self, pack: &str, server: &AssetServer) {
        let m = &self.manifest;
        let mut maps: Vec<MapRequest> = vec![];
        let mut glbs = vec![];
        for e in m.pack_pieces(pack) {
            glbs.push((e.id.clone(), server.load(format!("kit://{}", e.glb))));
            for slot in e.slots.values() {
                if let Ok(r) = slot_requests(m, e, slot, self.family) {
                    maps.extend(r.masks);
                    maps.extend(r.tile.into_iter().flat_map(|(t, _)| t));
                    maps.extend(r.unique.into_iter().map(|(_, mr)| mr));
                }
            }
        }
        for set in m.materials.values().filter(|s| s.pack() == pack) {
            for (ch, stem) in &set.maps {
                maps.push(MapRequest {
                    path: map_file(stem, self.family),
                    srgb: Channel::is_srgb(*ch),
                    repeat: true,
                });
            }
        }
        let mut seen = HashSet::new();
        maps.retain(|r| seen.insert(r.path.clone()));
        let images = maps.iter().map(|r| load_map(server, r)).collect();
        if let Some(p) = self.packs.get_mut(pack) {
            p.glbs = glbs;
            p.images = images;
            p.fetched = true;
        }
    }

    /// Demand, the first-interaction trigger and the background queue, once per frame.
    pub fn update(
        &mut self,
        view: &PlayView,
        view_changed: bool,
        catalog: &TileCatalog,
        requests: Option<&PackRequests>,
        server: &AssetServer,
    ) {
        let ids = self.order.clone();
        if requests.is_some_and(|r| r.all) {
            for k in &ids {
                self.request(k, server);
            }
        }
        if let Some(sel) = requests.and_then(|r| r.selected.as_deref()) {
            if let Some(def) = catalog.get(sel) {
                for k in self.tile_packs(def, catalog) {
                    self.request(k, server);
                }
            }
            match &self.first_selected {
                None => self.first_selected = Some(sel.to_owned()),
                Some(first) if !self.interacted && first != sel => {
                    self.interacted = true;
                    info!(
                        "play: first interaction (selected {sel}), streaming the remaining packs"
                    );
                }
                _ => {}
            }
        }
        // Until the baseline is taken the view is read every frame: the frame that wrote the start
        // world may be the one that created this resource, before `update` ran.
        if view_changed || self.baseline.is_none() {
            let tiles = view.tiles.iter().map(|t| t.tile.as_str());
            let ghost = view.ghost.iter().map(|g| g.tile.as_str());
            let mut want = BTreeSet::new();
            for def in tiles.chain(ghost).filter_map(|id| catalog.get(id)) {
                want.extend(self.tile_packs(def, catalog));
            }
            for k in want {
                self.request(k, server);
            }
            match &self.baseline {
                None if !view.tiles.is_empty() => self.baseline = Some(view.clone()),
                Some(b) if !self.interacted && b != view => {
                    self.interacted = true;
                    info!("play: first interaction, streaming the remaining packs");
                }
                _ => {}
            }
        }
        // One background pack at a time, after the first interaction and after core.
        let busy = self.packs.values().any(|p| p.state == PackState::Loading);
        if self.interacted
            && self.core_ready()
            && !busy
            && let Some(next) = ids.iter().find(|k| self.state(k) == Some(PackState::Idle))
        {
            self.request(next, server);
        }
        for k in ids {
            let fetch = self.packs.get_mut(k).is_some_and(|p| {
                if p.state == PackState::Loading && !p.fetched {
                    p.hold = p.hold.saturating_sub(1);
                    p.hold == 0
                } else {
                    false
                }
            });
            if fetch {
                self.fetch(k, server);
            }
        }
    }

    /// Packs whose files have all loaded since the last poll.
    pub fn poll(&mut self, server: &AssetServer) -> Vec<&'static str> {
        let mut out = vec![];
        for (k, p) in &mut self.packs {
            if p.state != PackState::Loading || !p.fetched {
                continue;
            }
            let glbs_done = p.glbs.iter().all(|(_, h)| {
                server.is_loaded_with_dependencies(h)
                    || server.load_state(h).is_failed()
                    || server.recursive_dependency_load_state(h).is_failed()
            });
            let images_done = p.images.iter().all(|h| {
                let s = server.load_state(h);
                s.is_loaded() || s.is_failed()
            });
            if glbs_done && images_done {
                p.state = PackState::Ready;
                info!("play: pack {k} ready");
                out.push(*k);
            }
        }
        out
    }

    /// The loaded GLB of piece `id`, once its pack is ready.
    pub fn glb(&self, id: &str) -> Option<&Handle<Gltf>> {
        self.packs
            .values()
            .filter(|p| p.state == PackState::Ready)
            .flat_map(|p| &p.glbs)
            .find(|(g, _)| g == id)
            .map(|(_, h)| h)
    }

    /// Packs holding the pieces a tile draws (pieces outside the manifest draw placeholders).
    pub fn tile_packs(&self, def: &TileDef, catalog: &TileCatalog) -> BTreeSet<&'static str> {
        drawn_pieces(def, catalog)
            .into_iter()
            .filter_map(|id| self.manifest.piece_pack(id))
            .filter_map(|k| self.packs.get_key_value(k).map(|(k, _)| *k))
            .collect()
    }

    /// Whether a tile draws a piece from a pack that is not ready yet: its visual waits for it.
    pub fn tile_waiting(&self, def: &TileDef, catalog: &TileCatalog) -> bool {
        self.tile_packs(def, catalog)
            .iter()
            .any(|k| self.state(k) != Some(PackState::Ready))
    }

    /// Recomputes the placed tiles waiting on a pack.
    pub fn refresh_waiting(&mut self, view: &PlayView, catalog: &TileCatalog) {
        let mut waiting = vec![];
        for t in &view.tiles {
            let Some(def) = catalog.get(&t.tile) else {
                continue;
            };
            let packs: Vec<&'static str> = self
                .tile_packs(def, catalog)
                .into_iter()
                .filter(|k| self.state(k) != Some(PackState::Ready))
                .collect();
            if !packs.is_empty() {
                waiting.push((t.cell, t.tile.clone(), packs));
            }
        }
        waiting.sort_by_key(|(c, ..)| (c.q, c.r));
        self.waiting = waiting;
    }

    /// Every pack and its state, in stream order (the state JSON's `packs`).
    pub fn status(&self) -> impl Iterator<Item = (&'static str, PackState)> + '_ {
        self.order
            .iter()
            .filter_map(|k| self.state(k).map(|s| (*k, s)))
    }

    /// `(cell, tile, packs)`: placed tiles hidden until these packs arrive.
    pub fn waiting(&self) -> &[(Axial, String, Vec<&'static str>)] {
        &self.waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit_scene::MANIFEST;
    use hex_assets::parse_manifest;

    fn packs() -> PlayPacks {
        PlayPacks::new(parse_manifest(MANIFEST).unwrap(), TextureFamily::Png)
    }

    /// A house cluster loads every hero variant its slots may draw, so a cell that picks the
    /// longhouse never draws a placeholder; scatter still stays out.
    #[test]
    fn drawn_pieces_include_every_hero_variant() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        let drawn = drawn_pieces(def, &c);
        for piece in [
            "house_cottage",
            "house_longhouse",
            "house_cottage_b",
            "house_twostorey",
            "house_twostorey_b",
            "house_twostorey_c",
        ] {
            assert!(drawn.contains(piece), "{piece}: {drawn:?}");
        }
        for s in &def.props.scatter {
            assert!(!drawn.contains(s.piece.as_str()), "{}", s.piece.as_str());
        }
        assert_eq!(
            packs().manifest.piece_pack("house_longhouse"),
            Some("village")
        );
        let oak = drawn_pieces(c.get("oak").unwrap(), &c);
        assert!(
            oak.contains("tree_hero_oak")
                && oak.contains("tree_hero_oak_b")
                && oak.contains("tree_hero_oak_c"),
            "{oak:?}"
        );
        assert_eq!(packs().manifest.piece_pack("tree_hero_oak_b"), Some("hero"));
        assert_eq!(packs().manifest.piece_pack("tree_hero_oak_c"), Some("hero"));
    }

    #[test]
    fn tiles_need_their_drawn_pieces_packs() {
        let p = packs();
        let c = TileCatalog::builtin().unwrap();
        let need = |id: &str| p.tile_packs(c.get(id).unwrap(), &c);
        assert_eq!(need("meadow"), BTreeSet::from([CORE_PACK]));
        assert_eq!(need("road_straight"), BTreeSet::from([CORE_PACK]));
        // The pine forest needs its base; the oak tile its base and the hero oak. Scatter (spruces,
        // rocks, flowers) does not hold a tile back: each rule draws once its piece is in the kit.
        assert_eq!(need("forest"), BTreeSet::from(["forest"]));
        assert_eq!(need("oak"), BTreeSet::from(["forest", "hero"]));
        assert_eq!(
            need("house_cluster"),
            BTreeSet::from([CORE_PACK, "village"])
        );
        // The windmill stands on the core meadow base; its piece streams with the village.
        assert_eq!(need("windmill"), BTreeSet::from([CORE_PACK, "village"]));
        // The farm's barn streams with the village, its bales and animals with the props.
        assert_eq!(
            need("farm"),
            BTreeSet::from([CORE_PACK, "props", "village"])
        );
        // The wheat field's centre streams with the village (its request also loads the gravel its
        // earth samples, as house_pad's driveway does).
        assert_eq!(need("wheat_field"), BTreeSet::from([CORE_PACK, "village"]));
        assert_eq!(need("ocean"), BTreeSet::from(["water"]));
        assert_eq!(need("river_bend"), BTreeSet::from(["water"]));
        // The bridge's road lanes use the core edge piece.
        assert_eq!(
            need("river_road_bridge"),
            BTreeSet::from([CORE_PACK, "water"])
        );
    }

    #[test]
    fn every_pack_starts_idle_and_core_is_first() {
        let p = packs();
        assert_eq!(
            p.order,
            vec![CORE_PACK, "props", "forest", "hero", "village", "water"]
        );
        assert!(p.packs.values().all(|k| k.state == PackState::Idle));
        assert!(!p.core_ready() && !p.all_ready());
        let s: Vec<_> = p.status().collect();
        assert_eq!(s.len(), hex_assets::PACKS.len());
        assert_eq!(s[0], (CORE_PACK, PackState::Idle));
        assert_eq!(PackState::Loading.as_str(), "loading");
    }
}
