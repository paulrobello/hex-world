//! Play-scenario game state (PRD §4.1–4.2, D20): the hex-core world plus the player's selection,
//! ghost rotation and tool. [`GameState::apply`] is the only way an action changes it.
use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use hex_core::{Axial, EdgeStatus, RotateDir, TileCatalog, TileDef, World};
use hex_render::{
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
    ToolChanged {
        mode: Tool,
    },
    InventoryToggled {
        open: bool,
    },
    PhotoModeToggled {
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
    pub tool: Tool,
    pub hovered: Option<Axial>,
    pub inventory_open: bool,
    pub photo_mode: bool,
    pub last: Option<ActionResult>,
    pub recent: VecDeque<GameEvent>,
    /// Bumped by every applied game action; `sync_view` rebuilds the render view when it moves.
    pub revision: u64,
    /// Refused placements and bulldozes, so the renderer pulses the refused highlight each time.
    pub refusals: u32,
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
            tool: Tool::Place,
            hovered: None,
            inventory_open: false,
            photo_mode: false,
            last: None,
            recent: VecDeque::new(),
            revision: 1,
            refusals: 0,
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

    fn legal_here(&self, cell: Axial) -> Vec<u8> {
        self.world
            .legal_rotations(&self.catalog, cell, self.selected_def())
    }

    /// Auto-rotate (PRD §4.2): `snap` takes the selected tile's chosen rotation when it is legal
    /// here, else the first legal rotation; otherwise the rotation is kept while it is still legal.
    /// With no legal rotation the rotation is left as it is.
    fn resnap(&mut self, snap: bool) {
        let Some(cell) = self.hovered_slot() else {
            return;
        };
        let legal = self.legal_here(cell);
        let chosen = self
            .chosen_rotation
            .get(&self.selected)
            .copied()
            .filter(|k| legal.contains(k));
        if (snap || !legal.contains(&self.rotation))
            && let Some(k) = chosen.or_else(|| legal.first().copied())
        {
            self.rotation = k;
        }
    }

    /// The ghost at the hovered slot; none in bulldoze mode or photo mode.
    pub fn ghost(&self) -> Option<(Axial, Vec<u8>, [EdgeStatus; 6], bool)> {
        if self.tool != Tool::Place || self.photo_mode {
            return None;
        }
        let cell = self.hovered_slot()?;
        let def = self.selected_def();
        let edges = self
            .world
            .edge_status(&self.catalog, cell, def, self.rotation);
        let valid = self.world.fits(&self.catalog, cell, def, self.rotation);
        Some((cell, self.legal_here(cell), edges, valid))
    }

    /// The hovered tile in bulldoze mode, with the refusal reason when removing it is blocked.
    pub fn bulldoze_target(&self) -> Option<(Axial, Option<String>)> {
        if self.tool != Tool::Bulldoze || self.photo_mode {
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
        match action {
            Action::Orbit(_)
            | Action::Pan(_)
            | Action::Zoom(_)
            | Action::Fly(_)
            | Action::Look(_) => vec![],
            Action::Hover(cell) => {
                if cell == self.hovered {
                    return vec![];
                }
                self.hovered = cell;
                self.resnap(true);
                vec![GameEvent::HoverChanged { cell }]
            }
            Action::SelectTile(id) => {
                if self.catalog.get(&id).is_none() {
                    return self.refuse("select_tile", format!("unknown tile {id:?}"));
                }
                self.selected = id.clone();
                self.resnap(true);
                self.result("select_tile", None, None);
                vec![GameEvent::TileSelected { tile: id }]
            }
            Action::SelectNth(n) => match self.catalog.tiles().get(n) {
                Some(def) => {
                    let id = def.id.clone();
                    self.apply_inner(Action::SelectTile(id))
                }
                None => self.refuse("select_tile", format!("no inventory tile {}", n + 1)),
            },
            Action::Rotate(dir) => {
                let cycled = self.hovered_slot().and_then(|cell| {
                    self.world.cycle_rotation(
                        &self.catalog,
                        cell,
                        self.selected_def(),
                        self.rotation,
                        dir,
                    )
                });
                // No legal rotation: still turn one step, so the edge markers show why.
                self.rotation = cycled.unwrap_or(match dir {
                    RotateDir::Forward => (self.rotation + 1) % 6,
                    RotateDir::Back => (self.rotation + 5) % 6,
                });
                self.chosen_rotation
                    .insert(self.selected.clone(), self.rotation);
                let valid = self.ghost().is_some_and(|g| g.3);
                self.result("rotate", self.hovered, None);
                vec![GameEvent::GhostRotated {
                    rotation: self.rotation,
                    valid,
                }]
            }
            Action::Primary => match (self.tool, self.hovered) {
                (_, None) => self.refuse("primary", "no cell hovered".into()),
                (Tool::Place, Some(cell)) => self.place(cell),
                (Tool::Bulldoze, Some(cell)) => self.bulldoze(cell),
            },
            Action::Place(cell) => {
                let mut events = self.apply_inner(Action::Hover(Some(cell)));
                events.extend(self.place(cell));
                events
            }
            Action::Bulldoze(cell) => self.bulldoze(cell),
            Action::Undo => {
                if !self.world.undo() {
                    return self.refuse("undo", "nothing to undo".into());
                }
                self.resnap(false);
                self.result("undo", None, None);
                vec![GameEvent::Undone]
            }
            Action::Redo => {
                if !self.world.redo() {
                    return self.refuse("redo", "nothing to redo".into());
                }
                self.resnap(false);
                self.result("redo", None, None);
                vec![GameEvent::Redone]
            }
            Action::ToggleBulldoze => {
                let mode = match self.tool {
                    Tool::Place => Tool::Bulldoze,
                    Tool::Bulldoze => Tool::Place,
                };
                self.apply_inner(Action::SetTool(mode))
            }
            Action::SetTool(mode) => {
                self.tool = mode;
                self.resnap(false);
                self.result("set_tool", None, None);
                vec![GameEvent::ToolChanged { mode }]
            }
            Action::ToggleInventory => {
                self.inventory_open = !self.inventory_open;
                self.result("toggle_inventory", None, None);
                vec![GameEvent::InventoryToggled {
                    open: self.inventory_open,
                }]
            }
            Action::ExitPhoto if !self.photo_mode => vec![],
            Action::TogglePhoto | Action::ExitPhoto => {
                self.photo_mode = !self.photo_mode;
                self.result("toggle_photo", None, None);
                vec![GameEvent::PhotoModeToggled {
                    on: self.photo_mode,
                }]
            }
        }
    }

    fn place(&mut self, cell: Axial) -> Vec<GameEvent> {
        let (tile, rotation) = (self.selected.clone(), self.rotation);
        match self.world.place(&self.catalog, cell, &tile, rotation) {
            Ok(()) => {
                self.resnap(false);
                self.result("place", Some(cell), None);
                vec![GameEvent::TilePlaced {
                    q: cell.q,
                    r: cell.r,
                    tile,
                    rotation,
                }]
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
                self.resnap(false);
                self.result("bulldoze", Some(cell), None);
                vec![GameEvent::TileBulldozed {
                    q: cell.q,
                    r: cell.r,
                    tile: tile.def,
                }]
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

    /// The `play` object of the state JSON.
    pub fn state_json(&self) -> Value {
        let tiles: Vec<Value> = self
            .world
            .tiles()
            .iter()
            .map(|(c, t)| json!({ "q": c.q, "r": c.r, "tile": t.def, "rotation": t.rotation }))
            .collect();
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
            .hovered_slot()
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
            "tiles": tiles,
            "frontier": self.world.frontier(),
            "hovered": self.hovered,
            "hovered_kind": hovered_kind,
            "ghost": ghost,
            "inventory": inventory,
            "bulldoze": bulldoze,
            "can_undo": self.world.can_undo(),
            "can_redo": self.world.can_redo(),
            "last_result": self.last,
            "recent_events": self.recent,
        })
    }

    /// What `hex_render::play` draws.
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
            frontier: if self.photo_mode {
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
            }),
            bulldoze: self
                .bulldoze_target()
                .map(|(c, reason)| (c, reason.is_none())),
            bulldoze_mode: self.tool == Tool::Bulldoze && !self.photo_mode,
            refusals: self.refusals,
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
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    for action in queue.0.drain(..) {
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

/// Publishes the renderer's rise cues as `riseStage` events (after `hex_render::play::PlaySystems`).
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
        assert!(
            matches!(&e[0], GameEvent::PlacementRefused { reason, .. } if reason.contains("do not fit"))
        );
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
    fn primary_follows_the_tool() {
        let mut g = game();
        assert!(matches!(
            g.apply(Action::Primary)[0],
            GameEvent::ActionRefused { .. }
        ));
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Hover(Some(ax(1, 0))));
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
