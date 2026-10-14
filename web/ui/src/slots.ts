// Saved worlds in this browser (PRD D9, §12.2): named slots, each holding a world's .hexworld bytes
// (the same bytes as a file export) beside a small summary for the Worlds sheet. Every storage call
// is wrapped: a private window, blocked site data or a stalled open gives no store at all, and the
// game plays on with autosave off.
import type { PlayTile, SocketKind } from "./bridge.ts";
import { rotated, tileDef } from "./rules.ts";

/** IndexedDB layout: one database, a summary store for the list and a byte store loaded on demand. */
export const DB_NAME = "hex-world";
export const DB_VERSION = 1;
export const META_STORE = "slots";
export const DATA_STORE = "saves";
/** A blocked or stalled open never fires; past this the store counts as unavailable. */
export const OPEN_TIMEOUT_MS = 3000;
/** The thumbnail keeps at most this many tiles (the nearest to the start tile). */
export const MAP_LIMIT = 400;

/** Socket-class counts over every tile edge, in the Legend's edge-colour order (G, F, R, V, B, O). */
export type Mix = Partial<Record<SocketKind, number>>;
/** A tile in the thumbnail map: cell, tile id, rotation. */
export type MapTile = [q: number, r: number, tile: string, rotation: number];

export interface SlotMeta {
  id: string;
  name: string;
  tiles: number;
  /** Unix time in milliseconds of the last write. */
  updatedAt: number;
  /** Size of the .hexworld bytes. */
  bytes: number;
  mix: Mix;
  map: MapTile[];
}

export interface SlotBackend {
  list(): Promise<SlotMeta[]>;
  read(id: string): Promise<Uint8Array | null>;
  write(meta: SlotMeta, data: Uint8Array | null): Promise<void>;
  remove(id: string): Promise<void>;
}

export const KIND_ORDER: SocketKind[] = ["Grass", "Forest", "Road", "River", "Beach", "Ocean"];

export function mixOf(tiles: readonly PlayTile[]): Mix {
  const mix: Mix = {};
  for (const t of tiles) {
    const def = tileDef(t.tile);
    if (!def) continue;
    for (const k of rotated(def, t.rotation)) mix[k] = (mix[k] ?? 0) + 1;
  }
  return mix;
}

export function mapOf(tiles: readonly PlayTile[]): MapTile[] {
  const d = (t: PlayTile) => (Math.abs(t.q) + Math.abs(t.r) + Math.abs(t.q + t.r)) / 2;
  return [...tiles]
    .sort((a, b) => d(a) - d(b))
    .slice(0, MAP_LIMIT)
    .map((t) => [t.q, t.r, t.tile, t.rotation]);
}

/** The first "World n" not taken. */
export function nextName(taken: readonly string[]): string {
  const used = new Set(taken.map((n) => n.trim().toLowerCase()));
  for (let i = 1; ; i++) if (!used.has(`world ${i}`)) return `World ${i}`;
}

export const MAX_NAME = 40;

/** A typed name made safe to show and store: trimmed, single-spaced, at most 40 characters. */
export function cleanName(raw: string): string {
  return raw.replace(/\s+/g, " ").trim().slice(0, MAX_NAME).trim();
}

/** `<world name>.hexworld`, with characters file systems refuse replaced. */
export function fileName(name: string): string {
  const base = cleanName(name)
    .replace(/[\\/:*?"<>|]/g, "-")
    .replace(/^\.+/, "");
  return `${base || "world"}.hexworld`;
}

/** The world name an imported file suggests: its name without the extension. */
export function nameFromFile(file: string): string {
  return cleanName(file.replace(/\.hexworld$/i, "").replace(/[_]+/g, " ")) || "Imported world";
}

/** Newest first. */
export const byRecent = (list: readonly SlotMeta[]): SlotMeta[] => [...list].sort((a, b) => b.updatedAt - a.updatedAt);

export function newId(now: number, rand: () => number = Math.random): string {
  return `w${now.toString(36)}${Math.floor(rand() * 36 ** 4)
    .toString(36)
    .padStart(4, "0")}`;
}

const isMeta = (v: unknown): v is SlotMeta => {
  const m = v as SlotMeta;
  return (
    !!m &&
    typeof m.id === "string" &&
    typeof m.name === "string" &&
    typeof m.tiles === "number" &&
    typeof m.updatedAt === "number"
  );
};

/** In-memory slots: the unit tests' backend. */
export class MemoryBackend implements SlotBackend {
  readonly meta = new Map<string, SlotMeta>();
  readonly data = new Map<string, Uint8Array>();
  async list(): Promise<SlotMeta[]> {
    return [...this.meta.values()].map((m) => ({ ...m }));
  }
  async read(id: string): Promise<Uint8Array | null> {
    return this.data.get(id) ?? null;
  }
  async write(meta: SlotMeta, data: Uint8Array | null): Promise<void> {
    this.meta.set(meta.id, { ...meta });
    if (data) this.data.set(meta.id, data);
  }
  async remove(id: string): Promise<void> {
    this.meta.delete(id);
    this.data.delete(id);
  }
}

const done = <T>(req: IDBRequest<T>): Promise<T> =>
  new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

const committed = (tx: IDBTransaction): Promise<void> =>
  new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("transaction aborted"));
  });

/** IndexedDB slots. A write puts the summary and the bytes in one transaction. */
export class IdbBackend implements SlotBackend {
  private readonly db: IDBDatabase;
  constructor(db: IDBDatabase) {
    this.db = db;
  }
  async list(): Promise<SlotMeta[]> {
    const all = await done(this.db.transaction(META_STORE).objectStore(META_STORE).getAll());
    return (all as unknown[]).filter(isMeta);
  }
  async read(id: string): Promise<Uint8Array | null> {
    const v = await done(this.db.transaction(DATA_STORE).objectStore(DATA_STORE).get(id));
    return v instanceof Uint8Array ? v : v instanceof ArrayBuffer ? new Uint8Array(v) : null;
  }
  async write(meta: SlotMeta, data: Uint8Array | null): Promise<void> {
    const tx = this.db.transaction([META_STORE, DATA_STORE], "readwrite");
    tx.objectStore(META_STORE).put(meta);
    if (data) tx.objectStore(DATA_STORE).put(data, meta.id);
    // Commit now instead of when the event loop next idles: a page hide may not get that far.
    tx.commit?.();
    await committed(tx);
  }
  async remove(id: string): Promise<void> {
    const tx = this.db.transaction([META_STORE, DATA_STORE], "readwrite");
    tx.objectStore(META_STORE).delete(id);
    tx.objectStore(DATA_STORE).delete(id);
    await committed(tx);
  }
}

/**
 * Opens the slot database, or resolves null when storage is unavailable: no IndexedDB, an open
 * that throws (blocked site data), errors (private windows), is blocked, or never answers.
 */
export function openSlots(factory: IDBFactory | undefined, timeoutMs = OPEN_TIMEOUT_MS): Promise<SlotBackend | null> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (b: SlotBackend | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(b);
    };
    const timer = setTimeout(() => finish(null), timeoutMs);
    try {
      if (!factory) return finish(null);
      const req = factory.open(DB_NAME, DB_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        if (!db.objectStoreNames.contains(META_STORE)) db.createObjectStore(META_STORE, { keyPath: "id" });
        if (!db.objectStoreNames.contains(DATA_STORE)) db.createObjectStore(DATA_STORE);
      };
      req.onsuccess = () => {
        if (settled) return void req.result.close();
        finish(new IdbBackend(req.result));
      };
      req.onerror = () => finish(null);
      req.onblocked = () => finish(null);
    } catch {
      finish(null);
    }
  });
}

/** `window.indexedDB`, or undefined when even reading it throws. */
export function idbFactory(): IDBFactory | undefined {
  try {
    return globalThis.indexedDB ?? undefined;
  } catch {
    return undefined;
  }
}

/**
 * Coalesces a burst of pokes into one call, `ms` after the last poke. `flush` runs a pending call
 * at once (before switching worlds or when the page hides).
 */
export class Debouncer {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly ms: number;
  private readonly fn: () => void;
  constructor(ms: number, fn: () => void) {
    this.ms = ms;
    this.fn = fn;
  }
  get pending(): boolean {
    return this.timer !== null;
  }
  poke(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.fn();
    }, this.ms);
  }
  /** Runs the pending call now; returns whether there was one. */
  flush(): boolean {
    if (this.timer === null) return false;
    clearTimeout(this.timer);
    this.timer = null;
    this.fn();
    return true;
  }
  cancel(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
