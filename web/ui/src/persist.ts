// Autosave, saved worlds and .hexworld files (PRD D9, §12.2). The engine encodes and validates; this
// keeps the current world's name and slot, autosaves after every placement, bulldoze, undo and redo
// (debounced), resumes the most recent slot on start, and moves bytes between the bridge, IndexedDB
// and files. With no storage the game plays on and the bar says autosave is off.
import type { Command, GameEvent, LoadRefusalCode, LoadResult, PlayTile, Volumes } from "./bridge.ts";
import {
  Debouncer,
  type MapTile,
  type Mix,
  type SlotBackend,
  type SlotMeta,
  byRecent,
  cleanName,
  fileName,
  mapOf,
  mixOf,
  nameFromFile,
  newId,
  nextName,
} from "./slots.ts";

export type SaveState = "saved" | "saving" | "off";

export const AUTOSAVE_MS = 600;

/** A toast request: the HUD's margin note. */
export interface Note {
  text: string;
  sub?: string;
  icon: string;
  tone?: "ok" | "bad" | "caution";
  action?: string;
  onAction?: () => void;
}

/** The bridge calls persistence needs (window.hexWorld). */
export interface PersistHost {
  cmd(c: Command): void;
  takeSave(tag: string): Uint8Array | null;
  load(bytes: Uint8Array): LoadResult;
  /** The world as of the last frame, encoded at once (a page hide cannot wait for a frame). */
  saveNow(volumes: Volumes | null): Uint8Array | null;
}

export interface PersistOptions {
  now?: () => number;
  debounceMs?: number;
  /** The audio layer's bus levels, sent with every save. */
  volumes?: () => Volumes | null;
  download?: (name: string, bytes: Uint8Array) => void;
  notify?: (n: Note) => void;
  /** Called whenever the name, the save state or the slot list changes. */
  changed?: () => void;
}

/** The world on screen: its slot id, its name, and whether its slot exists in storage yet. */
export interface CurrentWorld {
  id: string;
  name: string;
  stored: boolean;
}

interface Switch extends CurrentWorld {
  /** Write the slot as soon as the world is up (an import). */
  saveNow: boolean;
}

interface PendingSave {
  world: CurrentWorld;
  /** `copy` writes a duplicate's slot and leaves the save state alone. */
  purpose: "autosave" | "export" | "copy";
  tiles: PlayTile[];
}

/** Plain words for a refused file: a headline and what to do. */
export function refusalText(code: LoadRefusalCode, reason: string): { text: string; sub: string } {
  switch (code) {
    case "bad_magic":
      return {
        text: "This isn't a Hex World file",
        sub: "It doesn't start with the .hexworld header. Choose a file exported from Hex World.",
      };
    case "newer_version":
      return {
        text: "Made by a newer Hex World",
        sub: "Reload the page to get the newest Hex World, then import it again.",
      };
    case "unsupported_version":
      return { text: "This file's format isn't supported", sub: reason };
    case "truncated":
      return { text: "This file is cut short", sub: "It may not have finished downloading. Export it again." };
    case "trailing_bytes":
      return {
        text: "This file has extra data after its end",
        sub: "It may have been joined with another file. Export it again.",
      };
    case "checksum":
      return { text: "This file is damaged", sub: "Its checksum doesn't match its contents. Export it again." };
    case "too_large":
      return { text: "This file is too large to be a world", sub: "Hex World files are under 16 MB." };
    case "io":
    case "no_file":
      return { text: "Couldn't read the file", sub: reason };
    default:
      return { text: "This world can't be opened", sub: reason };
  }
}

export function stateWords(s: SaveState): string {
  return s === "saved" ? "Saved" : s === "saving" ? "Saving…" : "Autosave off";
}

/** "12 s ago", "4 min ago", "Yesterday, 21:40", "11 Oct, 18:02". */
export function whenText(t: number, now: number): string {
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return `${s} s ago`;
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  const d = new Date(t);
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(d)) / 86_400_000);
  if (days === 0) return `Today, ${hm}`;
  if (days === 1) return `Yesterday, ${hm}`;
  return `${d.getDate()} ${d.toLocaleString("en-GB", { month: "short" })}, ${hm}`;
}

export class Persistence {
  current: CurrentWorld;
  state: SaveState;
  /** Every stored slot, newest first. */
  slots: SlotMeta[] = [];
  /** When the current world was last written. */
  savedAt: number | null = null;
  /** Set once the start-up resume has settled; autosave waits for it. */
  ready = false;

  private store: SlotBackend | null;
  private readonly host: PersistHost;
  private readonly opts: PersistOptions;
  private readonly now: () => number;
  private readonly debounce: Debouncer;
  private tiles: PlayTile[] = [];
  private seq = 0;
  private readonly pending = new Map<string, PendingSave>();
  private readonly switches: Switch[] = [];
  private writing = 0;
  private lastStamp = 0;

  constructor(host: PersistHost, store: SlotBackend | null, opts: PersistOptions = {}) {
    this.host = host;
    this.store = store;
    this.opts = opts;
    this.now = opts.now ?? Date.now;
    this.debounce = new Debouncer(opts.debounceMs ?? AUTOSAVE_MS, () => this.autosave());
    this.current = { id: newId(this.now()), name: nextName([]), stored: false };
    this.state = store ? "saved" : "off";
  }

  get storageOn(): boolean {
    return this.store !== null;
  }

  /** The world on screen, from the latest play state. */
  get tilesNow(): number {
    return this.tiles.length;
  }
  get mixNow(): Mix {
    return mixOf(this.tiles);
  }
  get mapNow(): MapTile[] {
    return mapOf(this.tiles);
  }

  /** Resumes the most recent slot, or keeps the start world under a new name. */
  async start(): Promise<void> {
    if (!this.store) {
      this.ready = true;
      this.opts.notify?.({
        text: "Autosave is off in this window",
        sub: "This browser blocks storage here. Export keeps your world.",
        icon: "alert",
        tone: "caution",
        action: "Export",
        onAction: () => this.exportCurrent(),
      });
      this.changed();
      return;
    }
    await this.refresh();
    this.current = { id: newId(this.now()), name: nextName(this.slots.map((s) => s.name)), stored: false };
    const last = this.slots[0];
    if (!last || !(await this.open(last, false))) this.ready = true;
    this.changed();
  }

  /** The latest play tiles, read every frame (for each save's summary). */
  observe(tiles: PlayTile[]): void {
    this.tiles = tiles;
  }

  onEvents(batch: readonly GameEvent[]): void {
    for (const e of batch) {
      switch (e.type) {
        case "tilePlaced":
        case "tileReplaced":
        case "tileBulldozed":
        case "undone":
        case "redone":
          this.dirty();
          break;
        case "saved":
          this.onSaved(e.tag);
          break;
        case "loaded":
          this.onLoaded();
          break;
        default:
          break;
      }
    }
  }

  private dirty(): void {
    if (!this.store || !this.ready) return;
    this.setState("saving");
    this.debounce.poke();
  }

  /** A setting the save carries (quality, tilt-shift) changed: autosave as after an edit. */
  settingsChanged(): void {
    this.dirty();
  }

  /** Queues any pending autosave now, ahead of whatever is sent next (a world switch). */
  flush(): void {
    this.debounce.flush();
  }

  /**
   * The page is hiding (`visibilitychange` to hidden, or `pagehide`): a pending autosave is written
   * now from the engine's last frame, because a queued save needs a frame that a hidden page may
   * never run. The IndexedDB write starts before this returns; finishing it is best effort.
   */
  flushNow(): boolean {
    const waiting = this.debounce.pending || [...this.pending.values()].some((p) => p.purpose === "autosave");
    if (!this.store || !this.ready || !waiting) return false;
    const bytes = this.host.saveNow(this.opts.volumes?.() ?? null);
    if (!bytes) return false;
    this.debounce.cancel();
    void this.write({ world: { ...this.current }, purpose: "autosave", tiles: this.tiles }, bytes);
    return true;
  }

  private autosave(): void {
    if (this.store) this.requestSave(this.current, "autosave");
  }

  private requestSave(world: CurrentWorld, purpose: PendingSave["purpose"]): void {
    const tag = `${purpose}-${++this.seq}`;
    this.pending.set(tag, { world: { ...world }, purpose, tiles: this.tiles });
    if (purpose === "autosave") this.setState("saving");
    this.host.cmd({ cmd: "save", tag, path: null, volumes: this.opts.volumes?.() ?? null });
  }

  exportCurrent(): void {
    this.requestSave(this.current, "export");
  }

  private onSaved(tag: string | null): void {
    const p = tag ? this.pending.get(tag) : undefined;
    if (!p || !tag) return;
    this.pending.delete(tag);
    const bytes = this.host.takeSave(tag);
    if (!bytes) return;
    if (p.purpose === "export") {
      const name = fileName(p.world.name);
      this.opts.download?.(name, bytes);
      this.opts.notify?.({
        text: `Exported ${name}`,
        sub: `${kb(bytes.length)}, in your downloads`,
        icon: "export",
        tone: "ok",
      });
      return;
    }
    void this.write(p, bytes);
  }

  private async write(p: PendingSave, bytes: Uint8Array): Promise<void> {
    const store = this.store;
    if (!store) return;
    const meta: SlotMeta = {
      id: p.world.id,
      name: p.world.name,
      tiles: p.tiles.length,
      updatedAt: p.purpose === "copy" ? this.stampBefore(this.savedAt ?? this.now()) : this.stamp(),
      bytes: bytes.length,
      mix: mixOf(p.tiles),
      map: mapOf(p.tiles),
    };
    this.writing++;
    try {
      await store.write(meta, bytes);
      if (this.current.id === meta.id) {
        this.current.stored = true;
        this.savedAt = meta.updatedAt;
      }
      this.upsert(meta);
    } catch {
      this.storageLost();
    } finally {
      this.writing--;
    }
    if (
      this.store &&
      !this.debounce.pending &&
      !this.writing &&
      ![...this.pending.values()].some((x) => x.purpose === "autosave")
    )
      this.setState("saved");
    this.changed();
  }

  private storageLost(): void {
    if (!this.store) return;
    this.store = null;
    this.debounce.cancel();
    this.setState("off");
    this.opts.notify?.({
      text: "Autosave stopped",
      sub: "This browser refused to store the world. Export keeps it.",
      icon: "alert",
      tone: "caution",
      action: "Export",
      onAction: () => this.exportCurrent(),
    });
  }

  /**
   * A world came up. Its saved volumes are not applied: the audio layer keeps this viewer's own
   * levels (hexWorld.audio, in localStorage), which a world from another browser must not override.
   */
  private onLoaded(): void {
    const sw = this.switches.shift() ?? {
      id: newId(this.now()),
      name: nextName(this.slots.map((s) => s.name)),
      stored: false,
      saveNow: false,
    };
    this.current = { id: sw.id, name: sw.name, stored: sw.stored };
    const meta = this.slots.find((s) => s.id === sw.id);
    this.savedAt = meta?.updatedAt ?? null;
    if (meta && this.store) {
      // The world on screen is the most recent, so the next start resumes it. Stamped after any
      // autosave the switch flushed (its saved event comes first), never level with it.
      const touched = { ...meta, updatedAt: this.stamp() };
      this.upsert(touched);
      this.store.write(touched, null).catch(() => this.storageLost());
    }
    this.ready = true;
    this.setState(this.store ? "saved" : "off");
    // An import is written as soon as the HUD has seen its tiles (the debounce outlasts a frame).
    if (sw.saveNow && this.store) {
      this.setState("saving");
      this.debounce.poke();
    }
    this.changed();
  }

  /** Loads a stored slot (the Load button). Returns false when it could not be opened. */
  async loadSlot(id: string): Promise<boolean> {
    const meta = this.slots.find((s) => s.id === id);
    if (!meta || id === this.current.id) return false;
    return this.open(meta, true);
  }

  private async open(meta: SlotMeta, flush: boolean): Promise<boolean> {
    let bytes: Uint8Array | null = null;
    try {
      bytes = (await this.store?.read(meta.id)) ?? null;
    } catch {
      bytes = null;
    }
    if (!bytes) {
      this.opts.notify?.({
        text: `Couldn't read ${meta.name}`,
        sub: "Its saved bytes are missing from this browser.",
        icon: "alert",
        tone: "bad",
      });
      return false;
    }
    if (flush) this.flush();
    const r = this.host.load(bytes);
    if (!r.ok) {
      const w = refusalText(r.code, r.reason);
      this.opts.notify?.({ text: `${meta.name} can't be opened`, sub: w.text, icon: "alert", tone: "bad" });
      return false;
    }
    this.switches.push({ id: meta.id, name: meta.name, stored: true, saveNow: false });
    return true;
  }

  /** Downloads a stored slot's bytes as `<name>.hexworld`, without loading it. */
  async exportSlot(id: string): Promise<void> {
    const meta = this.slots.find((s) => s.id === id);
    let bytes: Uint8Array | null = null;
    try {
      bytes = meta ? ((await this.store?.read(id)) ?? null) : null;
    } catch {
      bytes = null;
    }
    if (!meta || !bytes) {
      this.opts.notify?.({
        text: "Couldn't export that world",
        sub: "Its saved bytes are missing from this browser.",
        icon: "alert",
        tone: "bad",
      });
      return;
    }
    const name = fileName(meta.name);
    this.opts.download?.(name, bytes);
    this.opts.notify?.({
      text: `Exported ${name}`,
      sub: `${kb(bytes.length)}, in your downloads`,
      icon: "export",
      tone: "ok",
    });
  }

  /** Copies the current world, or a stored one, to a new slot named "<name> copy". */
  async duplicate(id: string): Promise<void> {
    if (!this.store) return;
    const name = this.unique(
      `${(id === this.current.id ? this.current.name : this.slots.find((s) => s.id === id)?.name) ?? "World"} copy`,
    );
    const world = { id: newId(this.now()), name, stored: false };
    if (id === this.current.id) {
      this.requestSave(world, "copy");
    } else {
      const meta = this.slots.find((s) => s.id === id);
      let bytes: Uint8Array | null = null;
      try {
        bytes = meta ? await this.store.read(id) : null;
      } catch {
        bytes = null;
      }
      if (!meta || !bytes) return;
      const copy = { ...meta, id: world.id, name, updatedAt: this.stampBefore(meta.updatedAt) };
      try {
        await this.store.write(copy, bytes);
      } catch {
        this.storageLost();
        return;
      }
      this.upsert(copy);
      this.changed();
    }
    this.opts.notify?.({ text: `Duplicated as ${name}`, icon: "check", tone: "ok" });
  }

  notifyLoaded(name: string): void {
    this.opts.notify?.({ text: `Loaded ${name}`, icon: "check", tone: "ok" });
  }

  /** Starts a fresh world from the start tile under the next free name. */
  newWorld(): void {
    this.flush();
    this.host.cmd({ cmd: "new_world" });
    this.switches.push({ id: newId(this.now()), name: nextName(this.allNames()), stored: false, saveNow: false });
  }

  /**
   * Clears the world on screen back to the start tile in place (the engine's `new_world`, as New
   * world sends): the same name and slot, no undo history, autosaved once the HUD has seen the
   * cleared tiles. Returns false before the start-up resume has settled.
   */
  startOver(): boolean {
    if (!this.ready) return false;
    this.flush();
    this.host.cmd({ cmd: "new_world" });
    this.switches.push({ ...this.current, saveNow: true });
    this.opts.notify?.({ text: `Started ${this.current.name} over`, icon: "check", tone: "ok" });
    return true;
  }

  /** Imports a .hexworld file as a new world. A refused file gets a toast saying why. */
  importFile(file: string, bytes: Uint8Array): boolean {
    this.flush();
    const r = this.host.load(bytes);
    if (!r.ok) {
      const w = refusalText(r.code, r.reason);
      this.opts.notify?.({ text: w.text, sub: `${file}: ${w.sub}`, icon: "alert", tone: "bad" });
      return false;
    }
    const name = this.unique(nameFromFile(file));
    this.switches.push({ id: newId(this.now()), name, stored: false, saveNow: true });
    this.opts.notify?.({
      text: `Imported ${name}`,
      sub: `${r.tiles} ${r.tiles === 1 ? "tile" : "tiles"}, opened as a new world`,
      icon: "check",
      tone: "ok",
    });
    return true;
  }

  /** Renames a world; an empty name keeps the old one. */
  async rename(id: string, raw: string): Promise<void> {
    const name = cleanName(raw);
    if (!name) return;
    if (id === this.current.id) this.current.name = name;
    const meta = this.slots.find((s) => s.id === id);
    if (meta && this.store) {
      const next = { ...meta, name };
      this.upsert(next, false);
      try {
        await this.store.write(next, null);
      } catch {
        this.storageLost();
      }
    }
    this.changed();
  }

  /** Deletes a stored slot other than the current world. */
  async remove(id: string): Promise<boolean> {
    if (id === this.current.id || !this.store) return false;
    try {
      await this.store.remove(id);
    } catch {
      this.storageLost();
      return false;
    }
    this.slots = this.slots.filter((s) => s.id !== id);
    this.changed();
    return true;
  }

  async refresh(): Promise<void> {
    if (!this.store) return;
    try {
      this.slots = byRecent(await this.store.list());
    } catch {
      this.storageLost();
    }
  }

  private upsert(meta: SlotMeta, resort = true): void {
    const rest = this.slots.filter((s) => s.id !== meta.id);
    const i = this.slots.findIndex((s) => s.id === meta.id);
    if (!resort && i >= 0) {
      this.slots[i] = meta;
      return;
    }
    this.slots = byRecent([meta, ...rest]);
  }

  private allNames(): string[] {
    return [...this.slots.map((s) => s.name), this.current.name, ...this.switches.map((s) => s.name)];
  }

  private unique(name: string): string {
    const taken = new Set(this.allNames().map((n) => n.toLowerCase()));
    if (!taken.has(name.toLowerCase())) return name;
    for (let i = 2; ; i++) {
      const n = `${name} ${i}`;
      if (!taken.has(n.toLowerCase())) return n;
    }
  }

  /** A copy sorts just under its source, so it never becomes the world the next start resumes. */
  private stampBefore(t: number): number {
    return t - 1;
  }

  /** A strictly increasing write time, so two writes in one millisecond still order. */
  private stamp(): number {
    this.lastStamp = Math.max(this.now(), this.lastStamp + 1);
    return this.lastStamp;
  }

  private setState(s: SaveState): void {
    if (!this.store) s = "off";
    if (this.state === s) return;
    this.state = s;
    this.changed();
  }

  private changed(): void {
    this.opts.changed?.();
  }
}

export const kb = (n: number): string => (n < 1024 ? `${n} B` : `${Math.max(1, Math.round(n / 1024))} KB`);
