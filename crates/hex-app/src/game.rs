//! Play-scenario game state (PRD §4.1–4.2, D20): the hex-core world plus the player's selection,
//! ghost rotation and tool. [`GameState::apply`] is the only way an action changes it.
use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use hex_core::{
    Axial, EdgeStatus, Orientation, ReplaceError, RotateDir, TileCatalog, TileDef, World,
};
use hex_play::{
    ActiveScenario, Scenario,
    play::{PlayGhost, PlayTile, PlayView, rise::RiseMessage},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::actions::{Action, ActionQueue, Tool};
use crate::bridge;

/// Events mirrored into the state JSON (`play.recent_events`) for native runs, which never drain.
pub const RECENT_EVENTS: usize = 16;

/// Typed events for the JS side (`hexWorld.events()`); the variant name is the `type` field.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameEvent {
    TilePlaced {
        q: i32,
        r: i32,
        tile: String,
        rotation: u8,
        level: u8,
    },
    PlacementRefused {
        q: i32,
        r: i32,
        tile: String,
        rotation: u8,
        level: u8,
        reason: String,
    },
    TileBulldozed {
        q: i32,
        r: i32,
        tile: String,
    },
    /// The placed tile at `(q, r)` was swapped for `tile` at `rotation` in one undoable step;
    /// `replaced` is the tile id that was there.
    TileReplaced {
        q: i32,
        r: i32,
        tile: String,
        rotation: u8,
        level: u8,
        replaced: String,
    },
    BulldozeRefused {
        q: i32,
        r: i32,
        reason: String,
    },
    Undone,
    Redone,
    TileSelected {
        tile: String,
    },
    GhostRotated {
        rotation: u8,
        level: u8,
        valid: bool,
    },
    HoverChanged {
        cell: Option<Axial>,
    },
    /// The pinned slot changed (U4): a click on an empty slot pins it, `null` unpins.
    PinChanged {
        cell: Option<Axial>,
    },
    ToolChanged {
        mode: Tool,
    },
    InventoryToggled {
        open: bool,
    },
    PhotoModeToggled {
        on: bool,
    },
    /// First-person walk mode (PRD P8) turned on or off.
    WalkModeToggled {
        on: bool,
    },
    /// A photo-mode PNG export finished (PRD §4.6): natively written under `target/shots`, on the
    /// web taken with `hexWorld.takeShot(name)`. Sizes are the PNG's pixels.
    PhotoExported {
        name: String,
        width: u32,
        height: u32,
    },
    /// The export did not render, for example a 4× target past this GPU's texture limit.
    PhotoExportFailed {
        name: String,
        reason: String,
    },
    /// Any other refused action (undo with an empty stack, an unknown tile id, a click with
    /// nothing hovered).
    ActionRefused {
        action: &'static str,
        reason: String,
    },
    /// A stage of a tile's staged rise starts (PRD §4.4), for the audio layer's rumble and the
    /// HUD: `column`, `cap`, `edges`, `props`, `agents`, then `settled`; `sink` when an undo or a
    /// bulldoze takes a tile down. `skipped` cues arrive together from a fast-forward or under
    /// reduced motion. Not kept in `recent_events` (five per placement would crowd it out).
    RiseStage {
        q: i32,
        r: i32,
        tile: String,
        stage: &'static str,
        skipped: bool,
    },
    /// A car starts a trip, reversing out of its parking spot (PRD §7.1): the car's sim id, the
    /// spot's cell, and the spot's world XZ in metres. Not kept in `recent_events`.
    CarPulledOut {
        id: u64,
        q: i32,
        r: i32,
        x: f32,
        z: f32,
    },
    /// A car finished its trip and stands in its reserved spot, as [`GameEvent::CarPulledOut`].
    CarParked {
        id: u64,
        q: i32,
        r: i32,
        x: f32,
        z: f32,
    },
    /// A train moves off from a stand (P3): leaving a station stop, its first move after spawning,
    /// after its dwell at a line end, or after waiting for a junction or another train. The
    /// train's sim id and its front's world XZ in metres; the audio layer's whistle. Not kept in
    /// `recent_events`.
    TrainDeparted {
        id: u64,
        x: f32,
        z: f32,
    },
    /// A train comes to rest against a buffer stop at a line end (owner D2-Q5): the train's sim id
    /// and the stop's pivot, world XZ in metres; the audio layer's buffer clunk. Once per arrival,
    /// before its dwell. Not kept in `recent_events`.
    TrainAtLineEnd {
        id: u64,
        x: f32,
        z: f32,
    },
    /// A moving rowing boat's oars catch the water (one stroke, owner D4): the boat's id and its
    /// world XZ in metres; the audio layer's oar splash. Not kept in `recent_events`.
    BoatStroke {
        id: u64,
        x: f32,
        z: f32,
    },
    /// A sailing boat's boom crosses to the other side on a turn: its id and world XZ; the audio
    /// layer's sail flap. Not kept in `recent_events`.
    SailFlap {
        id: u64,
        x: f32,
        z: f32,
    },
    /// A `save` command encoded the world (PRD §12.2). On the web the bytes wait for
    /// `hexWorld.takeSave(tag)`; natively a `path` was written. `tag` echoes the command's.
    Saved {
        tag: Option<String>,
        tiles: u32,
        bytes: u32,
    },
    /// A save replaced the world, or `new_world` reset it. `version` is the file's format version
    /// before migration; `settings` is null for a new world, which keeps the current settings.
    Loaded {
        tiles: u32,
        version: u16,
        settings: Option<crate::persist::SettingsJson>,
    },
    /// A `.hexworld` file was refused and the world is unchanged. `code` names the check that
    /// failed (`bad_magic`, `truncated`, `newer_version`, ...); `version` is set for version refusals.
    LoadRefused {
        code: &'static str,
        reason: String,
        version: Option<u16>,
    },
}

/// The outcome of the latest game action (hover excluded), for `play.last_result`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActionResult {
    pub action: &'static str,
    pub ok: bool,
    pub cell: Option<Axial>,
    pub reason: Option<String>,
}

#[derive(Resource)]
pub struct GameState {
    pub world: World,
    pub catalog: TileCatalog,
    /// Selected tile id; always a catalog id.
    pub selected: String,
    /// The ghost orientation: rotation 0..6 and level.
    pub chosen: Orientation,
    /// The orientation the player last chose with R, per tile id. Hovering a slot or selecting the
    /// tile again restores it wherever it is legal, instead of snapping to the first legal one.
    pub chosen_orientation: BTreeMap<String, Orientation>,
    /// The member the player last chose per picker group (selected by id, or turned to with R), so
    /// a hover tries it first and only falls back to another member where it has no legal rotation.
    pub chosen_variant: BTreeMap<String, String>,
    pub tool: Tool,
    pub hovered: Option<Axial>,
    /// The pinned slot: the ghost, its rotation and the inventory stay here while the pointer moves.
    pub pinned: Option<Axial>,
    pub inventory_open: bool,
    pub photo_mode: bool,
    /// First-person walk mode (PRD P8): placement and bulldoze are off and the HUD hides.
    pub walk_mode: bool,
    pub last: Option<ActionResult>,
    pub recent: VecDeque<GameEvent>,
    /// Bumped by every applied game action; `sync_view` rebuilds the render view when it moves.
    pub revision: u64,
    /// Refused placements and bulldozes, so the renderer pulses the refused highlight each time.
    pub refusals: u32,
    /// Bumped whenever the tiles or the history change (a placement, a bulldoze, undo, redo or a
    /// replaced world), so `persist::snapshot` copies the world only then.
    pub world_rev: u64,
    /// Bumped when the world is replaced wholesale; the render view's `epoch` (drawn at rest).
    pub epoch: u64,
    /// The cell the latest world change bulldozed, so the renderer raises a dust cloud over its
    /// sink; any other world change (a placement, undo, redo, a replaced world) clears it.
    pub last_bulldoze: Option<Axial>,
}

impl GameState {
    pub fn new(seed: u64) -> Self {
        let catalog =
            TileCatalog::builtin().expect("the embedded catalog is validated by hex-core");
        let selected = catalog.tiles()[0].id.clone();
        Self {
            world: World::new(seed),
            catalog,
            selected,
            chosen: Orientation::default(),
            chosen_orientation: BTreeMap::new(),
            chosen_variant: BTreeMap::new(),
            tool: Tool::Place,
            hovered: None,
            pinned: None,
            inventory_open: false,
            photo_mode: false,
            walk_mode: false,
            last: None,
            recent: VecDeque::new(),
            revision: 1,
            refusals: 0,
            world_rev: 0,
            epoch: 0,
            last_bulldoze: None,
        }
    }

    fn selected_def(&self) -> &TileDef {
        self.catalog
            .get(&self.selected)
            .expect("selected is a catalog id")
    }

    /// The hovered cell when it is a placement slot.
    pub fn hovered_slot(&self) -> Option<Axial> {
        self.hovered.filter(|&c| self.world.is_slot(c))
    }

    /// The slot the ghost, rotation and inventory follow: the pinned slot, else the hovered one.
    pub fn target_slot(&self) -> Option<Axial> {
        self.pinned
            .filter(|&c| self.world.is_slot(c))
            .or_else(|| self.hovered_slot())
    }

    /// Sets the pin, returning its event when it changed.
    fn set_pin(&mut self, cell: Option<Axial>) -> Vec<GameEvent> {
        if self.pinned == cell {
            return vec![];
        }
        let before = self.target_slot();
        self.pinned = cell;
        // Pinning the slot under the pointer keeps the rotation the player turned there.
        self.resnap(self.target_slot() != before);
        vec![GameEvent::PinChanged { cell }]
    }

    /// Drops a pin whose cell is no longer a slot (after a world change).
    fn drop_stale_pin(&mut self) -> Vec<GameEvent> {
        match self.pinned {
            Some(c) if !self.world.is_slot(c) => self.set_pin(None),
            _ => vec![],
        }
    }

    /// The top (m) of the tile placed at `cell`, `(level + def.top()) * LEVEL_STEP`; `None` on an
    /// empty cell. Pointer picking and the orbit pivot stand on it (`camera::pick_cell`).
    pub fn top_of(&self, cell: Axial) -> Option<f32> {
        let t = self.world.tile(cell)?;
        let top = self.catalog.get(&t.def).map_or(0, |d| d.top());
        Some(f32::from(t.level + top) * hex_core::LEVEL_STEP)
    }

    /// The level each free slot's outline sits on, where it is not 0 ([`PlayView::slot_levels`]):
    /// the level of the selected tile's default orientation there (the first legal one, cliff-free
    /// first), else the highest neighbour's level when the tile fits nowhere. A slot whose
    /// neighbours are all level, unsloped ground is level 0 without asking.
    fn slot_levels(&self, frontier: &[Axial]) -> BTreeMap<Axial, u8> {
        let def = self.selected_def();
        frontier
            .iter()
            .filter_map(|&cell| {
                let around: Vec<(u8, bool)> = (0..6)
                    .filter_map(|e| self.world.tile(cell.neighbor(e)))
                    .map(|t| {
                        let sloped = self.catalog.get(&t.def).is_some_and(|d| d.is_sloped());
                        (t.level, sloped)
                    })
                    .collect();
                if around.iter().all(|&(level, sloped)| level == 0 && !sloped) {
                    return None;
                }
                let level = self
                    .world
                    .auto_orient(&self.catalog, cell, def)
                    .map_or_else(
                        || around.iter().map(|&(l, _)| l).max().unwrap_or(0),
                        |o| o.level,
                    );
                (level > 0).then_some((cell, level))
            })
            .collect()
    }

    fn legal_here(&self, cell: Axial) -> Vec<Orientation> {
        self.world
            .legal_orientations(&self.catalog, cell, self.selected_def())
    }

    /// The orientation `d` takes at `cell` among `legal` (cliff-free first): `preferred` where it
    /// is legal and as cliff-free as `legal[0]`, else `legal[0]`'s level at the preferred rotation
    /// where that is legal and as cliff-free, else `legal[0]`. So a cliff chosen at one cell is
    /// never the default at another, nor on coming back. `None` when `legal` is empty.
    fn pick(
        &self,
        cell: Axial,
        d: &TileDef,
        legal: &[Orientation],
        preferred: Option<Orientation>,
    ) -> Option<Orientation> {
        let first = *legal.first()?;
        let cliff = |o: Orientation| {
            self.world
                .cliff_edges(&self.catalog, cell, d, o)
                .contains(&true)
        };
        let fits = |o: &Orientation| legal.contains(o) && cliff(*o) == cliff(first);
        preferred
            .into_iter()
            .flat_map(|p| [p, Orientation::new(p.rotation, first.level)])
            .find(fits)
            .or(Some(first))
    }

    /// The tile and orientation the ghost snaps to on the free slot `cell` (PRD §4.2): the chosen
    /// member of the selected tile's picker group (the selected tile itself when it has none) at
    /// its chosen orientation as [`GameState::pick`] allows it, else the next member that fits,
    /// the same way. `None` when no member fits.
    fn snap_at(&self, cell: Axial) -> Option<(String, Orientation)> {
        let members = self.catalog.siblings(&self.selected);
        // Not `selected`: a hover that fell back to another member must not make it the preference.
        let preferred = self
            .catalog
            .group_of(&self.selected)
            .and_then(|g| self.chosen_variant.get(g))
            .or(members.first().map(|d| &d.id))
            .unwrap_or(&self.selected);
        let start = members.iter().position(|d| &d.id == preferred).unwrap_or(0);
        let n = members.len();
        (0..n).map(|i| members[(start + i) % n]).find_map(|d| {
            let legal = self.world.legal_orientations(&self.catalog, cell, d);
            self.pick(cell, d, &legal, self.chosen_orientation.get(&d.id).copied())
                .map(|k| (d.id.clone(), k))
        })
    }

    /// Auto-rotate (PRD §4.2): `snap` takes [`GameState::snap_at`]; otherwise the tile and
    /// orientation are kept while they are still legal. With nothing legal they are left as they
    /// are.
    fn resnap(&mut self, snap: bool) {
        if let Some(cell) = self.target_slot()
            && (snap || !self.legal_here(cell).contains(&self.chosen))
            && let Some((tile, o)) = self.snap_at(cell)
        {
            self.selected = tile;
            self.chosen = o;
        }
        self.ground_unraisable();
    }

    /// A kept orientation's level may not suit the selected tile (a meadow turned up to level 1,
    /// then the ocean selected where it fits nowhere): it drops to the ground.
    fn ground_unraisable(&mut self) {
        if !hex_core::level_allowed(self.selected_def(), self.chosen.level) {
            self.chosen.level = 0;
        }
    }

    /// Photo or walk mode: the world is only looked at, so no ghost, slot ring or bulldoze target.
    fn viewing(&self) -> bool {
        self.photo_mode || self.walk_mode
    }

    /// The ghost at the target slot, with the rotations legal at the ghost's level (ascending);
    /// none in bulldoze mode, photo mode or walk mode.
    pub fn ghost(&self) -> Option<(Axial, Vec<u8>, [EdgeStatus; 6], bool)> {
        if self.tool != Tool::Place || self.viewing() {
            return None;
        }
        let cell = self.target_slot()?;
        let def = self.selected_def();
        let edges = self
            .world
            .edge_status(&self.catalog, cell, def, self.chosen);
        let valid = self.world.fits(&self.catalog, cell, def, self.chosen);
        let mut legal: Vec<u8> = self
            .legal_here(cell)
            .into_iter()
            .filter(|o| o.level == self.chosen.level)
            .map(|o| o.rotation)
            .collect();
        legal.sort();
        Some((cell, legal, edges, valid))
    }

    /// The hovered tile in bulldoze mode, with the refusal reason when removing it is blocked.
    pub fn bulldoze_target(&self) -> Option<(Axial, Option<String>)> {
        if self.tool != Tool::Bulldoze || self.viewing() {
            return None;
        }
        let cell = self.hovered.filter(|c| self.world.tile(*c).is_some())?;
        Some((
            cell,
            self.world
                .can_bulldoze(&self.catalog, cell)
                .err()
                .map(|e| e.to_string()),
        ))
    }

    /// Applies one game action and returns its events. Camera actions are not game actions.
    pub fn apply(&mut self, action: Action) -> Vec<GameEvent> {
        if action.is_camera() {
            return vec![];
        }
        let events = self.apply_inner(action);
        self.revision += 1;
        for e in &events {
            self.recent.push_back(e.clone());
        }
        while self.recent.len() > RECENT_EVENTS {
            self.recent.pop_front();
        }
        events
    }

    fn result(&mut self, action: &'static str, cell: Option<Axial>, reason: Option<String>) {
        self.last = Some(ActionResult {
            action,
            ok: reason.is_none(),
            cell,
            reason,
        });
    }

    fn refuse(&mut self, action: &'static str, reason: String) -> Vec<GameEvent> {
        self.result(action, None, Some(reason.clone()));
        vec![GameEvent::ActionRefused { action, reason }]
    }

    fn apply_inner(&mut self, action: Action) -> Vec<GameEvent> {
        if self.walk_mode
            && let Some(name) = crate::walk::refused_while_walking(&action)
        {
            return self.refuse(name, "leave walk mode first".into());
        }
        match action {
            Action::Orbit(_)
            | Action::Pan(_)
            | Action::Zoom(_)
            | Action::Fly(_)
            | Action::Look(_)
            | Action::Stride(_)
            | Action::WalkLook(_)
            | Action::Hop => vec![],
            Action::ToggleWalk => self.set_walk(!self.walk_mode),
            Action::ExitWalk => self.set_walk(false),
            Action::SetWalk(on) => self.set_walk(on),
            Action::Hover(cell) => self.hover(cell),
            Action::SelectTile(id) => self.select_tile(id),
            Action::SelectNth(n) => match self.catalog.picker().get(n) {
                Some(entry) => {
                    let id = entry.id.clone();
                    self.select_tile(id)
                }
                None => self.refuse("select_tile", format!("no inventory tile {}", n + 1)),
            },
            Action::Rotate(dir) => self.rotate(dir),
            Action::Primary => self.primary(),
            Action::QuickPlace => self.quick_place_action(),
            Action::QuickReplace => match self.hovered {
                Some(cell) => self.replace(cell),
                None => self.refuse("replace", "no cell hovered".into()),
            },
            Action::Replace(cell) => self.replace(cell),
            Action::Place(cell) => self.place_at(cell),
            Action::PlaceAtLevel(cell, level) => self.place_at_level(cell, level),
            Action::Pin(cell) => self.pin_action(cell),
            Action::Unpin => self.unpin(),
            Action::Escape => self.escape(),
            Action::Bulldoze(cell) => self.bulldoze(cell),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::ToggleBulldoze => self.toggle_bulldoze(),
            Action::SetTool(mode) => self.set_tool(mode),
            Action::ToggleInventory => self.toggle_inventory(),
            Action::ExitPhoto if !self.photo_mode => vec![],
            Action::TogglePhoto | Action::ExitPhoto => self.toggle_photo(),
        }
    }

    fn toggle_photo(&mut self) -> Vec<GameEvent> {
        let mut events = if self.photo_mode {
            vec![]
        } else {
            self.set_pin(None)
        };
        self.photo_mode = !self.photo_mode;
        self.result("toggle_photo", None, None);
        events.push(GameEvent::PhotoModeToggled {
            on: self.photo_mode,
        });
        events
    }

    fn exit_photo(&mut self) -> Vec<GameEvent> {
        if self.photo_mode {
            self.toggle_photo()
        } else {
            vec![]
        }
    }

    fn set_walk(&mut self, on: bool) -> Vec<GameEvent> {
        if on == self.walk_mode {
            return vec![];
        }
        if on && self.photo_mode {
            return self.refuse("walk", "leave photo mode first".into());
        }
        let mut events = if on { self.set_pin(None) } else { vec![] };
        self.walk_mode = on;
        self.result("walk", None, None);
        events.push(GameEvent::WalkModeToggled { on });
        events
    }

    fn hover(&mut self, cell: Option<Axial>) -> Vec<GameEvent> {
        if cell == self.hovered {
            return vec![];
        }
        self.hovered = cell;
        if self.pinned.is_none() {
            self.resnap(true);
        }
        vec![GameEvent::HoverChanged { cell }]
    }

    /// Selects a tile id, or a picker group id (the Tiles sheet's Beach Curve), which selects the
    /// group's chosen member, else its first. Selecting a member by id makes it the chosen one.
    fn select_tile(&mut self, id: String) -> Vec<GameEvent> {
        let tile = if self.catalog.get(&id).is_some() {
            if let Some(g) = self.catalog.group_of(&id) {
                self.chosen_variant.insert(g.to_owned(), id.clone());
            }
            id
        } else if let Some(first) = self.catalog.group_members(&id).first() {
            self.chosen_variant
                .get(&id)
                .cloned()
                .unwrap_or_else(|| first.id.clone())
        } else {
            return self.refuse("select_tile", format!("unknown tile {id:?}"));
        };
        self.selected = tile;
        self.resnap(true);
        self.result("select_tile", None, None);
        vec![GameEvent::TileSelected {
            tile: self.selected.clone(),
        }]
    }

    fn rotate(&mut self, dir: RotateDir) -> Vec<GameEvent> {
        let cycled = self.target_slot().and_then(|cell| {
            let members = self.catalog.siblings(&self.selected);
            self.world
                .cycle_group(
                    &self.catalog,
                    cell,
                    &members,
                    (&self.selected, self.chosen),
                    dir,
                )
                .map(|(d, o)| (d.id.clone(), o))
        });
        // No legal orientation: still turn one step, so the edge markers show why.
        match cycled {
            Some((tile, o)) => {
                self.selected = tile;
                self.chosen = o;
            }
            None => {
                self.chosen.rotation = match dir {
                    RotateDir::Forward => (self.chosen.rotation + 1) % 6,
                    RotateDir::Back => (self.chosen.rotation + 5) % 6,
                };
                self.ground_unraisable();
            }
        }
        if let Some(g) = self.catalog.group_of(&self.selected) {
            self.chosen_variant
                .insert(g.to_owned(), self.selected.clone());
        }
        self.chosen_orientation
            .insert(self.selected.clone(), self.chosen);
        let valid = self.ghost().is_some_and(|g| g.3);
        self.result("rotate", self.target_slot().or(self.hovered), None);
        vec![GameEvent::GhostRotated {
            rotation: self.chosen.rotation,
            level: self.chosen.level,
            valid,
        }]
    }

    fn place_at(&mut self, cell: Axial) -> Vec<GameEvent> {
        let mut events = self.set_pin(None);
        events.extend(self.hover(Some(cell)));
        events.extend(self.place(cell));
        events
    }

    /// [`GameState::place_at`] on `level`: the ghost's rotation where it is legal there, else the
    /// first legal rotation there. With none legal the placement is attempted as asked and the
    /// world's own refusal (clashing edges, level out of range, no neighbour) is the event.
    fn place_at_level(&mut self, cell: Axial, level: u8) -> Vec<GameEvent> {
        let mut events = self.set_pin(None);
        events.extend(self.hover(Some(cell)));
        let legal = self.legal_here(cell);
        let on_level: Vec<Orientation> = legal.into_iter().filter(|o| o.level == level).collect();
        let preferred = Orientation::new(self.chosen.rotation, level);
        let def = self.selected_def().clone();
        // `pick` keeps the ghost's rotation only when it is as cliff-free as the first entry.
        let o = self
            .pick(cell, &def, &on_level, Some(preferred))
            .unwrap_or(preferred);
        let tile = self.selected.clone();
        let placed = self.place_with(cell, tile, o);
        if placed
            .iter()
            .any(|e| matches!(e, GameEvent::TilePlaced { .. }))
        {
            self.chosen = o;
            self.chosen_orientation.insert(self.selected.clone(), o);
        }
        events.extend(placed);
        events
    }

    fn pin_action(&mut self, cell: Axial) -> Vec<GameEvent> {
        if self.tool != Tool::Place || self.photo_mode || !self.world.is_slot(cell) {
            return self.refuse(
                "pin",
                format!("({}, {}) is not a free slot", cell.q, cell.r),
            );
        }
        self.pin(cell)
    }

    fn unpin(&mut self) -> Vec<GameEvent> {
        let events = self.set_pin(None);
        self.result("unpin", None, None);
        events
    }

    fn escape(&mut self) -> Vec<GameEvent> {
        if self.pinned.is_some() {
            self.unpin()
        } else {
            self.exit_photo()
        }
    }

    fn undo(&mut self) -> Vec<GameEvent> {
        if !self.world.undo() {
            return self.refuse("undo", "nothing to undo".into());
        }
        let mut events = vec![GameEvent::Undone];
        self.last_bulldoze = None;
        events.extend(self.drop_stale_pin());
        self.resnap(false);
        self.world_rev += 1;
        self.result("undo", None, None);
        events
    }

    fn redo(&mut self) -> Vec<GameEvent> {
        if !self.world.redo() {
            return self.refuse("redo", "nothing to redo".into());
        }
        let mut events = vec![GameEvent::Redone];
        self.last_bulldoze = None;
        events.extend(self.drop_stale_pin());
        self.resnap(false);
        self.world_rev += 1;
        self.result("redo", None, None);
        events
    }

    fn toggle_bulldoze(&mut self) -> Vec<GameEvent> {
        let mode = match self.tool {
            Tool::Place => Tool::Bulldoze,
            Tool::Bulldoze => Tool::Place,
        };
        self.set_tool(mode)
    }

    fn set_tool(&mut self, mode: Tool) -> Vec<GameEvent> {
        let mut events = if mode == Tool::Place {
            vec![]
        } else {
            self.set_pin(None)
        };
        self.tool = mode;
        self.resnap(false);
        self.result("set_tool", None, None);
        events.push(GameEvent::ToolChanged { mode });
        events
    }

    fn toggle_inventory(&mut self) -> Vec<GameEvent> {
        self.inventory_open = !self.inventory_open;
        self.result("toggle_inventory", None, None);
        vec![GameEvent::InventoryToggled {
            open: self.inventory_open,
        }]
    }

    fn primary(&mut self) -> Vec<GameEvent> {
        if self.walk_mode {
            return self.refuse("primary", "leave walk mode first".into());
        }
        match (self.tool, self.hovered) {
            (Tool::Place, _) if self.pinned.is_some() => self.click_while_pinned(),
            (_, None) => self.refuse("primary", "no cell hovered".into()),
            (Tool::Place, Some(cell)) if self.world.is_slot(cell) => self.pin(cell),
            (Tool::Place, Some(cell)) => self.place(cell),
            (Tool::Bulldoze, Some(cell)) => self.bulldoze(cell),
        }
    }

    fn quick_place_action(&mut self) -> Vec<GameEvent> {
        match self.hovered {
            Some(cell) if self.tool == Tool::Place && self.world.is_slot(cell) => {
                self.quick_place(cell)
            }
            _ => self.primary(),
        }
    }

    /// Pins `cell` (a free slot) so the ghost and the sheet stay there.
    fn pin(&mut self, cell: Axial) -> Vec<GameEvent> {
        let events = self.set_pin(Some(cell));
        self.result("pin", Some(cell), None);
        events
    }

    /// Left-click with a pinned slot: on it places, on another free slot pins that one, and
    /// anywhere else unpins without a refusal.
    fn click_while_pinned(&mut self) -> Vec<GameEvent> {
        match self.hovered {
            Some(cell) if Some(cell) == self.pinned => self.place(cell),
            Some(cell) if self.world.is_slot(cell) => self.pin(cell),
            _ => self.unpin(),
        }
    }

    fn place(&mut self, cell: Axial) -> Vec<GameEvent> {
        self.place_with(cell, self.selected.clone(), self.chosen)
    }

    /// The tile and orientation the ghost would show on the free slot `cell`: the current ones
    /// when the ghost is there, else [`GameState::snap_at`], else the current ones (refused there).
    fn snap_orientation(&self, cell: Axial) -> (String, Orientation) {
        if self.target_slot() == Some(cell) {
            return (self.selected.clone(), self.chosen);
        }
        self.snap_at(cell)
            .unwrap_or_else(|| (self.selected.clone(), self.chosen))
    }

    /// Shift+click on a free slot: places at once at the ghost's tile and orientation for it. The
    /// pin, if it is on another slot, stays.
    fn quick_place(&mut self, cell: Axial) -> Vec<GameEvent> {
        let (tile, o) = self.snap_orientation(cell);
        self.place_with(cell, tile, o)
    }

    /// Swaps the tile at `cell` for the selected tile (one undoable action), in the cell's own
    /// cliff-free-first order with the ghost's rotation kept where [`GameState::pick`] allows it.
    /// A free slot places as Shift+click does; the same tile in the same orientation and level
    /// changes nothing.
    fn replace(&mut self, cell: Axial) -> Vec<GameEvent> {
        if self.tool != Tool::Place || self.photo_mode {
            return self.refuse("replace", "replacing needs the place tool".into());
        }
        if self.world.is_slot(cell) {
            return self.quick_place(cell);
        }
        // The selected tile first, then the rest of its picker group, so a Beach Curve replace
        // takes whichever of headland and bay fits. The ghost sits on a free slot, never on this
        // cell, so the cell's own cliff-free-first order decides the level; the ghost's rotation
        // is kept where `pick` allows it.
        let members = self.catalog.siblings(&self.selected);
        let start = members
            .iter()
            .position(|d| d.id == self.selected)
            .unwrap_or(0);
        let n = members.len();
        let (tile, o) = (0..n)
            .map(|i| members[(start + i) % n])
            .find_map(|d| {
                let legal = self.world.replace_orientations(&self.catalog, cell, d);
                let ghost = (d.id == self.selected).then_some(self.chosen);
                self.pick(cell, d, &legal, ghost).map(|k| (d.id.clone(), k))
            })
            .unwrap_or_else(|| (self.selected.clone(), self.chosen));
        let (rotation, level) = (o.rotation, o.level);
        match self.world.replace(&self.catalog, cell, &tile, o) {
            Ok(old) => {
                let mut events = vec![GameEvent::TileReplaced {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
                    level,
                    replaced: old.def,
                }];
                events.extend(self.drop_stale_pin());
                self.last_bulldoze = None;
                self.resnap(false);
                self.world_rev += 1;
                self.result("replace", Some(cell), None);
                events
            }
            Err(ReplaceError::Unchanged) => {
                self.result(
                    "replace",
                    Some(cell),
                    Some(ReplaceError::Unchanged.to_string()),
                );
                vec![]
            }
            Err(ReplaceError::Empty) => self.refuse("replace", ReplaceError::Empty.to_string()),
            Err(e) => {
                let reason = e.to_string();
                self.result("replace", Some(cell), Some(reason.clone()));
                self.refusals += 1;
                vec![GameEvent::PlacementRefused {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
                    level,
                    reason,
                }]
            }
        }
    }

    fn place_with(&mut self, cell: Axial, tile: String, o: Orientation) -> Vec<GameEvent> {
        let (rotation, level) = (o.rotation, o.level);
        match self.world.place(&self.catalog, cell, &tile, o) {
            Ok(()) => {
                let mut events = vec![GameEvent::TilePlaced {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
                    level,
                }];
                events.extend(self.drop_stale_pin());
                self.last_bulldoze = None;
                self.resnap(false);
                self.world_rev += 1;
                self.result("place", Some(cell), None);
                events
            }
            Err(e) => {
                let reason = e.to_string();
                self.result("place", Some(cell), Some(reason.clone()));
                self.refusals += 1;
                vec![GameEvent::PlacementRefused {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
                    level,
                    reason,
                }]
            }
        }
    }

    fn bulldoze(&mut self, cell: Axial) -> Vec<GameEvent> {
        match self.world.bulldoze(&self.catalog, cell) {
            Ok(tile) => {
                let mut events = vec![GameEvent::TileBulldozed {
                    q: cell.q,
                    r: cell.r,
                    tile: tile.def,
                }];
                self.last_bulldoze = Some(cell);
                events.extend(self.drop_stale_pin());
                self.resnap(false);
                self.world_rev += 1;
                self.result("bulldoze", Some(cell), None);
                events
            }
            Err(e) => {
                let reason = e.to_string();
                self.result("bulldoze", Some(cell), Some(reason.clone()));
                self.refusals += 1;
                vec![GameEvent::BulldozeRefused {
                    q: cell.q,
                    r: cell.r,
                    reason,
                }]
            }
        }
    }

    /// Replaces the world with a loaded or a fresh one (PRD §12.2). The history travels with the
    /// world; the pointer, tool and refusal state reset, and agents re-derive from the new world.
    /// Photo mode is left as it is: `photo::sync` restores its own camera when it ends.
    pub fn replace_world(&mut self, world: World, event: GameEvent) {
        self.world = world;
        self.hovered = None;
        self.pinned = None;
        self.tool = Tool::Place;
        self.last = None;
        self.recent.clear();
        self.recent.push_back(event);
        self.chosen_orientation.clear();
        self.chosen_variant.clear();
        self.last_bulldoze = None;
        self.revision += 1;
        self.world_rev += 1;
        self.epoch += 1;
    }

    /// The `play` object of the state JSON, assembled whole for schema tests.
    #[cfg(test)]
    pub fn state_json(&self) -> Value {
        let mut state = self.state_fields_json();
        state
            .as_object_mut()
            .unwrap()
            .extend(self.world_state_json().as_object().unwrap().clone());
        state
    }

    /// World-sized fields change only with `world_rev`, not when the pointer or selection moves.
    pub fn world_state_json(&self) -> Value {
        let tiles: Vec<Value> = self
            .world
            .tiles()
            .iter()
            .map(|(c, t)| json!({ "q": c.q, "r": c.r, "tile": t.def, "rotation": t.rotation, "level": t.level }))
            .collect();
        json!({
            "tiles": tiles,
            "frontier": self.world.frontier(),
            "can_undo": self.world.can_undo(),
            "can_redo": self.world.can_redo(),
        })
    }

    /// Selection, mode and placement feedback, independent of world-sized tile data.
    pub fn state_fields_json(&self) -> Value {
        let hovered_kind = self.hovered.map(|c| {
            if self.world.tile(c).is_some() {
                "tile"
            } else if self.world.is_slot(c) {
                "slot"
            } else {
                "empty"
            }
        });
        let ghost = self.ghost().map(|(c, legal, edges, valid)| {
            json!({ "q": c.q, "r": c.r, "tile": self.selected, "rotation": self.chosen.rotation,
                    "level": self.chosen.level, "valid": valid, "legal_rotations": legal, "edges": edges })
        });
        let inventory: Vec<&str> = self
            .target_slot()
            .map(|c| {
                self.world
                    .inventory(&self.catalog, c)
                    .iter()
                    .map(|d| d.id.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let bulldoze = self.bulldoze_target().map(|(c, reason)| {
            json!({ "q": c.q, "r": c.r, "allowed": reason.is_none(), "reason": reason })
        });
        json!({
            "tool": self.tool,
            "selected": self.selected,
            "rotation": self.chosen.rotation,
            "inventory_open": self.inventory_open,
            "photo_mode": self.photo_mode,
            "walk_mode": self.walk_mode,
            "hovered": self.hovered,
            "hovered_kind": hovered_kind,
            "pinned": self.pinned,
            "ghost": ghost,
            "inventory": inventory,
            "bulldoze": bulldoze,
            // Per selected tile, so it lives here with the selection rather than in the world block.
            "slot_levels": self.slot_levels(&self.world.frontier()).iter()
                .map(|(c, l)| json!({ "q": c.q, "r": c.r, "level": l }))
                .collect::<Vec<_>>(),
            "last_result": self.last,
            "recent_events": self.recent,
        })
    }

    /// What `hex_play::play` draws.
    pub fn view(&self) -> PlayView {
        let frontier = if self.viewing() {
            vec![]
        } else {
            self.world.frontier()
        };
        let mut view = PlayView {
            slot_levels: self.slot_levels(&frontier),
            frontier,
            tiles: self
                .world
                .tiles()
                .iter()
                .map(|(c, t)| PlayTile {
                    cell: *c,
                    tile: t.def.clone(),
                    rotation: t.rotation,
                    level: t.level,
                })
                .collect(),
            ghost: self.ghost().map(|(cell, _, edges, valid)| PlayGhost {
                cell,
                tile: self.selected.clone(),
                rotation: self.chosen.rotation,
                level: self.chosen.level,
                edges,
                valid,
                pinned: self.pinned == Some(cell),
                tunnel: hex_core::tunnel::preview(
                    &self.world,
                    &self.catalog,
                    cell,
                    self.selected_def(),
                    self.chosen,
                )
                .cells,
            }),
            pointer_slot: self.hovered_slot().filter(|&c| {
                self.tool == Tool::Place && !self.viewing() && self.pinned.is_some_and(|p| p != c)
            }),
            pointer_tile: self.hovered.filter(|&c| {
                self.tool == Tool::Place && !self.viewing() && self.world.tile(c).is_some()
            }),
            bulldoze: self
                .bulldoze_target()
                .map(|(c, reason)| (c, reason.is_none())),
            bulldoze_cover: self.bulldoze_target().is_some_and(|(c, _)| {
                self.world.can_bulldoze(&self.catalog, c)
                    == Err(hex_core::BulldozeError::TunnelCover)
            }),
            bulldoze_mode: self.tool == Tool::Bulldoze && !self.viewing(),
            bulldozed: self.last_bulldoze,
            refusals: self.refusals,
            epoch: self.epoch,
            world_seed: self.world.seed(),
            ..PlayView::default()
        };
        view.index_tiles();
        view
    }
}

/// Events not yet drained by `hex_world_events`; nothing drains natively, so the oldest drop.
pub const EVENT_QUEUE_CAP: usize = 256;

pub fn publish_event(e: &GameEvent) {
    let json = serde_json::to_string(e).unwrap_or_default();
    info!("event: {json}");
    let mut q = bridge::EVENTS.lock().unwrap();
    q.push_back(json);
    while q.len() > EVENT_QUEUE_CAP {
        q.pop_front();
    }
}

/// Drains the action queue: camera actions move the camera, game actions go to [`GameState`]
/// (only in the play scenario, where the world is drawn).
pub fn apply_actions(
    mut queue: ResMut<ActionQueue>,
    mut game: ResMut<GameState>,
    active: Res<ActiveScenario>,
    mut walk: ResMut<crate::walk::WalkSession>,
    mut cameras: Query<&mut Transform, hex_render::post::GameCamera>,
) {
    for action in queue.0.drain(..) {
        if action.is_walk() {
            walk.intent(&action);
            continue;
        }
        if action.is_camera() {
            // In play the rig orbits and zooms about the raised tops it looks at (P4).
            let play = active.0 == Scenario::Play;
            for mut t in &mut cameras {
                *t = crate::camera::apply_over(*t, &action, |c| {
                    play.then(|| game.top_of(c)).flatten()
                });
            }
            continue;
        }
        if active.0 != Scenario::Play {
            info!("action {action:?} ignored outside the play scenario");
            continue;
        }
        for e in game.apply(action) {
            publish_event(&e);
        }
    }
}

/// Publishes the renderer's rise cues as `riseStage` events (after `hex_play::play::PlaySystems`).
pub fn publish_rise_events(mut cues: MessageReader<RiseMessage>) {
    for m in cues.read() {
        publish_event(&GameEvent::RiseStage {
            q: m.cell.q,
            r: m.cell.r,
            tile: m.tile.clone(),
            stage: m.cue.name(),
            skipped: m.skipped,
        });
    }
}

/// Rebuilds the render view when the game changed.
pub fn sync_view(game: Res<GameState>, mut view: ResMut<PlayView>, mut seen: Local<u64>) {
    if *seen == game.revision {
        return;
    }
    *seen = game.revision;
    let v = game.view();
    if *view != v {
        *view = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_core::BulldozeError;

    fn ax(q: i32, r: i32) -> Axial {
        Axial::new(q, r)
    }

    fn game() -> GameState {
        GameState::new(1)
    }

    /// The elevation visual-parity capture script (`docs/checkpoints/elevation/visual-parity/`,
    /// run by `make shots-piece` in a booked window) builds its scene with no refusal: a level-1
    /// meadow ringed by six cliff-free hillsides in flat meadow, and a two-tile plateau one level
    /// up beside flat meadow, its edges cliffs.
    #[test]
    fn the_visual_parity_script_builds_its_elevation_scene() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/elevation/visual-parity/visual-parity.json"
        );
        let script = crate::control::parse_script(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut g = game();
        for cmd in &script {
            if let Some(a) = cmd.action() {
                for e in g.apply(a) {
                    assert!(
                        !matches!(e, GameEvent::PlacementRefused { .. }),
                        "{cmd:?}: {e:?}"
                    );
                }
            }
        }
        let tile = |q, r| g.world.tile(ax(q, r)).cloned().expect("placed");
        let cliffs = |q, r| {
            let t = tile(q, r);
            let def = g.catalog.get(&t.def).unwrap();
            let o = Orientation::new(t.rotation, t.level);
            g.world
                .cliff_edges(&g.catalog, ax(q, r), def, o)
                .iter()
                .filter(|c| **c)
                .count()
        };
        let p = tile(3, 0);
        assert_eq!((p.def.as_str(), p.level), ("meadow", 1));
        assert_eq!(cliffs(3, 0), 0, "the hillside ring meets the plateau");
        for e in 0..6 {
            let c = ax(3, 0).neighbor(e);
            let t = tile(c.q, c.r);
            assert_eq!((t.def.as_str(), t.level), ("hillside_000011", 0), "{c:?}");
            assert_eq!(cliffs(c.q, c.r), 0, "{c:?}");
        }
        for (q, r) in [(-1, -1), (-2, 0)] {
            let t = tile(q, r);
            assert_eq!((t.def.as_str(), t.level), ("meadow", 1));
            assert!(cliffs(q, r) >= 1, "({q}, {r}) stands on a cliff");
        }
    }

    /// The elevation parity and shots world (`docs/checkpoints/elevation/elevation.json`) is legal
    /// by the Rust rules: no placement is refused, a three-tile plateau stands a level up ringed by
    /// hillsides, a two-portal tunnel runs under raised meadow, a road climbs a ramp, and the river
    /// and beach stay on the ground.
    #[test]
    fn the_elevation_world_script_is_legal() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/elevation/elevation.json"
        );
        let script = crate::control::parse_script(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut g = game();
        for cmd in &script {
            if let Some(a) = cmd.action() {
                for e in g.apply(a) {
                    assert!(
                        !matches!(e, GameEvent::PlacementRefused { .. }),
                        "{cmd:?}: {e:?}"
                    );
                }
            }
        }
        let at = |q, r| g.world.tile(ax(q, r)).cloned().expect("placed");
        for (q, r) in [(0, -2), (0, -3), (1, -3)] {
            let t = at(q, r);
            assert_eq!(
                (t.def.as_str(), t.level),
                ("meadow", 1),
                "plateau ({q}, {r})"
            );
        }
        assert_eq!(
            hex_core::tunnel::tunnels(&g.world, &g.catalog)
                .iter()
                .map(|t| (t.a, t.b, t.cover.clone(), t.level))
                .collect::<Vec<_>>(),
            [(ax(2, 2), ax(4, 2), vec![ax(3, 2)], 0)]
        );
        assert_eq!(
            at(3, 2).level,
            1,
            "the cover stands a level above the portals"
        );
        assert_eq!((at(6, 2).def.as_str(), at(6, 2).level), ("road_ramp", 0));
        assert_eq!(
            (at(7, 2).def.as_str(), at(7, 2).level),
            ("road_straight", 1)
        );
        for (q, r) in [(-1, 0), (-2, 0), (0, 1)] {
            assert_eq!(at(q, r).level, 0, "({q}, {r}) is on the ground");
        }
        // Nothing in the world stands on a cliff.
        for (c, t) in g.world.tiles() {
            let def = g.catalog.get(&t.def).unwrap();
            let o = Orientation::new(t.rotation, t.level);
            assert!(
                !g.world.cliff_edges(&g.catalog, *c, def, o).contains(&true),
                "{c:?} {}",
                t.def
            );
        }
        // The cover cannot be removed or swapped (hex-core TunnelCover).
        g.apply(Action::SelectTile("forest".into()));
        let refused = g.apply(Action::Replace(ax(3, 2)));
        assert!(
            matches!(&refused[..], [GameEvent::PlacementRefused { reason, .. }] if reason.contains("tunnel")),
            "{refused:?}"
        );
        g.apply(Action::SetTool(Tool::Bulldoze));
        let refused = g.apply(Action::Bulldoze(ax(3, 2)));
        assert!(
            matches!(&refused[..], [GameEvent::BulldozeRefused { reason, .. }] if reason.contains("tunnel")),
            "{refused:?}"
        );
    }

    /// Hovering a tunnel cover in bulldoze mode carries the TunnelCover refusal into the view
    /// (`bulldoze_cover`), the way the HUD's `cutOff` returns nothing for a cover; a portal does not.
    #[test]
    fn bulldoze_of_a_tunnel_cover_is_flagged_in_the_view() {
        let mut g = game();
        let o = Orientation::new;
        // A tunnel along r = 0 with a road-free meadow detour along r = 2 keeping the world
        // connected (hex-sim tests/common tunnel_layout), the cover placed last.
        let mut edits = vec![
            (ax(1, 0), "house_cluster", 0, 0),
            (ax(2, 0), "road_straight", 0, 0),
            (ax(3, 0), "road_tunnel_portal", 0, 0),
            (ax(0, 1), "meadow", 0, 0),
        ];
        edits.extend((0..=9).map(|q| (ax(q, 2), "meadow", 0, 0)));
        edits.extend([
            (ax(9, 1), "meadow", 0, 0),
            (ax(9, 0), "meadow", 0, 0),
            (ax(8, 0), "house_cluster", 3, 0),
            (ax(7, 0), "road_straight", 0, 0),
            (ax(6, 0), "road_tunnel_portal", 3, 0),
            (ax(4, 0), "meadow", 0, 1),
            (ax(5, 0), "meadow", 0, 1),
        ]);
        for (cell, id, rot, level) in edits {
            g.world.place(&g.catalog, cell, id, o(rot, level)).unwrap();
        }
        g.apply(Action::SetTool(Tool::Bulldoze));
        g.apply(Action::Hover(Some(ax(4, 0))));
        let v = g.view();
        assert_eq!(v.bulldoze, Some((ax(4, 0), false)));
        assert!(v.bulldoze_cover);
        g.apply(Action::Hover(Some(ax(3, 0))));
        assert!(
            !g.view().bulldoze_cover,
            "a portal is refused for another reason, if at all"
        );
    }

    /// A scripted `place` on a level keeps the ghost's rotation only when it is as cliff-free as the
    /// first legal one there; a cliff rotation never wins over a seamless one.
    #[test]
    fn scripted_place_on_a_level_never_prefers_a_cliff_rotation() {
        let mut g = game();
        let cell = ax(1, 0);
        g.apply(Action::SelectTile("hillside_000011".into()));
        let def = g.catalog.get("hillside_000011").unwrap().clone();
        let on_ground: Vec<Orientation> = g
            .world
            .legal_orientations(&g.catalog, cell, &def)
            .into_iter()
            .filter(|o| o.level == 0)
            .collect();
        let cliff = |o: Orientation| {
            g.world
                .cliff_edges(&g.catalog, cell, &def, o)
                .contains(&true)
        };
        let seamless = on_ground
            .iter()
            .copied()
            .find(|&o| !cliff(o))
            .expect("a seamless rotation");
        let steep = on_ground
            .iter()
            .copied()
            .find(|&o| cliff(o))
            .expect("a cliff rotation");
        g.apply(Action::Hover(Some(cell)));
        g.chosen = steep;
        g.chosen_orientation.insert(g.selected.clone(), steep);
        let e = g.apply(Action::PlaceAtLevel(cell, 0));
        assert!(
            e.iter().any(|e| matches!(e, GameEvent::TilePlaced { .. })),
            "{e:?}"
        );
        let t = g.world.tile(cell).unwrap();
        let got = Orientation::new(t.rotation, t.level);
        assert!(
            !g.world
                .cliff_edges(&g.catalog, cell, &def, got)
                .contains(&true),
            "took the cliff rotation {got:?} over {seamless:?}"
        );
    }

    /// A refused scripted `place` leaves the ghost where the hover put it, not on the refused level.
    #[test]
    fn a_refused_scripted_place_leaves_the_ghost_alone() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::PlaceAtLevel(ax(1, 0), 9));
        assert_ne!(g.chosen.level, 9);
        assert_eq!(g.chosen.level, 0);
        assert_ne!(g.chosen_orientation.get("meadow").map(|o| o.level), Some(9));
    }

    /// The script `place` command's optional `level` (P4): omitted, the first legal orientation;
    /// given, that level, at the ghost's rotation where it is legal there.
    #[test]
    fn scripted_place_takes_a_level_or_is_refused() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        let e = g.apply(Action::PlaceAtLevel(ax(1, 0), 2));
        assert!(
            e.contains(&GameEvent::TilePlaced {
                q: 1,
                r: 0,
                tile: "meadow".into(),
                rotation: 0,
                level: 2
            }),
            "{e:?}"
        );
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().level, 2);
        // Omitted: the ground, as the ghost lands.
        g.apply(Action::Place(ax(-1, 0)));
        assert_eq!(g.world.tile(ax(-1, 0)).unwrap().level, 0);
        // A level no orientation is legal on is a refusal with the world's own reason.
        g.apply(Action::SelectTile("river_straight".into()));
        let e = g.apply(Action::PlaceAtLevel(ax(0, 1), 1));
        assert!(
            matches!(e.last(), Some(GameEvent::PlacementRefused { level: 1, .. })),
            "{e:?}"
        );
        assert!(g.world.tile(ax(0, 1)).is_none());
        let e = g.apply(Action::PlaceAtLevel(ax(0, 1), 9));
        assert!(
            matches!(e.last(), Some(GameEvent::PlacementRefused { level: 9, .. })),
            "{e:?}"
        );
    }

    fn place(g: &mut GameState, id: &str, cell: Axial) -> Vec<GameEvent> {
        g.apply(Action::SelectTile(id.into()));
        g.apply(Action::Place(cell))
    }

    #[test]
    fn quick_place_fallback_preserves_the_primary_walk_guard() {
        let mut g = game();
        g.apply(Action::SetWalk(true));
        assert_eq!(
            g.apply(Action::QuickPlace),
            vec![GameEvent::ActionRefused {
                action: "primary",
                reason: "leave walk mode first".into(),
            }]
        );
    }

    #[test]
    fn hover_auto_rotates_to_the_first_legal_rotation() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        // Edge 3 of (1,0) faces the meadow, so neither road edge may point there.
        let (_, legal, edges, valid) = g.ghost().unwrap();
        assert_eq!(legal, [1, 2, 4, 5]);
        assert_eq!(g.chosen.rotation, 1);
        assert!(valid);
        assert_eq!(edges[3], EdgeStatus::Compatible);
        assert_eq!(edges[0], EdgeStatus::Neutral);
    }

    #[test]
    fn rotate_cycles_distinct_legal_orientations_both_ways() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        // Legal 1, 2, 4, 5, but a straight road turned 180° looks the same: two orientations on
        // the ground, then the same two one level up (a cliff to the meadow), and so on to 3.
        assert_eq!(g.chosen, Orientation::new(1, 0));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(2, 0));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(
            g.chosen,
            Orientation::new(1, 1),
            "4 repeats 1's sockets and is skipped; the ground ones are done"
        );
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(g.chosen, Orientation::new(2, 0));
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(g.chosen, Orientation::new(1, 0));
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(
            g.chosen,
            Orientation::new(2, 3),
            "back from the first wraps"
        );
    }

    #[test]
    fn rotate_on_a_fully_symmetric_tile_keeps_its_one_orientation() {
        let mut g = game();
        g.apply(Action::SelectTile("forest".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.chosen, Orientation::new(0, 0));
        for dir in [RotateDir::Forward, RotateDir::Back] {
            let e = g.apply(Action::Rotate(dir));
            assert_eq!(g.chosen, Orientation::new(0, 0));
            assert_eq!(
                e,
                [GameEvent::GhostRotated {
                    rotation: 0,
                    level: 0,
                    valid: true
                }]
            );
        }
        // A raisable meadow has one orientation per level: R steps it up, ground first.
        g.apply(Action::SelectTile("meadow".into()));
        assert_eq!(g.chosen, Orientation::new(0, 0));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(0, 1));
        g.apply(Action::Rotate(RotateDir::Back));
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(g.chosen, Orientation::new(0, 3));
    }

    #[test]
    fn the_chosen_rotation_survives_moving_off_and_back_while_legal() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(
            g.chosen,
            Orientation::new(1, 0),
            "auto-rotate: the first legal rotation, on the ground"
        );
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(2, 0));
        // At (1, -1) the meadow sits on world edge 2, so rotation 2 is illegal there.
        g.apply(Action::Hover(Some(ax(1, -1))));
        assert_eq!(
            g.chosen,
            Orientation::new(0, 0),
            "not legal here: the first legal rotation"
        );
        g.apply(Action::Hover(None));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(
            g.chosen.rotation, 2,
            "back on the slot: the chosen rotation, not a reset"
        );
        g.apply(Action::Hover(Some(ax(0, 1))));
        assert_eq!(g.chosen.rotation, 2, "legal at another slot too");
        // Per tile: another tile snaps as usual, and the road keeps its choice.
        g.apply(Action::SelectTile("road_bend_120".into()));
        let first = g.ghost().unwrap().1[0];
        assert_eq!(g.chosen.rotation, first);
        g.apply(Action::SelectTile("road_straight".into()));
        assert_eq!(g.chosen.rotation, 2);
    }

    /// A cliff chosen with R at one slot is never the default elsewhere, nor on coming back.
    #[test]
    fn a_chosen_cliff_level_does_not_follow_the_hover() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(0, 1));
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.chosen.level, 0, "a fresh ground slot snaps cliff-free");
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.chosen.level, 0, "back on the slot: no kept cliff");
        // The remembered rotation still applies where it is cliff-free.
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Rotate(RotateDir::Forward));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(1, 1));
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.chosen, Orientation::new(1, 0));
    }

    #[test]
    fn quick_place_away_from_the_ghost_places_cliff_free() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(0, 1));
        g.quick_place(ax(-1, 0));
        assert_eq!(g.world.tile(ax(-1, 0)).unwrap().level, 0);
    }

    #[test]
    fn replacing_a_plateau_tile_keeps_the_plateau_level() {
        let mut g = game();
        for q in 1..=3 {
            g.world
                .place(&g.catalog, ax(q, 0), "meadow", Orientation::new(0, 1))
                .unwrap();
        }
        // The ghost sits on a ground slot west of the start, on level 0.
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.chosen.level, 0);
        g.apply(Action::Replace(ax(2, 0)));
        let t = g.world.tile(ax(2, 0)).unwrap();
        assert_eq!((t.def.as_str(), t.level), ("road_straight", 1));
        assert_eq!(
            t.rotation, g.chosen.rotation,
            "the ghost's rotation, where it fits"
        );
    }

    #[test]
    fn a_raised_choice_drops_to_the_ground_for_a_tile_that_cannot_rise() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen, Orientation::new(0, 1));
        // The ocean fits nowhere beside the meadow, so nothing snaps; it still may not float.
        g.apply(Action::SelectTile("ocean".into()));
        assert_eq!(g.chosen.level, 0);
        assert_eq!(g.view().ghost.unwrap().level, 0);
        g.apply(Action::Primary);
        let e = g.apply(Action::Primary);
        assert!(
            matches!(&e[0], GameEvent::PlacementRefused { reason, .. } if reason.contains("do not fit")),
            "{e:?}"
        );
        // Back on the meadow at that slot: cliff-free again, not the remembered cliff.
        g.apply(Action::SelectTile("meadow".into()));
        assert_eq!(g.chosen, Orientation::new(0, 0));
    }

    #[test]
    fn no_legal_rotation_shows_an_invalid_ghost_and_refuses() {
        let mut g = game();
        g.apply(Action::SelectTile("ocean".into()));
        g.apply(Action::Hover(Some(ax(0, 1))));
        let (_, legal, edges, valid) = g.ghost().unwrap();
        assert!(legal.is_empty() && !valid);
        assert!(edges.contains(&EdgeStatus::Incompatible));
        let before = g.chosen.rotation;
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen.rotation, (before + 1) % 6, "turns one raw step");
        let e = g.apply(Action::Primary);
        assert_eq!(
            e,
            [GameEvent::PinChanged {
                cell: Some(ax(0, 1))
            }]
        );
        assert_eq!(
            g.chosen.rotation,
            (before + 1) % 6,
            "pinning keeps the turn"
        );
        let e = g.apply(Action::Primary);
        assert!(
            matches!(&e[0], GameEvent::PlacementRefused { reason, .. } if reason.contains("do not fit"))
        );
        assert_eq!(g.pinned, Some(ax(0, 1)), "a refusal keeps the pin");
        assert_eq!(g.world.tiles().len(), 1);
        let last = g.last.as_ref().unwrap();
        assert!(!last.ok && last.reason.is_some());
    }

    #[test]
    fn place_undo_redo_and_events() {
        let mut g = game();
        let e = place(&mut g, "forest", ax(-1, 0));
        assert!(e.contains(&GameEvent::TilePlaced {
            q: -1,
            r: 0,
            tile: "forest".into(),
            rotation: 0,
            level: 0
        }));
        assert!(g.world.can_undo() && !g.world.can_redo());
        assert_eq!(g.apply(Action::Undo), [GameEvent::Undone]);
        assert!(g.world.tile(ax(-1, 0)).is_none());
        assert_eq!(g.apply(Action::Redo), [GameEvent::Redone]);
        assert!(g.world.tile(ax(-1, 0)).is_some());
        g.apply(Action::Redo);
        assert_eq!(
            g.last.as_ref().unwrap().reason.as_deref(),
            Some("nothing to redo")
        );
    }

    #[test]
    fn only_a_bulldoze_marks_the_removed_cell_for_the_dust_cloud() {
        let mut g = game();
        place(&mut g, "forest", ax(-1, 0));
        g.apply(Action::Undo);
        assert!(g.world.tile(ax(-1, 0)).is_none());
        assert_eq!(
            g.view().bulldozed,
            None,
            "an undone placement sinks with the puff"
        );
        g.apply(Action::Redo);
        g.apply(Action::Bulldoze(ax(-1, 0)));
        assert!(g.world.tile(ax(-1, 0)).is_none());
        assert_eq!(g.view().bulldozed, Some(ax(-1, 0)));
        g.apply(Action::Undo);
        assert_eq!(g.view().bulldozed, None);
        g.apply(Action::Redo);
        assert_eq!(
            g.view().bulldozed,
            None,
            "a redone bulldoze steps the history"
        );
        // Bulldoze, place on the same cell, undo: the undo takes the tile down, not the bulldoze.
        place(&mut g, "forest", ax(-1, 0));
        g.apply(Action::Bulldoze(ax(-1, 0)));
        place(&mut g, "meadow", ax(-1, 0));
        g.apply(Action::Undo);
        assert!(g.world.tile(ax(-1, 0)).is_none());
        assert_eq!(g.view().bulldozed, None);
    }

    #[test]
    fn primary_follows_the_tool() {
        let mut g = game();
        assert!(matches!(
            g.apply(Action::Primary)[0],
            GameEvent::ActionRefused { .. }
        ));
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Primary);
        assert!(g.world.tile(ax(1, 0)).is_none(), "the first click pins");
        g.apply(Action::Primary);
        assert!(g.world.tile(ax(1, 0)).is_some());
        g.apply(Action::ToggleBulldoze);
        assert_eq!(g.tool, Tool::Bulldoze);
        assert!(g.ghost().is_none(), "no ghost in bulldoze mode");
        g.apply(Action::Primary);
        assert!(
            g.world.tile(ax(1, 0)).is_none(),
            "bulldozed the hovered tile"
        );
    }

    #[test]
    fn beach_curve_rotates_between_headland_and_bay() {
        let mut g = game();
        g.apply(Action::SelectTile("beach_curve".into()));
        assert_eq!(
            g.selected, "beach_outer",
            "the group selects its first member"
        );
        g.apply(Action::Hover(Some(ax(1, 0))));
        // East of the start meadow: one headland orientation, then three bay ones, then around.
        let mut seen = vec![(g.selected.clone(), g.chosen.rotation)];
        for _ in 0..4 {
            g.apply(Action::Rotate(RotateDir::Forward));
            assert!(g.ghost().unwrap().3, "every step is legal");
            seen.push((g.selected.clone(), g.chosen.rotation));
        }
        let ids: Vec<&str> = seen.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            ids,
            [
                "beach_outer",
                "beach_inner",
                "beach_inner",
                "beach_inner",
                "beach_outer"
            ]
        );
        assert_eq!(seen[0], seen[4], "the fifth press wraps");
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!((g.selected.as_str(), g.chosen.rotation), ("beach_inner", 5));
        // The bay stays chosen: hovering off and back, or picking the group again, restores it.
        g.apply(Action::Hover(None));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!((g.selected.as_str(), g.chosen.rotation), ("beach_inner", 5));
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::SelectTile("beach_curve".into()));
        assert_eq!((g.selected.as_str(), g.chosen.rotation), ("beach_inner", 5));
        // Placing places the variant the ghost shows.
        g.apply(Action::Place(ax(1, 0)));
        let t = g.world.tile(ax(1, 0)).unwrap();
        assert_eq!((t.def.as_str(), t.rotation), ("beach_inner", 5));
    }

    #[test]
    fn beach_curve_hover_switches_to_the_member_that_fits() {
        let mut g = game();
        place(&mut g, "meadow", ax(0, 1));
        place(&mut g, "meadow", ax(1, 1));
        g.apply(Action::SelectTile("beach_outer".into()));
        // (1, 0) touches three meadows: only the bay wraps three grass edges.
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.selected, "beach_inner");
        assert!(g.ghost().unwrap().3);
        // An explicit headland pick is still preferred where the headland fits.
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.selected, "beach_outer");
        // Shift+click on the three-meadow slot from elsewhere places the bay.
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.selected, "beach_outer");
        let ev = g.quick_place(ax(1, 0));
        assert!(matches!(ev[0], GameEvent::TilePlaced { .. }), "{ev:?}");
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().def, "beach_inner");
        // The number keys count picker entries: Beach Curve is one of them.
        let picker = g.catalog.picker();
        let n = picker.iter().position(|e| e.id == "beach_curve").unwrap();
        g.apply(Action::SelectNth(n));
        assert!(g.selected.starts_with("beach_"));
        assert_eq!(picker[n + 1].id, "river_mouths");
        // Picking the group on a bay-only slot shows the bay, but the fallback is not a choice: the
        // next slot where both fit shows the headland again.
        let mut g = game();
        place(&mut g, "meadow", ax(0, 1));
        place(&mut g, "meadow", ax(1, 1));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::SelectTile("beach_curve".into()));
        assert_eq!(g.selected, "beach_inner");
        g.apply(Action::Hover(Some(ax(-1, 0))));
        assert_eq!(g.selected, "beach_outer");
    }

    #[test]
    fn select_by_id_and_number() {
        let mut g = game();
        assert!(matches!(
            g.apply(Action::SelectTile("castle".into()))[0],
            GameEvent::ActionRefused { .. }
        ));
        assert_eq!(g.selected, "meadow");
        g.apply(Action::SelectNth(1));
        assert_eq!(g.selected, "forest");
        assert!(matches!(
            g.apply(Action::SelectNth(99))[0],
            GameEvent::ActionRefused { .. }
        ));
    }

    #[test]
    fn a_click_pins_a_slot_and_a_second_click_places() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        let e = g.apply(Action::Primary);
        assert_eq!(
            e,
            [GameEvent::PinChanged {
                cell: Some(ax(1, 0))
            }]
        );
        assert_eq!(g.world.tiles().len(), 1, "the first click only pins");
        // The pointer travels over the start tile and another slot: the ghost stays pinned.
        g.apply(Action::Hover(Some(ax(0, 0))));
        g.apply(Action::Hover(Some(ax(1, -1))));
        let (cell, ..) = g.ghost().unwrap();
        assert_eq!(cell, ax(1, 0));
        assert_eq!(g.state_json()["pinned"], json!({ "q": 1, "r": 0 }));
        assert_eq!(g.view().pointer_slot, Some(ax(1, -1)));
        assert!(g.view().ghost.unwrap().pinned);
        // R turns the pinned ghost, through the rotations legal at (1, 0).
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen.rotation, 2);
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(
            g.chosen.rotation, 2,
            "hovering the pinned slot keeps the turn"
        );
        let e = g.apply(Action::Primary);
        assert!(e.contains(&GameEvent::TilePlaced {
            q: 1,
            r: 0,
            tile: "road_straight".into(),
            rotation: 2,
            level: 0
        }));
        assert!(e.contains(&GameEvent::PinChanged { cell: None }));
        assert_eq!(g.pinned, None);
    }

    #[test]
    fn a_click_on_another_slot_repins_and_elsewhere_or_esc_unpins() {
        let mut g = game();
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Primary);
        g.apply(Action::Hover(Some(ax(-1, 0))));
        g.apply(Action::Primary);
        assert_eq!(g.pinned, Some(ax(-1, 0)), "another slot pins that one");
        assert_eq!(g.world.tiles().len(), 1);
        g.apply(Action::Hover(Some(ax(0, 0))));
        let refusals = g.refusals;
        let e = g.apply(Action::Primary);
        assert_eq!(e, [GameEvent::PinChanged { cell: None }]);
        assert_eq!(g.refusals, refusals, "unpinning is not a refusal");
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Primary);
        assert_eq!(g.pinned, Some(ax(1, 0)));
        assert_eq!(
            g.apply(Action::Escape),
            [GameEvent::PinChanged { cell: None }]
        );
        assert!(
            g.apply(Action::Escape).is_empty(),
            "Esc with nothing pinned outside photo mode does nothing"
        );
        g.apply(Action::Primary);
        g.apply(Action::ToggleBulldoze);
        assert_eq!(g.pinned, None, "bulldoze mode drops the pin");
    }

    #[test]
    fn direct_place_ignores_a_pin_elsewhere() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Pin(ax(1, -1)));
        assert_eq!(g.pinned, Some(ax(1, -1)));
        let e = g.apply(Action::Place(ax(1, 0)));
        assert!(
            e.iter()
                .any(|e| matches!(e, GameEvent::TilePlaced { q: 1, r: 0, .. })),
            "placed at (1, 0) with a rotation legal there: {e:?}"
        );
        assert_eq!(g.pinned, None);
        assert!(matches!(
            g.apply(Action::Pin(ax(0, 0)))[0],
            GameEvent::ActionRefused { action: "pin", .. }
        ));
    }

    #[test]
    fn an_undo_that_removes_the_slot_drops_the_pin() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        // (2, 0) is a slot only through the meadow at (1, 0).
        g.apply(Action::Pin(ax(2, 0)));
        assert_eq!(g.pinned, Some(ax(2, 0)));
        let e = g.apply(Action::Undo);
        assert_eq!(e, [GameEvent::Undone, GameEvent::PinChanged { cell: None }]);
        assert_eq!(
            g.ghost().map(|gh| gh.0),
            Some(ax(1, 0)),
            "the ghost is back under the pointer"
        );
    }

    #[test]
    fn shift_click_places_a_run_without_pinning() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        for q in 1..=3 {
            g.apply(Action::Hover(Some(ax(q, 0))));
            let e = g.apply(Action::QuickPlace);
            assert!(
                e.contains(&GameEvent::TilePlaced {
                    q,
                    r: 0,
                    tile: "meadow".into(),
                    rotation: 0,
                    level: 0
                }),
                "{e:?}"
            );
            assert!(!e.iter().any(|e| matches!(e, GameEvent::PinChanged { .. })));
            assert_eq!(g.pinned, None, "no pin step");
        }
        assert_eq!(g.world.tiles().len(), 4);
        // On a placed tile Shift+click is a plain click: refused, nothing placed or pinned.
        g.apply(Action::Hover(Some(ax(2, 0))));
        assert!(matches!(
            g.apply(Action::QuickPlace)[..],
            [GameEvent::PlacementRefused { .. }]
        ));
    }

    #[test]
    fn shift_click_uses_the_ghost_rotation_and_keeps_a_pin_elsewhere() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.chosen.rotation, 2, "the player turned the ghost");
        g.apply(Action::QuickPlace);
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().rotation, 2);
        // A pin on (-1, 0): Shift+click on (0, -1) places there and the pin stays.
        g.apply(Action::Pin(ax(-1, 0)));
        g.apply(Action::Hover(Some(ax(0, -1))));
        let e = g.apply(Action::QuickPlace);
        assert!(
            g.world.tile(ax(0, -1)).is_some(),
            "placed at (0, -1): {e:?}"
        );
        assert_eq!(g.pinned, Some(ax(-1, 0)), "the pin elsewhere stays");
        assert!(!e.iter().any(|e| matches!(e, GameEvent::PinChanged { .. })));
        // Shift+click on the pinned slot places there and clears the pin.
        g.apply(Action::Hover(Some(ax(-1, 0))));
        let e = g.apply(Action::QuickPlace);
        assert!(e.contains(&GameEvent::PinChanged { cell: None }), "{e:?}");
        assert!(g.world.tile(ax(-1, 0)).is_some());
    }

    #[test]
    fn shift_click_where_nothing_fits_is_refused() {
        let mut g = game();
        g.apply(Action::SelectTile("ocean".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        let refusals = g.refusals;
        let e = g.apply(Action::QuickPlace);
        assert!(matches!(e[..], [GameEvent::PlacementRefused { .. }]));
        assert_eq!(g.refusals, refusals + 1, "the refusal pulses");
        assert_eq!((g.world.tiles().len(), g.pinned), (1, None));
    }

    #[test]
    fn shift_click_in_bulldoze_mode_bulldozes() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        g.apply(Action::SetTool(Tool::Bulldoze));
        g.apply(Action::Hover(Some(ax(1, 0))));
        g.apply(Action::QuickPlace);
        assert!(g.world.tile(ax(1, 0)).is_none());
    }

    #[test]
    fn ctrl_click_replaces_one_undoable_step() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        g.apply(Action::SelectTile("forest".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        let rev = g.world_rev;
        let e = g.apply(Action::QuickReplace);
        assert_eq!(
            e,
            [GameEvent::TileReplaced {
                q: 1,
                r: 0,
                tile: "forest".into(),
                rotation: g.chosen.rotation,
                level: g.chosen.level,
                replaced: "meadow".into(),
            }]
        );
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().def, "forest");
        assert_eq!(g.world.tiles().len(), 2);
        assert!(g.world_rev > rev, "the autosave snapshot moves");
        assert_eq!(g.view().bulldozed, None, "a replace is not a bulldoze");
        assert_eq!(g.last.as_ref().unwrap().action, "replace");
        assert_eq!(g.apply(Action::Undo), [GameEvent::Undone]);
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().def, "meadow");
        assert_eq!(g.apply(Action::Redo), [GameEvent::Redone]);
        assert_eq!(g.world.tile(ax(1, 0)).unwrap().def, "forest");
        // The same tile in the same orientation: nothing happens, no event, no refusal pulse.
        let refusals = g.refusals;
        assert!(g.apply(Action::QuickReplace).is_empty());
        assert_eq!(g.refusals, refusals);
        assert!(!g.last.as_ref().unwrap().ok);
        assert!(!g.world.can_redo() && g.world.can_undo());
    }

    #[test]
    fn replace_prefers_the_ghost_rotation_else_the_first_that_fits() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(-1, 0))));
        g.apply(Action::Rotate(RotateDir::Forward));
        let ghost = g.chosen;
        // At (1, 0) the meadows sit on world edge 3 only, so the ghost's orientation from (-1, 0)
        // fits there when its road ends avoid edge 3.
        let fits = g.world.replace_orientations(
            &g.catalog,
            ax(1, 0),
            g.catalog.get("road_straight").unwrap(),
        );
        g.apply(Action::Replace(ax(1, 0)));
        let t = g.world.tile(ax(1, 0)).unwrap();
        let got = Orientation::new(t.rotation, t.level);
        if fits.contains(&ghost) {
            assert_eq!(got, ghost);
        } else {
            assert_eq!(got, fits[0]);
        }
        assert!(fits.contains(&got));
    }

    #[test]
    fn a_refused_replace_changes_nothing_and_pulses() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        g.apply(Action::SelectTile("ocean".into()));
        let before = g.world.clone();
        let refusals = g.refusals;
        let e = g.apply(Action::Replace(ax(1, 0)));
        assert!(
            matches!(&e[..], [GameEvent::PlacementRefused { tile, reason, .. }]
                if tile == "ocean" && reason.contains("do not fit")),
            "{e:?}"
        );
        assert_eq!(g.world, before);
        assert_eq!(g.refusals, refusals + 1);
        let last = g.last.as_ref().unwrap();
        assert_eq!((last.action, last.ok), ("replace", false));
        // Bulldoze mode and empty cells refuse without a pulse.
        g.apply(Action::SetTool(Tool::Bulldoze));
        assert!(matches!(
            g.apply(Action::Replace(ax(1, 0)))[..],
            [GameEvent::ActionRefused {
                action: "replace",
                ..
            }]
        ));
        g.apply(Action::SetTool(Tool::Place));
        assert!(matches!(
            g.apply(Action::Replace(ax(7, 7)))[..],
            [GameEvent::ActionRefused {
                action: "replace",
                ..
            }]
        ));
        assert_eq!(g.refusals, refusals + 1);
    }

    #[test]
    fn ctrl_click_on_a_free_slot_places_like_shift_click() {
        let mut g = game();
        g.apply(Action::SelectTile("forest".into()));
        g.apply(Action::Hover(Some(ax(0, 1))));
        let e = g.apply(Action::QuickReplace);
        assert!(matches!(e[..], [GameEvent::TilePlaced { q: 0, r: 1, .. }]));
        assert_eq!(g.pinned, None);
    }

    #[test]
    fn the_view_names_a_hovered_placed_tile_in_place_mode_only() {
        let mut g = game();
        place(&mut g, "meadow", ax(1, 0));
        g.apply(Action::Hover(Some(ax(1, 0))));
        let v = g.view();
        assert_eq!(v.pointer_tile, Some(ax(1, 0)));
        assert!(v.ghost.is_none() && v.pointer_slot.is_none());
        g.apply(Action::Hover(Some(ax(2, 0))));
        assert_eq!(
            g.view().pointer_tile,
            None,
            "a free slot is not a placed tile"
        );
        // A pin keeps its ghost while the rim follows the pointer over placed tiles.
        g.apply(Action::Pin(ax(2, 0)));
        g.apply(Action::Hover(Some(ax(0, 0))));
        let v = g.view();
        assert_eq!(v.pointer_tile, Some(ax(0, 0)));
        assert!(v.ghost.is_some_and(|gh| gh.pinned && gh.cell == ax(2, 0)));
        g.apply(Action::TogglePhoto);
        assert_eq!(g.view().pointer_tile, None, "photo mode");
        g.apply(Action::TogglePhoto);
        g.apply(Action::ToggleBulldoze);
        let v = g.view();
        assert_eq!(v.pointer_tile, None, "bulldoze mode");
        assert_eq!(v.bulldoze.map(|b| b.0), Some(ax(0, 0)));
    }

    /// The proof script's game sequence (docs/checkpoints/play/play.json), replayed headlessly.
    #[test]
    fn checkpoint_sequence() {
        let json = include_str!("../../../docs/checkpoints/play/play.json");
        let cmds = crate::control::parse_script(json).unwrap();
        let mut g = game();
        let mut events = vec![];
        for c in cmds {
            if let Some(a) = c.action() {
                events.extend(g.apply(a));
            }
        }
        let placed: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::TilePlaced { tile, .. } => Some(tile.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            placed,
            ["road_straight", "road_straight", "house_cluster", "forest"]
        );
        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::PlacementRefused { tile, .. } if tile == "ocean"
        )));
        assert!(events.contains(&GameEvent::Undone) && events.contains(&GameEvent::Redone));
        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::TileBulldozed { tile, .. } if tile == "house_cluster"
        )));
        let disconnect = BulldozeError::WouldDisconnect.to_string();
        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::BulldozeRefused { reason, .. } if *reason == disconnect
        )));
        assert_eq!(g.world.tiles().len(), 4, "meadow, two roads, forest");
    }

    /// Elevation (P4): a free slot's outline sits on the level the selected tile's default
    /// orientation takes there. Beside a lone level-1 meadow a meadow goes cliff-free on level 1;
    /// between it and the level-0 start every level makes a cliff, so the ground wins. A flat
    /// world lists no slot.
    /// The published `slot_levels` follow the selected tile: it rides the selection fields, so a
    /// selection change republishes it without a world change.
    #[test]
    fn published_slot_levels_follow_the_selection() {
        let mut g = game();
        g.world
            .place(&g.catalog, ax(1, 0), "meadow", Orientation::new(0, 1))
            .unwrap();
        let published = |g: &GameState| {
            g.state_fields_json()["slot_levels"]
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(published(&g), 3, "meadow goes up beside the plateau");
        g.apply(Action::SelectTile("river_straight".into()));
        assert_eq!(published(&g), g.view().slot_levels.len());
        assert_eq!(published(&g), 0, "a river stays on the ground");
    }

    #[test]
    fn free_slots_take_the_level_the_selected_tile_would() {
        let mut g = game();
        assert_eq!(g.selected, "meadow");
        assert!(g.view().slot_levels.is_empty(), "a flat world");
        g.world
            .place(&g.catalog, ax(1, 0), "meadow", Orientation::new(0, 1))
            .unwrap();
        let v = g.view();
        let raised: Vec<Axial> = v.slot_levels.keys().copied().collect();
        assert_eq!(raised, vec![ax(1, 1), ax(2, -1), ax(2, 0)], "{v:?}");
        assert!(v.slot_levels.values().all(|&l| l == 1));
        for c in &v.frontier {
            if !raised.contains(c) {
                assert_eq!(
                    g.world
                        .auto_orient(&g.catalog, *c, g.selected_def())
                        .unwrap()
                        .level,
                    0
                );
            }
        }
    }

    #[test]
    fn view_and_state_follow_the_world() {
        let mut g = game();
        g.apply(Action::Hover(Some(ax(1, 0))));
        let v = g.view();
        assert_eq!(v.tiles.len(), 1);
        assert_eq!(v.frontier.len(), 6);
        assert!(v.ghost.is_some());
        let s = g.state_json();
        assert_eq!(s["hovered_kind"], "slot");
        assert_eq!(s["ghost"]["legal_rotations"].as_array().unwrap().len(), 6);
        assert!(
            s["inventory"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t == "forest")
        );
        g.apply(Action::TogglePhoto);
        let v = g.view();
        assert!(v.ghost.is_none() && v.frontier.is_empty());
        assert_eq!(g.recent.len(), 2);
        let e = serde_json::to_value(&g.recent[1]).unwrap();
        assert_eq!(e, json!({ "type": "photoModeToggled", "on": true }));
    }
}
