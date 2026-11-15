// The window.hexWorld shim between the wasm module and the HUD overlay's typed bridge
// (web/ui/src/bridge.ts). index.html loads web/ui/shim.js with defer; its IIFE registers the
// TrunkApplicationStarted listener that assembles window.hexWorld exactly as the inline script did,
// with one fix: the capture pollers read one-slot photo-event holders beside the queue, so another
// events() caller draining first no longer resolves exportPhoto with width null (ARC-003).
// The pure parts (patch merge, event queue, capture pollers) are exported for
// web/ui/test/shim.test.ts.
import type {
  Command,
  GameEvent,
  HexWorld,
  InspectResult,
  LoadResult,
  PhotoExportResult,
  Rect,
  State,
  Tool,
  ToggleTarget,
  Volumes,
} from "./bridge.ts";

/** The wasm-bindgen exports the shim drives; their names are contract-tested against
 * `bridge-schema.json`'s `exports` list (web/ui/test/bridge-sync.test.ts). */
export interface WasmBindings {
  hex_world_ready(): boolean;
  hex_world_state_json(): string;
  hex_world_diag_json(): string;
  hex_world_events(): string;
  hex_world_cell_rect(q: number, r: number, level: number, rise: number): string;
  hex_world_cmd(json: string): boolean;
  hex_world_inbox_len(): number;
  hex_world_screenshot(name: string): void;
  hex_world_take_shot(name: string): string | null | undefined;
  hex_world_take_save(tag: string): Uint8Array | null | undefined;
  hex_world_save_now(volumes?: Float32Array): Uint8Array | null | undefined;
  hex_world_load(bytes: Uint8Array): string;
  hex_world_inspect(bytes: Uint8Array): string;
}

/** Applies one `hex_world_state_json()` patch to the cached state: `patch.state`'s keys replace
 * cached ones, and a non-empty `patch.play` merges onto the cached `play`, whose keys the patch
 * omits keep their values. The expression order is the inline script's, byte for byte. */
export function mergePatch(
  state: { play?: Record<string, unknown> } & Record<string, unknown>,
  patch: { state: Record<string, unknown>; play: Record<string, unknown> },
): void {
  Object.assign(state, patch.state);
  if (state.play && Object.keys(patch.play).length) {
    state.play = Object.assign({}, state.play, patch.play);
  }
}

type Watcher = (batch: GameEvent[]) => void;
type PhotoExported = Extract<GameEvent, { type: "photoExported" }>;
type PhotoExportFailed = Extract<GameEvent, { type: "photoExportFailed" }>;

/** The game-event queue drained from `hex_world_events()`: capped at 256 with the oldest dropped,
 * and every batch shown to the watchers. The two photo slots sit beside the queue and are filled
 * when the event enters it, so a capture poller reads its event even when another `events()`
 * caller drained the queue first. */
export class EventQueue {
  private queued: GameEvent[] = [];
  private watchers: Watcher[] = [];
  private exported: PhotoExported | null = null;
  private failed: PhotoExportFailed | null = null;

  /** Feeds one wasm batch (the parsed `hex_world_events()` array) and returns it. */
  push(batch: GameEvent[]): GameEvent[] {
    if (!batch.length) return batch;
    this.queued = this.queued.concat(batch).slice(-256);
    for (const e of batch) {
      if (e.type === "photoExported") this.exported = e;
      if (e.type === "photoExportFailed") this.failed = e;
    }
    for (const watch of this.watchers) watch(batch);
    return batch;
  }

  /** Parses one wasm batch. */
  pushJson(json: string): GameEvent[] {
    return this.push(JSON.parse(json) as GameEvent[]);
  }

  /** The queued events, oldest first; the photo slots stay for the pollers. */
  drain(): GameEvent[] {
    const out = this.queued;
    this.queued = [];
    return out;
  }

  get length(): number {
    return this.queued.length;
  }

  /** Overlay hook: every batch, without taking it from `events()` callers. */
  watch(watch: Watcher): void {
    this.watchers.push(watch);
  }

  /** The last `photoExported` event for `name` (any name when omitted), drained or not. */
  photoExported(name?: string): { width: number; height: number } | null {
    const e = this.exported;
    return e && (!name || e.name === name) ? { width: e.width, height: e.height } : null;
  }

  /** The last `photoExportFailed` event for `name` (any name when omitted). */
  photoExportFailed(name?: string): { reason: string } | null {
    const e = this.failed;
    return e && (!name || e.name === name) ? { reason: e.reason } : null;
  }
}

/** Injectable dependencies of the capture pollers, so the tests run them without a browser. */
export interface ShotDeps {
  /** Millisecond clock (`Date.now` at runtime). */
  now(): number;
  /** Schedules the next poll tick (`setTimeout` at runtime). */
  schedule(tick: () => void, ms: number): void;
}

/** Sends `photo_export` and resolves `{name,width,height,url}` once the offscreen PNG lands;
 * rejects with the engine's reason on `photoExportFailed`, or past the deadline. Reads the
 * one-slot photo holder, not the shared queue, so a concurrent `events()` caller cannot starve it. */
export function exportPhoto(
  deps: {
    cmd(c: Record<string, unknown>): void;
    /** Drains wasm events into the queue (fills the photo holders); each poll tick pumps first. */
    pump(): void;
    takeShot(name: string): string | null | undefined;
    events: EventQueue;
  } & ShotDeps,
  scale?: number,
  name?: string,
  timeoutMs?: number,
): Promise<PhotoExportResult> {
  const shot = name || "hex-world-" + deps.now();
  const deadline = deps.now() + (timeoutMs || 60000);
  deps.cmd({ cmd: "photo_export", scale: scale || 1, name: shot });
  return new Promise((resolve, reject) => {
    const poll = () => {
      deps.pump();
      const url = deps.takeShot(shot);
      if (url) {
        const ev = deps.events.photoExported(shot);
        return resolve({ name: shot, width: ev ? ev.width : null, height: ev ? ev.height : null, url });
      }
      const bad = deps.events.photoExportFailed(shot);
      if (bad) return reject(new Error(bad.reason));
      if (deps.now() > deadline) return reject(new Error("hexWorld.exportPhoto timed out: " + shot));
      deps.schedule(poll, 100);
    };
    poll();
  });
}

/** Sends `screenshot` and resolves the PNG data URL once the capture lands, after any queued
 * commands; rejects past the deadline. */
export function screenshot(
  deps: { screenshot(name: string): void; takeShot(name: string): string | null | undefined } & ShotDeps,
  name?: string,
  timeoutMs?: number,
): Promise<string> {
  const shot = name || "shot-" + deps.now();
  const deadline = deps.now() + (timeoutMs || 10000);
  deps.screenshot(shot);
  return new Promise((resolve, reject) => {
    const poll = () => {
      const url = deps.takeShot(shot);
      if (url) return resolve(url);
      if (deps.now() > deadline) return reject(new Error("hexWorld.screenshot timed out: " + shot));
      deps.schedule(poll, 100);
    };
    poll();
  });
}

/** Assembles window.hexWorld from the wasm bindings (the TrunkApplicationStarted handler). */
export function installShim(w: WasmBindings): void {
  const events = new EventQueue();
  const pump = () => events.pushJson(w.hex_world_events());

  // The cached engine state; queue counts are call-local overlays, never accumulated into it.
  const latestState: Record<string, unknown> = {};
  const state = (): State => {
    const json = w.hex_world_state_json();
    if (json) mergePatch(latestState, JSON.parse(json));
    const s: Record<string, unknown> = Object.assign({}, latestState);
    if (typeof s.events_pending === "number") s.events_pending += events.length;
    if (typeof s.queued === "number") s.queued += w.hex_world_inbox_len();
    return s as State;
  };

  // One command object or an array; parse errors are logged with console.error by wasm.
  const cmd = (c: string | Record<string, unknown> | Record<string, unknown>[]): void => {
    w.hex_world_cmd(typeof c === "string" ? c : JSON.stringify(c));
  };
  const shotDeps = {
    takeShot: (name: string) => w.hex_world_take_shot(name),
    now: Date.now,
    schedule: (tick: () => void, ms: number) => setTimeout(tick, ms),
  };

  const hw: HexWorld = {
    ready: () => w.hex_world_ready(),
    state,
    diag: () => JSON.parse(w.hex_world_diag_json()) as Record<string, unknown>,
    dumpState: state,
    cmd,
    // Drains queued game events (tilePlaced, placementRefused, undone, ...), oldest first. The
    // UI overlay sees every batch through watchEvents without taking events from callers here.
    events: () => {
      pump();
      return events.drain();
    },
    watchEvents: (cb) => events.watch(cb),
    pumpEvents: pump,
    // [left, top, right, bottom] in CSS px around cell (q, r) for the current camera, or null.
    cellRect: (q, r, level, rise) => JSON.parse(w.hex_world_cell_rect(q, r, level || 0, rise || 0)) as Rect | null,
    // Removes and returns a finished capture's PNG data URL (a screenshot or a photo export).
    takeShot: (name) => w.hex_world_take_shot(name) || null,
    // Saves (PRD §12.2): cmd({cmd:"save",tag}) encodes after earlier commands and fires `saved`;
    // takeSave(tag) then returns the .hexworld bytes (null before or after). load(bytes)
    // validates a Uint8Array at once and queues it; inspect decodes without loading.
    takeSave: (tag) => w.hex_world_take_save(tag) || null,
    // The world as of the last frame, encoded at once, for a page hide that cannot wait a frame.
    saveNow: (v?: Volumes | null) =>
      w.hex_world_save_now(v ? new Float32Array([v.master, v.music, v.sfx, v.ambience, v.ui]) : undefined) || null,
    load: (bytes) => JSON.parse(w.hex_world_load(bytes)) as LoadResult,
    inspect: (bytes) => JSON.parse(w.hex_world_inspect(bytes)) as InspectResult,
    newWorld: () => cmd({ cmd: "new_world" }),
    // Play-scenario actions: typed wrappers over cmd, queued like any other command.
    selectTile: (id) => cmd({ cmd: "select_tile", id }),
    hover: (q, r) => cmd({ cmd: "hover", q, r }),
    rotateGhost: (dir) => cmd({ cmd: "rotate", dir: dir || "cw" }),
    placeTile: (q, r) => cmd({ cmd: "place", q, r }),
    replaceTile: (q, r) => cmd({ cmd: "replace", q, r }),
    pin: (q, r) => cmd({ cmd: "pin", q, r }),
    unpin: () => cmd({ cmd: "unpin" }),
    undo: () => cmd({ cmd: "undo" }),
    redo: () => cmd({ cmd: "redo" }),
    bulldoze: (q, r) => cmd({ cmd: "bulldoze", q, r }),
    setTool: (mode) => cmd({ cmd: "set_tool", mode }),
    toggle: (what: ToggleTarget) => cmd({ cmd: "toggle", what }),
    // Photo mode (PRD §4.6): panel values ({ dof, focus, aperture, exposure, fov, roll, pause_agents }).
    photo: (values: Partial<Extract<Command, { cmd: "photo" }>>) => cmd(Object.assign({ cmd: "photo" }, values)),
    // Resolves to { name, width, height, url } once the offscreen PNG lands, or rejects with the reason.
    exportPhoto: (scale, name, timeoutMs) => exportPhoto({ cmd, pump, events, ...shotDeps }, scale, name, timeoutMs),
    // Resolves to a PNG data URL once the capture lands, after any queued commands.
    screenshot: (name, timeoutMs) =>
      screenshot({ screenshot: (n) => w.hex_world_screenshot(n), ...shotDeps }, name, timeoutMs),
  };
  window.hexWorld = hw;

  if (new URLSearchParams(location.search).get("diag") === "1") {
    const el = document.getElementById("diag");
    if (el) {
      el.style.display = "block";
      setInterval(() => {
        el.textContent = JSON.stringify(window.hexWorld?.diag(), null, 1);
      }, 500);
    }
  }
}

// Auto-install when a DOM is present (the page); a bare node import (the tests) gets only the
// pure parts above.
if (typeof document !== "undefined") {
  addEventListener("TrunkApplicationStarted", () => {
    const w = (window as { wasmBindings?: WasmBindings }).wasmBindings;
    if (!w) {
      console.error("hexWorld: wasmBindings missing");
      return;
    }
    installShim(w);
  });
}
