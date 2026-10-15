// Typed view of window.hexWorld (index.html) and the Rust command, event and state shapes
// (crates/hex-app/src/{control,game}.rs). The *_FIELDS tables are checked against
// bridge-schema.json, which a Rust test regenerates from the real enums (make ui-schema).

export type SocketKind = "Grass" | "Forest" | "Road" | "River" | "Beach" | "Ocean";
export type Tool = "place" | "bulldoze";
export type Spin = "cw" | "ccw";
export type ToggleTarget = "bulldoze" | "inventory" | "photo";
export type EdgeStatus = "Compatible" | "Incompatible" | "Neutral";
export type HoveredKind = "tile" | "slot" | "empty";
/** Photo mode's depth of field: off, the tilt-shift kernel, or the lens (manual). */
export type Dof = "off" | "tilt" | "manual";
/** Quality tiers (PRD §9.2). */
export type TierName = "low" | "medium" | "high" | "ultra";
/** A quality choice: a tier, or `auto` (the detected tier). The save stores it. */
export type QualityChoice = "auto" | TierName;
/** The named looks (tone mapper, exposure and grading), in the engine's display order (`hex_render::Grade::ALL`). */
export const GRADES = ["aces", "tony", "rich-1", "rich-2", "rich-3"] as const;
export type GradeName = (typeof GRADES)[number];
/** Where the tier in force came from (`docs/agent-operability.md`, "Quality tier"). */
export type TierSource = "default" | "explicit" | "detected" | "probe" | "runtime";

export interface Cell {
  q: number;
  r: number;
}

export type Vec3 = [number, number, number];

/** Audio bus gains as a save carries them (the audio layer's `effects` and `interface` are `sfx` and `ui`). */
export interface Volumes {
  master: number;
  music: number;
  sfx: number;
  ambience: number;
  ui: number;
}

/**
 * The settings a `loaded` event reports. The engine has already applied `quality` (a `tier` key
 * still wins) and the tilt-shift pair; `volumes` stay unapplied, since audio levels are per viewer.
 */
export interface SavedSettings {
  quality: QualityChoice;
  tilt_shift: boolean;
  tilt_shift_strength: number;
  volumes: Volumes;
}

/** Why `.hexworld` bytes were refused: the check that failed, in words, and the file's version for version refusals. */
export type LoadRefusalCode =
  | "too_large"
  | "bad_magic"
  | "newer_version"
  | "unsupported_version"
  | "truncated"
  | "trailing_bytes"
  | "checksum"
  | "decode"
  | "invalid_value"
  | "invalid_world"
  | "invalid_history"
  | "io"
  | "no_file";

/** `hexWorld.load(bytes)`: answered at once; on ok the load is queued behind earlier commands. */
export type LoadResult =
  | { ok: true; tiles: number; version: number }
  | { ok: false; code: LoadRefusalCode; reason: string; version: number | null };

export type Command =
  | { cmd: "camera"; pos: [number, number, number]; look_at: [number, number, number] }
  | { cmd: "screenshot"; name: string }
  | { cmd: "wait"; frames: number }
  | { cmd: "scenario"; name: string }
  | { cmd: "tilt"; on: boolean | null; strength: number | null }
  | { cmd: "dump_state"; name: string }
  | { cmd: "set_tier"; tier: QualityChoice }
  | { cmd: "set_grade"; grade: GradeName }
  | { cmd: "set_reduced_motion"; on: boolean }
  | { cmd: "exit" }
  | { cmd: "select_tile"; id: string }
  | { cmd: "hover"; q: number; r: number }
  | { cmd: "pin"; q: number; r: number }
  | { cmd: "unpin" }
  | { cmd: "rotate"; dir: Spin }
  | { cmd: "place"; q: number; r: number }
  | { cmd: "replace"; q: number; r: number }
  | { cmd: "undo" }
  | { cmd: "redo" }
  | { cmd: "bulldoze"; q: number; r: number }
  | { cmd: "set_tool"; mode: Tool }
  | { cmd: "toggle"; what: ToggleTarget }
  | {
      cmd: "photo";
      dof?: Dof | null;
      focus?: number | null;
      aperture?: number | null;
      exposure?: number | null;
      fov?: number | null;
      roll?: number | null;
      pause_agents?: boolean | null;
    }
  | { cmd: "photo_export"; scale: 1 | 2 | 4; name: string }
  | { cmd: "save"; tag: string | null; path: string | null; volumes: Volumes | null }
  | { cmd: "load"; path: string | null }
  | { cmd: "new_world" }
  | { cmd: "walk"; on: boolean | null; pointer_lock: boolean | null }
  | { cmd: "walk_to"; x: number; z: number; heading: number | null; pitch: number | null }
  | { cmd: "walk_step"; forward: number; right: number; seconds: number; jog: boolean | null }
  | { cmd: "walk_look"; heading: number | null; pitch: number | null };

export type GameEvent =
  | { type: "tilePlaced"; q: number; r: number; tile: string; rotation: number }
  | { type: "placementRefused"; q: number; r: number; tile: string; rotation: number; reason: string }
  | { type: "tileBulldozed"; q: number; r: number; tile: string }
  | { type: "bulldozeRefused"; q: number; r: number; reason: string }
  | { type: "tileReplaced"; q: number; r: number; tile: string; rotation: number; replaced: string }
  | { type: "undone" }
  | { type: "redone" }
  | { type: "tileSelected"; tile: string }
  | { type: "ghostRotated"; rotation: number; valid: boolean }
  | { type: "hoverChanged"; cell: Cell | null }
  | { type: "pinChanged"; cell: Cell | null }
  | { type: "toolChanged"; mode: Tool }
  | { type: "inventoryToggled"; open: boolean }
  | { type: "photoModeToggled"; on: boolean }
  | { type: "walkModeToggled"; on: boolean }
  | { type: "actionRefused"; action: string; reason: string }
  | { type: "riseStage"; q: number; r: number; tile: string; stage: string; skipped: boolean }
  /** A car reverses out of its spot (trip start): its sim id, the spot's cell and world XZ in metres. */
  | { type: "carPulledOut"; id: number; q: number; r: number; x: number; z: number }
  /** A car stands in its reserved spot (trip end), as carPulledOut. */
  | { type: "carParked"; id: number; q: number; r: number; x: number; z: number }
  | { type: "photoExported"; name: string; width: number; height: number }
  | { type: "photoExportFailed"; name: string; reason: string }
  | { type: "saved"; tag: string | null; tiles: number; bytes: number }
  | { type: "loaded"; tiles: number; version: number; settings: SavedSettings | null }
  | { type: "loadRefused"; code: LoadRefusalCode; reason: string; version: number | null };

export interface PlayTile {
  q: number;
  r: number;
  tile: string;
  rotation: number;
}

export interface Ghost {
  q: number;
  r: number;
  tile: string;
  rotation: number;
  valid: boolean;
  legal_rotations: number[];
  edges: EdgeStatus[];
}

export interface BulldozeTarget {
  q: number;
  r: number;
  allowed: boolean;
  reason: string | null;
}

export interface ActionResult {
  action: string;
  ok: boolean;
  cell: Cell | null;
  reason: string | null;
}

/** A tile mid-rise: its stage name and the timeline position `t` in seconds. */
export interface RisingTile {
  q: number;
  r: number;
  tile: string;
  stage: string;
  t: number;
}

/** A driving car in `play.car_list`: its sim id, trip phase and world XZ position in metres. */
export interface CarInfo {
  id: number;
  phase: "pulling_out" | "leaving" | "road" | "parking";
  x: number;
  z: number;
}

export interface PlayState {
  tool: Tool;
  selected: string;
  rotation: number;
  inventory_open: boolean;
  photo_mode: boolean;
  /** First-person walk mode (PRD P8): the HUD hides behind the walk hint. */
  walk_mode: boolean;
  tiles: PlayTile[];
  frontier: Cell[];
  hovered: Cell | null;
  /** The slot a click pinned: the ghost stays there while the pointer moves; null when nothing is pinned. */
  pinned: Cell | null;
  hovered_kind: HoveredKind | null;
  ghost: Ghost | null;
  inventory: string[];
  bulldoze: BulldozeTarget | null;
  can_undo: boolean;
  can_redo: boolean;
  last_result: ActionResult | null;
  recent_events: GameEvent[];
  /** Cars driving. */
  cars: number;
  /** Cars standing in house parking spots (PRD §7.1). */
  parked_cars: number;
  /** Every driving car and the part of its trip it is on. */
  car_list: CarInfo[];
  connected_destinations: number;
  spawn_rate_hz: number;
  rising: RisingTile[];
  reduced_motion: boolean;
  /** The ground point the camera orbits, in world metres, or null without a camera. */
  focus: Vec3 | null;
  /** The camera's distance from `focus` in metres (§4.5 zoom range 12 to 2,500). */
  zoom: number | null;
}

/** A photo export in flight: its name, scale and PNG size in pixels. */
export interface PhotoExportJob {
  name: string;
  scale: number;
  width: number;
  height: number;
}

/** `state().photo`: the photo panel (PRD §4.6) and what the engine is doing with it. */
export interface PhotoState {
  on: boolean;
  dof: Dof;
  /** Metres. */
  focus: number;
  /** f-number. */
  aperture: number;
  /** Stops brighter than the game camera. */
  exposure: number;
  /** Degrees. */
  fov: number;
  /** Degrees. */
  roll: number;
  pause_agents: boolean;
  agents_paused: boolean;
  /** The car sim's fixed ticks so far; it holds still while agents are paused. */
  sim_ticks: number;
  exporting: PhotoExportJob | null;
  /** The GPU's largest texture side, which caps the export size; null before the renderer starts. */
  max_texture: number | null;
}

/** Why the walker's latest move stopped. */
export type WalkBlocked = "building" | "tree" | "rock" | "water" | "edge" | "steep";

/** `state().walk`: first-person walk mode (PRD P8). Position fields are null while it is off. */
export interface WalkState {
  on: boolean;
  /** Feet on the ground, world metres. */
  feet: Vec3 | null;
  eye: Vec3 | null;
  /** Degrees clockwise viewed from above, 0 toward -Z. */
  heading: number | null;
  pitch: number | null;
  /** Ground height under the feet (m). */
  ground: number | null;
  water_depth: number | null;
  cell: Cell | null;
  tile: string | null;
  blocked: WalkBlocked | null;
  /** The collider's kit piece when `blocked` is a building, tree or rock. */
  blocked_by: string | null;
  moving: boolean;
  airborne: boolean;
  /** Metres walked since entering. */
  distance: number;
  /** Seconds left of a scripted `walk_step`. */
  step_left: number | null;
  pointer_lock: boolean;
}

/** `state().tilt`: the game camera's tilt-shift (PRD §9.1). */
export interface TiltState {
  enabled: boolean;
  strength: number;
  taps: number;
}

/** `state().tier`: the tier in force, why, and what detection found (the overlay reads these fields). */
export interface TierState {
  tier: TierName;
  source: TierSource;
  reason: string;
  /** The static rules' answer, also when a `tier` key overrides it; null before the render device exists. */
  detected: { tier: TierName; reason: string } | null;
  /** The web warm-up probe; null natively, for explicit tiers and before it decides. */
  probe: { median_ms: number; kept: number; hitches: number; tier: TierName; reason: string } | null;
  /** Tier changes applied since startup (scene rebuilds). */
  changes: number;
  /** The latest change the GPU has finished drawing; behind `changes` while a rebuild is in flight. */
  drawn: number;
  /** Quality requests the engine has handled (`set_tier`, a loaded save's quality), no-ops included. */
  requests: number;
  /** `drawn !== changes`. */
  rebuilding: boolean;
}

/** One look preset as `state().grade.presets` lists it. */
export interface GradePreset {
  name: GradeName;
  /** The engine's one-line description. */
  detail: string;
}

/** `state().grade`: the look in force, the default look, and every preset in display order. */
export interface GradeState {
  name: GradeName;
  /** The look with no `?grade=` and no stored choice (`hex_render::Grade::default()`). */
  default: GradeName;
  presets: GradePreset[];
}

/** `hexWorld.state()`: only the fields the overlay reads; the rest are documented in docs/agent-operability.md. */
export interface State {
  scenario?: string;
  play?: PlayState | null;
  photo?: PhotoState;
  walk?: WalkState;
  tilt?: TiltState;
  tier?: TierState;
  grade?: GradeState;
  /** The game camera: position and unit forward vector in world metres. */
  camera?: { pos: Vec3; forward: Vec3 } | null;
}

export type Rect = [left: number, top: number, right: number, bottom: number];

export interface HexWorld {
  ready(): boolean;
  state(): State;
  cmd(c: Command | Command[]): void;
  events(): GameEvent[];
  /** Overlay hook: every drained batch, without taking it from `events()` callers. */
  watchEvents(cb: (batch: GameEvent[]) => void): void;
  pumpEvents(): void;
  cellRect(q: number, r: number): Rect | null;
  /** Removes and returns a finished capture's PNG data URL (screenshots and photo exports). */
  takeShot(name: string): string | null;
  /** The `.hexworld` bytes of the `save` command tagged `tag`, once its `saved` event fired; null before or after. */
  takeSave(tag: string): Uint8Array | null;
  /** Validates `.hexworld` bytes and queues their load behind earlier commands. */
  load(bytes: Uint8Array): LoadResult;
  /** The world as of the last frame as `.hexworld` bytes, at once; null before the first frame. */
  saveNow(volumes: Volumes | null): Uint8Array | null;
}

// ---------- Field tables, checked against bridge-schema.json by test/bridge-sync.test.ts ----------
// `keys<T>()` takes an object literal with exactly T's keys, so a field added to or dropped from a
// type above fails to compile here until the table matches it.
type Exactly<T> = { [K in keyof Required<T>]: true };
const keys = <T>(o: Exactly<T>): string[] => Object.keys(o).sort();
type CommandOf<K extends Command["cmd"]> = Omit<Extract<Command, { cmd: K }>, "cmd">;
type EventOf<K extends GameEvent["type"]> = Omit<Extract<GameEvent, { type: K }>, "type">;

export const COMMAND_FIELDS: { [K in Command["cmd"]]: string[] } = {
  camera: keys<CommandOf<"camera">>({ pos: true, look_at: true }),
  screenshot: keys<CommandOf<"screenshot">>({ name: true }),
  wait: keys<CommandOf<"wait">>({ frames: true }),
  scenario: keys<CommandOf<"scenario">>({ name: true }),
  tilt: keys<CommandOf<"tilt">>({ on: true, strength: true }),
  dump_state: keys<CommandOf<"dump_state">>({ name: true }),
  set_tier: keys<CommandOf<"set_tier">>({ tier: true }),
  set_grade: keys<CommandOf<"set_grade">>({ grade: true }),
  set_reduced_motion: keys<CommandOf<"set_reduced_motion">>({ on: true }),
  exit: keys<CommandOf<"exit">>({}),
  select_tile: keys<CommandOf<"select_tile">>({ id: true }),
  hover: keys<CommandOf<"hover">>({ q: true, r: true }),
  pin: keys<CommandOf<"pin">>({ q: true, r: true }),
  unpin: keys<CommandOf<"unpin">>({}),
  rotate: keys<CommandOf<"rotate">>({ dir: true }),
  place: keys<CommandOf<"place">>({ q: true, r: true }),
  replace: keys<CommandOf<"replace">>({ q: true, r: true }),
  undo: keys<CommandOf<"undo">>({}),
  redo: keys<CommandOf<"redo">>({}),
  bulldoze: keys<CommandOf<"bulldoze">>({ q: true, r: true }),
  set_tool: keys<CommandOf<"set_tool">>({ mode: true }),
  toggle: keys<CommandOf<"toggle">>({ what: true }),
  photo: keys<CommandOf<"photo">>({
    dof: true,
    focus: true,
    aperture: true,
    exposure: true,
    fov: true,
    roll: true,
    pause_agents: true,
  }),
  photo_export: keys<CommandOf<"photo_export">>({ scale: true, name: true }),
  save: keys<CommandOf<"save">>({ tag: true, path: true, volumes: true }),
  load: keys<CommandOf<"load">>({ path: true }),
  new_world: keys<CommandOf<"new_world">>({}),
  walk: keys<CommandOf<"walk">>({ on: true, pointer_lock: true }),
  walk_to: keys<CommandOf<"walk_to">>({ x: true, z: true, heading: true, pitch: true }),
  walk_step: keys<CommandOf<"walk_step">>({ forward: true, right: true, seconds: true, jog: true }),
  walk_look: keys<CommandOf<"walk_look">>({ heading: true, pitch: true }),
};

export const EVENT_FIELDS: { [K in GameEvent["type"]]: string[] } = {
  tilePlaced: keys<EventOf<"tilePlaced">>({ q: true, r: true, tile: true, rotation: true }),
  placementRefused: keys<EventOf<"placementRefused">>({ q: true, r: true, tile: true, rotation: true, reason: true }),
  tileBulldozed: keys<EventOf<"tileBulldozed">>({ q: true, r: true, tile: true }),
  bulldozeRefused: keys<EventOf<"bulldozeRefused">>({ q: true, r: true, reason: true }),
  tileReplaced: keys<EventOf<"tileReplaced">>({ q: true, r: true, tile: true, rotation: true, replaced: true }),
  undone: keys<EventOf<"undone">>({}),
  redone: keys<EventOf<"redone">>({}),
  tileSelected: keys<EventOf<"tileSelected">>({ tile: true }),
  ghostRotated: keys<EventOf<"ghostRotated">>({ rotation: true, valid: true }),
  hoverChanged: keys<EventOf<"hoverChanged">>({ cell: true }),
  pinChanged: keys<EventOf<"pinChanged">>({ cell: true }),
  toolChanged: keys<EventOf<"toolChanged">>({ mode: true }),
  inventoryToggled: keys<EventOf<"inventoryToggled">>({ open: true }),
  photoModeToggled: keys<EventOf<"photoModeToggled">>({ on: true }),
  walkModeToggled: keys<EventOf<"walkModeToggled">>({ on: true }),
  actionRefused: keys<EventOf<"actionRefused">>({ action: true, reason: true }),
  riseStage: keys<EventOf<"riseStage">>({ q: true, r: true, tile: true, stage: true, skipped: true }),
  carPulledOut: keys<EventOf<"carPulledOut">>({ id: true, q: true, r: true, x: true, z: true }),
  carParked: keys<EventOf<"carParked">>({ id: true, q: true, r: true, x: true, z: true }),
  photoExported: keys<EventOf<"photoExported">>({ name: true, width: true, height: true }),
  photoExportFailed: keys<EventOf<"photoExportFailed">>({ name: true, reason: true }),
  saved: keys<EventOf<"saved">>({ tag: true, tiles: true, bytes: true }),
  loaded: keys<EventOf<"loaded">>({ tiles: true, version: true, settings: true }),
  loadRefused: keys<EventOf<"loadRefused">>({ code: true, reason: true, version: true }),
};

export const ENUMS = {
  tool: keys<Record<Tool, true>>({ place: true, bulldoze: true }),
  spin: keys<Record<Spin, true>>({ cw: true, ccw: true }),
  toggle: keys<Record<ToggleTarget, true>>({ bulldoze: true, inventory: true, photo: true }),
  edge_status: keys<Record<EdgeStatus, true>>({ Compatible: true, Incompatible: true, Neutral: true }),
  dof: keys<Record<Dof, true>>({ off: true, tilt: true, manual: true }),
  // Display order, not sorted: the schema lists the presets as the engine shows them.
  grade: [...GRADES],
};

export const STATE_FIELDS = {
  play: keys<PlayState>({
    tool: true,
    selected: true,
    rotation: true,
    inventory_open: true,
    photo_mode: true,
    walk_mode: true,
    tiles: true,
    frontier: true,
    hovered: true,
    pinned: true,
    hovered_kind: true,
    ghost: true,
    inventory: true,
    bulldoze: true,
    can_undo: true,
    can_redo: true,
    last_result: true,
    recent_events: true,
    cars: true,
    parked_cars: true,
    car_list: true,
    connected_destinations: true,
    spawn_rate_hz: true,
    rising: true,
    reduced_motion: true,
    focus: true,
    zoom: true,
  }),
  ghost: keys<Ghost>({ q: true, r: true, tile: true, rotation: true, valid: true, legal_rotations: true, edges: true }),
  bulldoze: keys<BulldozeTarget>({ q: true, r: true, allowed: true, reason: true }),
  last_result: keys<ActionResult>({ action: true, ok: true, cell: true, reason: true }),
  tile: keys<PlayTile>({ q: true, r: true, tile: true, rotation: true }),
  rising: keys<RisingTile>({ q: true, r: true, tile: true, stage: true, t: true }),
  cell: keys<Cell>({ q: true, r: true }),
  hovered_kind: keys<Record<HoveredKind, true>>({ tile: true, slot: true, empty: true }),
  photo: keys<PhotoState>({
    on: true,
    dof: true,
    focus: true,
    aperture: true,
    exposure: true,
    fov: true,
    roll: true,
    pause_agents: true,
    agents_paused: true,
    sim_ticks: true,
    exporting: true,
    max_texture: true,
  }),
  walk: keys<WalkState>({
    on: true,
    feet: true,
    eye: true,
    heading: true,
    pitch: true,
    ground: true,
    water_depth: true,
    cell: true,
    tile: true,
    blocked: true,
    blocked_by: true,
    moving: true,
    airborne: true,
    distance: true,
    step_left: true,
    pointer_lock: true,
  }),
  grade: keys<GradeState>({ name: true, default: true, presets: true }),
  grade_preset: keys<GradePreset>({ name: true, detail: true }),
};
