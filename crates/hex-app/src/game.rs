//! Play-scenario game state (PRD §4.1–4.2, D20): the hex-core world plus the player's selection,
//! ghost rotation and tool. [`GameState::apply`] is the only way an action changes it.
use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use hex_core::{Axial, EdgeStatus, ReplaceError, RotateDir, TileCatalog, TileDef, World};
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
    },
    PlacementRefused {
        q: i32,
        r: i32,
        tile: String,
        rotation: u8,
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
    /// Ghost rotation 0..6.
    pub rotation: u8,
    /// The rotation the player last chose with R, per tile id. Hovering a slot or selecting the
    /// tile again restores it wherever it is legal, instead of snapping to the first legal one.
    pub chosen_rotation: BTreeMap<String, u8>,
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
            rotation: 0,
            chosen_rotation: BTreeMap::new(),
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

    fn legal_here(&self, cell: Axial) -> Vec<u8> {
        self.world
            .legal_rotations(&self.catalog, cell, self.selected_def())
    }

    /// The tile and rotation the ghost snaps to on the free slot `cell` (PRD §4.2): the chosen
    /// member of the selected tile's picker group (the selected tile itself when it has none) at
    /// its chosen rotation where that is legal, else at its first legal rotation, else the next
    /// member that fits, the same way. `None` when no member fits.
    fn snap_at(&self, cell: Axial) -> Option<(String, u8)> {
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
            let legal = self.world.legal_rotations(&self.catalog, cell, d);
            self.chosen_rotation
                .get(&d.id)
                .copied()
                .filter(|k| legal.contains(k))
                .or_else(|| legal.first().copied())
                .map(|k| (d.id.clone(), k))
        })
    }

    /// Auto-rotate (PRD §4.2): `snap` takes [`GameState::snap_at`]; otherwise the tile and rotation
    /// are kept while they are still legal. With nothing legal they are left as they are.
    fn resnap(&mut self, snap: bool) {
        let Some(cell) = self.target_slot() else {
            return;
        };
        if (snap || !self.legal_here(cell).contains(&self.rotation))
            && let Some((tile, k)) = self.snap_at(cell)
        {
            self.selected = tile;
            self.rotation = k;
        }
    }

    /// Photo or walk mode: the world is only looked at, so no ghost, slot ring or bulldoze target.
    fn viewing(&self) -> bool {
        self.photo_mode || self.walk_mode
    }

    /// The ghost at the target slot; none in bulldoze mode, photo mode or walk mode.
    pub fn ghost(&self) -> Option<(Axial, Vec<u8>, [EdgeStatus; 6], bool)> {
        if self.tool != Tool::Place || self.viewing() {
            return None;
        }
        let cell = self.target_slot()?;
        let def = self.selected_def();
        let edges = self
            .world
            .edge_status(&self.catalog, cell, def, self.rotation);
        let valid = self.world.fits(&self.catalog, cell, def, self.rotation);
        Some((cell, self.legal_here(cell), edges, valid))
    }

    /// The hovered tile in bulldoze mode, with the refusal reason when removing it is blocked.
    pub fn bulldoze_target(&self) -> Option<(Axial, Option<String>)> {
        if self.tool != Tool::Bulldoze || self.viewing() {
            return None;
        }
        let cell = self.hovered.filter(|c| self.world.tile(*c).is_some())?;
        Some((
            cell,
            self.world.can_bulldoze(cell).err().map(|e| e.to_string()),
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
                    (&self.selected, self.rotation),
                    dir,
                )
                .map(|(d, k)| (d.id.clone(), k))
        });
        // No legal rotation: still turn one step, so the edge markers show why.
        match cycled {
            Some((tile, k)) => {
                self.selected = tile;
                self.rotation = k;
            }
            None => {
                self.rotation = match dir {
                    RotateDir::Forward => (self.rotation + 1) % 6,
                    RotateDir::Back => (self.rotation + 5) % 6,
                }
            }
        }
        if let Some(g) = self.catalog.group_of(&self.selected) {
            self.chosen_variant
                .insert(g.to_owned(), self.selected.clone());
        }
        self.chosen_rotation
            .insert(self.selected.clone(), self.rotation);
        let valid = self.ghost().is_some_and(|g| g.3);
        self.result("rotate", self.target_slot().or(self.hovered), None);
        vec![GameEvent::GhostRotated {
            rotation: self.rotation,
            valid,
        }]
    }

    fn place_at(&mut self, cell: Axial) -> Vec<GameEvent> {
        let mut events = self.set_pin(None);
        events.extend(self.hover(Some(cell)));
        events.extend(self.place(cell));
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
        self.place_with(cell, self.selected.clone(), self.rotation)
    }

    /// The tile and rotation the ghost would show on the free slot `cell`: the current ones when
    /// the ghost is there, else [`GameState::snap_at`], else the current ones (refused there).
    fn snap_rotation(&self, cell: Axial) -> (String, u8) {
        if self.target_slot() == Some(cell) {
            return (self.selected.clone(), self.rotation);
        }
        self.snap_at(cell)
            .unwrap_or_else(|| (self.selected.clone(), self.rotation))
    }

    /// Shift+click on a free slot: places at once at the ghost's tile and rotation for it. The
    /// pin, if it is on another slot, stays.
    fn quick_place(&mut self, cell: Axial) -> Vec<GameEvent> {
        let (tile, rotation) = self.snap_rotation(cell);
        self.place_with(cell, tile, rotation)
    }

    /// Swaps the tile at `cell` for the selected tile (one undoable action). The ghost's rotation
    /// is preferred where it fits, else the first rotation that fits. A free slot places as
    /// Shift+click does; the same tile in the same orientation changes nothing.
    fn replace(&mut self, cell: Axial) -> Vec<GameEvent> {
        if self.tool != Tool::Place || self.photo_mode {
            return self.refuse("replace", "replacing needs the place tool".into());
        }
        if self.world.is_slot(cell) {
            return self.quick_place(cell);
        }
        // The selected tile first (at the ghost's rotation where it fits), then the rest of its
        // picker group, so a Beach Curve replace takes whichever of headland and bay fits.
        let members = self.catalog.siblings(&self.selected);
        let start = members
            .iter()
            .position(|d| d.id == self.selected)
            .unwrap_or(0);
        let n = members.len();
        let (tile, rotation) = (0..n)
            .map(|i| members[(start + i) % n])
            .find_map(|d| {
                let legal = self.world.replace_rotations(&self.catalog, cell, d);
                let k = if d.id == self.selected && legal.contains(&self.rotation) {
                    Some(self.rotation)
                } else {
                    legal.first().copied()
                };
                k.map(|k| (d.id.clone(), k))
            })
            .unwrap_or_else(|| (self.selected.clone(), self.rotation));
        match self.world.replace(&self.catalog, cell, &tile, rotation) {
            Ok(old) => {
                let mut events = vec![GameEvent::TileReplaced {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
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
                    reason,
                }]
            }
        }
    }

    fn place_with(&mut self, cell: Axial, tile: String, rotation: u8) -> Vec<GameEvent> {
        match self.world.place(&self.catalog, cell, &tile, rotation) {
            Ok(()) => {
                let mut events = vec![GameEvent::TilePlaced {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
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
                    reason,
                }]
            }
        }
    }

    fn bulldoze(&mut self, cell: Axial) -> Vec<GameEvent> {
        match self.world.bulldoze(cell) {
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
        self.chosen_rotation.clear();
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
            .map(|(c, t)| json!({ "q": c.q, "r": c.r, "tile": t.def, "rotation": t.rotation }))
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
            json!({ "q": c.q, "r": c.r, "tile": self.selected, "rotation": self.rotation,
                    "valid": valid, "legal_rotations": legal, "edges": edges })
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
            "rotation": self.rotation,
            "inventory_open": self.inventory_open,
            "photo_mode": self.photo_mode,
            "walk_mode": self.walk_mode,
            "hovered": self.hovered,
            "hovered_kind": hovered_kind,
            "pinned": self.pinned,
            "ghost": ghost,
            "inventory": inventory,
            "bulldoze": bulldoze,
            "last_result": self.last,
            "recent_events": self.recent,
        })
    }

    /// What `hex_play::play` draws.
    pub fn view(&self) -> PlayView {
        PlayView {
            tiles: self
                .world
                .tiles()
                .iter()
                .map(|(c, t)| PlayTile {
                    cell: *c,
                    tile: t.def.clone(),
                    rotation: t.rotation,
                })
                .collect(),
            frontier: if self.viewing() {
                vec![]
            } else {
                self.world.frontier()
            },
            ghost: self.ghost().map(|(cell, _, edges, valid)| PlayGhost {
                cell,
                tile: self.selected.clone(),
                rotation: self.rotation,
                edges,
                valid,
                pinned: self.pinned == Some(cell),
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
            bulldoze_mode: self.tool == Tool::Bulldoze && !self.viewing(),
            bulldozed: self.last_bulldoze,
            refusals: self.refusals,
            epoch: self.epoch,
            world_seed: self.world.seed(),
        }
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
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    for action in queue.0.drain(..) {
        if action.is_walk() {
            walk.intent(&action);
            continue;
        }
        if action.is_camera() {
            for mut t in &mut cameras {
                *t = crate::camera::apply(*t, &action);
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
        assert_eq!(g.rotation, 1);
        assert!(valid);
        assert_eq!(edges[3], EdgeStatus::Compatible);
        assert_eq!(edges[0], EdgeStatus::Neutral);
    }

    #[test]
    fn rotate_cycles_distinct_legal_orientations_both_ways() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        // Legal 1, 2, 4, 5, but a straight road turned 180° looks the same: two orientations.
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.rotation, 2);
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.rotation, 1, "4 repeats 1's sockets and is skipped");
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(g.rotation, 2);
        g.apply(Action::Rotate(RotateDir::Back));
        assert_eq!(g.rotation, 1);
    }

    #[test]
    fn rotate_on_a_fully_symmetric_tile_keeps_its_one_orientation() {
        let mut g = game();
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.rotation, 0);
        for dir in [RotateDir::Forward, RotateDir::Back] {
            let e = g.apply(Action::Rotate(dir));
            assert_eq!(g.rotation, 0);
            assert_eq!(
                e,
                [GameEvent::GhostRotated {
                    rotation: 0,
                    valid: true
                }]
            );
        }
    }

    #[test]
    fn the_chosen_rotation_survives_moving_off_and_back_while_legal() {
        let mut g = game();
        g.apply(Action::SelectTile("road_straight".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.rotation, 1, "auto-rotate: the first legal rotation");
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.rotation, 2);
        // At (1, -1) the meadow sits on world edge 2, so rotation 2 is illegal there.
        g.apply(Action::Hover(Some(ax(1, -1))));
        assert_eq!(g.rotation, 0, "not legal here: the first legal rotation");
        g.apply(Action::Hover(None));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(
            g.rotation, 2,
            "back on the slot: the chosen rotation, not a reset"
        );
        g.apply(Action::Hover(Some(ax(0, 1))));
        assert_eq!(g.rotation, 2, "legal at another slot too");
        // Per tile: another tile snaps as usual, and the road keeps its choice.
        g.apply(Action::SelectTile("road_bend_120".into()));
        let first = g.ghost().unwrap().1[0];
        assert_eq!(g.rotation, first);
        g.apply(Action::SelectTile("road_straight".into()));
        assert_eq!(g.rotation, 2);
    }

    #[test]
    fn no_legal_rotation_shows_an_invalid_ghost_and_refuses() {
        let mut g = game();
        g.apply(Action::SelectTile("ocean".into()));
        g.apply(Action::Hover(Some(ax(0, 1))));
        let (_, legal, edges, valid) = g.ghost().unwrap();
        assert!(legal.is_empty() && !valid);
        assert!(edges.contains(&EdgeStatus::Incompatible));
        let before = g.rotation;
        g.apply(Action::Rotate(RotateDir::Forward));
        assert_eq!(g.rotation, (before + 1) % 6, "turns one raw step");
        let e = g.apply(Action::Primary);
        assert_eq!(
            e,
            [GameEvent::PinChanged {
                cell: Some(ax(0, 1))
            }]
        );
        assert_eq!(g.rotation, (before + 1) % 6, "pinning keeps the turn");
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
            rotation: 0
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
        let mut seen = vec![(g.selected.clone(), g.rotation)];
        for _ in 0..4 {
            g.apply(Action::Rotate(RotateDir::Forward));
            assert!(g.ghost().unwrap().3, "every step is legal");
            seen.push((g.selected.clone(), g.rotation));
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
        assert_eq!((g.selected.as_str(), g.rotation), ("beach_inner", 5));
        // The bay stays chosen: hovering off and back, or picking the group again, restores it.
        g.apply(Action::Hover(None));
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!((g.selected.as_str(), g.rotation), ("beach_inner", 5));
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::SelectTile("beach_curve".into()));
        assert_eq!((g.selected.as_str(), g.rotation), ("beach_inner", 5));
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
        assert_eq!(g.rotation, 2);
        g.apply(Action::Hover(Some(ax(1, 0))));
        assert_eq!(g.rotation, 2, "hovering the pinned slot keeps the turn");
        let e = g.apply(Action::Primary);
        assert!(e.contains(&GameEvent::TilePlaced {
            q: 1,
            r: 0,
            tile: "road_straight".into(),
            rotation: 2
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
                    rotation: 0
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
        assert_eq!(g.rotation, 2, "the player turned the ghost");
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
                rotation: g.rotation,
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
        let ghost = g.rotation;
        // At (1, 0) the meadows sit on world edge 3 only, so the ghost's rotation from (-1, 0)
        // fits there when its road ends avoid edge 3.
        let fits = g.world.replace_rotations(
            &g.catalog,
            ax(1, 0),
            g.catalog.get("road_straight").unwrap(),
        );
        g.apply(Action::Replace(ax(1, 0)));
        let got = g.world.tile(ax(1, 0)).unwrap().rotation;
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
